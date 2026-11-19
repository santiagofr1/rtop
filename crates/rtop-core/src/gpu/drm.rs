//! AMD e Intel por sysfs (`/sys/class/drm`), y uso por proceso de cualquier
//! driver DRM leyendo `/proc/<pid>/fdinfo` (claves `drm-*`).

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use super::{Gpu, GpuProc, Vendor};

pub(super) struct Card {
    pub dev: PathBuf,
    pub pci: String,
    pub vendor: Vendor,
    pub name: String,
}

pub(super) fn read_u64(p: impl AsRef<Path>) -> Option<u64> {
    fs::read_to_string(p).ok()?.trim().parse().ok()
}

fn hwmon(dev: &Path, file: &str) -> Option<u64> {
    let dir = fs::read_dir(dev.join("hwmon")).ok()?.flatten().next()?.path();
    read_u64(dir.join(file))
}

pub(super) fn discover(cpu_brand: &str) -> Vec<Card> {
    let mut cards = vec![];
    let Ok(rd) = fs::read_dir("/sys/class/drm") else { return cards };
    let mut names: Vec<String> = rd
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("card") && !n.contains('-'))
        .collect();
    names.sort();
    for n in names {
        let dev = PathBuf::from("/sys/class/drm").join(&n).join("device");
        let pci = fs::canonicalize(&dev)
            .ok()
            .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
            .unwrap_or_else(|| n.clone());
        let vendor = match fs::read_to_string(dev.join("vendor")).unwrap_or_default().trim() {
            "0x1002" => Vendor::Amd,
            "0x10de" => Vendor::Nvidia,
            "0x8086" => Vendor::Intel,
            _ => Vendor::Other,
        };
        let name = match vendor {
            // la integrada AMD suele venir en el nombre del CPU: "... w/ Radeon 740M Graphics"
            Vendor::Amd => cpu_brand
                .split_once("w/ ")
                .or_else(|| cpu_brand.split_once("with "))
                .map(|(_, g)| format!("AMD {}", g.trim()))
                .unwrap_or_else(|| "AMD Radeon".into()),
            Vendor::Nvidia => "NVIDIA".into(),
            Vendor::Intel => "Intel GPU".into(),
            Vendor::Other => n.clone(),
        };
        cards.push(Card { dev, pci, vendor, name });
    }
    cards
}

// ---------- uso por proceso vía fdinfo (amdgpu, i915, xe, ...) ----------

#[derive(Default)]
pub(super) struct FdinfoState {
    /// (pdev, client-id) -> ns acumulados del motor
    prev: HashMap<(String, u64), u64>,
    at: Option<Instant>,
}

struct ClientSample {
    pdev: String,
    client: u64,
    pid: u32,
    engine_ns: u64,
    vram: u64,
}

fn scan_fdinfo() -> Vec<ClientSample> {
    let mut out = vec![];
    let mut seen: std::collections::HashSet<(String, u64)> = Default::default();
    let Ok(procs) = fs::read_dir("/proc") else { return out };
    for p in procs.flatten() {
        let Ok(pid) = p.file_name().to_string_lossy().parse::<u32>() else { continue };
        let Ok(fds) = fs::read_dir(p.path().join("fd")) else { continue };
        for fd in fds.flatten() {
            let Ok(target) = fs::read_link(fd.path()) else { continue };
            if !target.starts_with("/dev/dri/") {
                continue;
            }
            let Ok(info) = fs::read_to_string(p.path().join("fdinfo").join(fd.file_name())) else { continue };
            let Some((pdev, client, engine_ns, vram)) = parse_fdinfo(&info) else { continue };
            // el mismo cliente puede estar en varios fds/procesos (dup, fork)
            if !seen.insert((pdev.clone(), client)) {
                continue;
            }
            out.push(ClientSample { pdev, client, pid, engine_ns, vram });
        }
    }
    out
}

/// Un fdinfo de DRM -> (pdev, client-id, ns de motor sumados, VRAM en bytes).
/// None si no es un cliente DRM.
fn parse_fdinfo(info: &str) -> Option<(String, u64, u64, u64)> {
    let mut pdev = String::new();
    let mut client = None;
    let mut ns = 0u64;
    let mut vram = 0u64;
    let first = |v: &str| v.split_whitespace().next().and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
    for l in info.lines() {
        let Some((k, v)) = l.split_once(':') else { continue };
        let v = v.trim();
        match k {
            "drm-pdev" => pdev = v.to_string(),
            "drm-client-id" => client = v.parse().ok(),
            k if k.starts_with("drm-engine-") && !k.starts_with("drm-engine-capacity") => ns += first(v),
            "drm-memory-vram" | "drm-resident-vram" => vram = vram.max(first(v) * 1024),
            _ => {}
        }
    }
    Some((pdev, client?, ns, vram))
}

fn comm(pid: u32) -> String {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| pid.to_string())
}

pub(super) fn fdinfo_procs(st: &mut FdinfoState) -> HashMap<String, Vec<GpuProc>> {
    let now = Instant::now();
    let dt_ns = st.at.map(|a| now.duration_since(a).as_nanos() as f64).unwrap_or(0.0);
    st.at = Some(now);
    let samples = scan_fdinfo();
    let mut next = HashMap::new();
    // pdev -> pid -> (uso, vram)
    let mut agg: HashMap<String, HashMap<u32, (f32, u64, bool)>> = HashMap::new();
    for s in samples {
        let key = (s.pdev.clone(), s.client);
        let usage = st.prev.get(&key).and_then(|&p| {
            (dt_ns > 0.0).then(|| (s.engine_ns.saturating_sub(p) as f64 / dt_ns * 100.0) as f32)
        });
        next.insert(key, s.engine_ns);
        let e = agg.entry(s.pdev).or_default().entry(s.pid).or_insert((0.0, 0, false));
        if let Some(u) = usage {
            e.0 += u;
            e.2 = true;
        }
        e.1 += s.vram;
    }
    st.prev = next;
    agg.into_iter()
        .map(|(pdev, m)| {
            let mut v: Vec<GpuProc> = m
                .into_iter()
                .map(|(pid, (u, vram, has))| GpuProc {
                    pid,
                    name: comm(pid),
                    usage: has.then_some(u.min(100.0)),
                    vram,
                })
                .collect();
            super::sort_procs(&mut v);
            (pdev, v)
        })
        .collect()
}


/// Lee una placa AMD/Intel de sysfs. `procs` sale de `fdinfo_procs`.
pub(super) fn read_card(c: &Card, procs: Vec<GpuProc>) -> Gpu {
    let mut g = Gpu::empty(c.pci.clone(), c.name.clone(), c.vendor);
    g.busy = read_u64(c.dev.join("gpu_busy_percent")).map(|v| v as f32);
    g.vram_used = read_u64(c.dev.join("mem_info_vram_used")).unwrap_or(0);
    g.vram_total = read_u64(c.dev.join("mem_info_vram_total")).unwrap_or(0);
    g.shared_used = read_u64(c.dev.join("mem_info_gtt_used")).unwrap_or(0);
    g.shared_total = read_u64(c.dev.join("mem_info_gtt_total")).unwrap_or(0);
    // integrada: poca VRAM dedicada (carve-out) y bastante GTT
    g.integrated = g.vram_total > 0 && g.vram_total <= 2 * 1024 * 1024 * 1024;
    g.temp = hwmon(&c.dev, "temp1_input").map(|t| t as f32 / 1000.0);
    g.power_w = hwmon(&c.dev, "power1_average")
        .or_else(|| hwmon(&c.dev, "power1_input"))
        .map(|p| p as f32 / 1e6);
    g.power_limit_w = hwmon(&c.dev, "power1_cap").map(|p| p as f32 / 1e6);
    g.clock_mhz = hwmon(&c.dev, "freq1_input").map(|f| f as f32 / 1e6);
    g.mem_clock_mhz = hwmon(&c.dev, "freq2_input").map(|f| f as f32 / 1e6);
    g.procs = procs;
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fdinfo() {
        let info = "pos:\t0\ndrm-driver:\tamdgpu\ndrm-pdev:\t0000:65:00.0\ndrm-client-id:\t42\n\
                    drm-engine-gfx:\t1000 ns\ndrm-engine-compute:\t500 ns\ndrm-engine-capacity-gfx:\t1\n\
                    drm-memory-vram:\t2048 KiB\n";
        assert_eq!(parse_fdinfo(info), Some(("0000:65:00.0".into(), 42, 1500, 2048 * 1024)));
        // sin client-id no es un cliente DRM
        assert_eq!(parse_fdinfo("pos: 0\nflags: 02\n"), None);
    }
}
