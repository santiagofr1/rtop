//! GPUs: AMD/Intel vía sysfs (`/sys/class/drm`), NVIDIA vía `nvidia-smi`.
//!
//! Se consulta en un hilo aparte porque `nvidia-smi` tarda ~40-60 ms. Si la
//! NVIDIA está en runtime-suspend (laptops híbridas) no se la despierta.
//!
//! Uso por proceso: AMD/Intel leyendo `/proc/<pid>/fdinfo` (claves `drm-*`),
//! NVIDIA con `nvidia-smi pmon`.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vendor {
    Amd,
    Nvidia,
    Intel,
    Other,
}

impl Vendor {
    pub fn label(self) -> &'static str {
        match self {
            Vendor::Amd => "AMD",
            Vendor::Nvidia => "NVIDIA",
            Vendor::Intel => "Intel",
            Vendor::Other => "?",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GpuProc {
    pub pid: u32,
    pub name: String,
    /// % de uso del motor gráfico
    pub usage: Option<f32>,
    pub vram: u64,
}

#[derive(Clone, Debug)]
pub struct Gpu {
    /// clave estable (dirección PCI)
    pub id: String,
    pub name: String,
    pub vendor: Vendor,
    pub integrated: bool,
    /// uso 0..100, None si no se puede leer
    pub busy: Option<f32>,
    pub vram_used: u64,
    pub vram_total: u64,
    /// memoria compartida (GTT) en integradas
    pub shared_used: u64,
    pub shared_total: u64,
    pub temp: Option<f32>,
    pub power_w: Option<f32>,
    pub power_limit_w: Option<f32>,
    pub clock_mhz: Option<f32>,
    pub mem_clock_mhz: Option<f32>,
    pub fan_pct: Option<f32>,
    pub pstate: Option<String>,
    /// dGPU dormida (runtime suspend): no hay datos, y está bien así
    pub asleep: bool,
    pub procs: Vec<GpuProc>,
}

impl Gpu {
    pub fn vram_pct(&self) -> f32 {
        if self.vram_total == 0 { 0.0 } else { self.vram_used as f32 * 100.0 / self.vram_total as f32 }
    }
    pub fn short_name(&self) -> String {
        short_gpu_name(&self.name)
    }
    fn empty(id: String, name: String, vendor: Vendor) -> Gpu {
        Gpu {
            id,
            name,
            vendor,
            integrated: false,
            busy: None,
            vram_used: 0,
            vram_total: 0,
            shared_used: 0,
            shared_total: 0,
            temp: None,
            power_w: None,
            power_limit_w: None,
            clock_mhz: None,
            mem_clock_mhz: None,
            fan_pct: None,
            pstate: None,
            asleep: false,
            procs: vec![],
        }
    }
}

pub fn short_gpu_name(n: &str) -> String {
    let mut s = n.to_string();
    for junk in ["NVIDIA ", "GeForce ", "AMD ", "Intel(R) ", "Laptop GPU", " Graphics", "(TM)"] {
        s = s.replace(junk, "");
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct Card {
    dev: PathBuf,
    pci: String,
    vendor: Vendor,
    name: String,
}

fn read_u64(p: impl AsRef<Path>) -> Option<u64> {
    fs::read_to_string(p).ok()?.trim().parse().ok()
}

fn hwmon(dev: &Path, file: &str) -> Option<u64> {
    let dir = fs::read_dir(dev.join("hwmon")).ok()?.flatten().next()?.path();
    read_u64(dir.join(file))
}

fn discover(cpu_brand: &str) -> Vec<Card> {
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
struct FdinfoState {
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
            let mut pdev = String::new();
            let mut client = None;
            let mut ns = 0u64;
            let mut vram = 0u64;
            for l in info.lines() {
                let Some((k, v)) = l.split_once(':') else { continue };
                let v = v.trim();
                match k {
                    "drm-pdev" => pdev = v.to_string(),
                    "drm-client-id" => client = v.parse().ok(),
                    k if k.starts_with("drm-engine-") && !k.starts_with("drm-engine-capacity") => {
                        ns += v.split_whitespace().next().and_then(|x| x.parse::<u64>().ok()).unwrap_or(0)
                    }
                    "drm-memory-vram" | "drm-resident-vram" => {
                        let kib = v.split_whitespace().next().and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
                        vram = vram.max(kib * 1024);
                    }
                    _ => {}
                }
            }
            let Some(client) = client else { continue };
            // el mismo cliente puede estar en varios fds/procesos (dup, fork)
            if !seen.insert((pdev.clone(), client)) {
                continue;
            }
            out.push(ClientSample { pdev, client, pid, engine_ns: ns, vram });
        }
    }
    out
}

fn comm(pid: u32) -> String {
    fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| pid.to_string())
}

fn fdinfo_procs(st: &mut FdinfoState) -> HashMap<String, Vec<GpuProc>> {
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
            v.sort_by(|a, b| b.usage.unwrap_or(0.0).total_cmp(&a.usage.unwrap_or(0.0)).then(b.vram.cmp(&a.vram)));
            (pdev, v)
        })
        .collect()
}

// ---------- NVIDIA ----------

fn nvidia_smi(with_procs: bool) -> Vec<Gpu> {
    let Ok(o) = Command::new("nvidia-smi")
        .args([
            "--query-gpu=index,pci.bus_id,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit,clocks.gr,clocks.mem,fan.speed,pstate",
            "--format=csv,noheader,nounits",
        ])
        .output()
    else {
        return vec![];
    };
    let num = |s: &str| s.trim().parse::<f32>().ok();
    let mut gpus: Vec<Gpu> = String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').map(str::trim).collect();
            if f.len() < 13 {
                return None;
            }
            const MIB: u64 = 1024 * 1024;
            // "00000000:01:00.0" -> "0000:01:00.0" (formato sysfs)
            let pci = f[1].to_lowercase();
            let pci = pci.get(pci.len().saturating_sub(12)..).unwrap_or(&pci).to_string();
            let mut g = Gpu::empty(pci, f[2].to_string(), Vendor::Nvidia);
            g.busy = num(f[3]);
            g.vram_used = num(f[4]).unwrap_or(0.0) as u64 * MIB;
            g.vram_total = num(f[5]).unwrap_or(0.0) as u64 * MIB;
            g.temp = num(f[6]);
            g.power_w = num(f[7]);
            g.power_limit_w = num(f[8]);
            g.clock_mhz = num(f[9]);
            g.mem_clock_mhz = num(f[10]);
            g.fan_pct = num(f[11]);
            g.pstate = Some(f[12].to_string()).filter(|s| !s.is_empty() && s != "[N/A]");
            Some(g)
        })
        .collect();

    // procesos: gpu pid type sm mem enc dec jpg ofa fb ccpm command
    if !with_procs {
        return gpus;
    }
    if let Ok(o) = Command::new("nvidia-smi").args(["pmon", "-c", "1", "-s", "um"]).output() {
        for l in String::from_utf8_lossy(&o.stdout).lines() {
            if l.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 6 {
                continue;
            }
            let (Ok(idx), Ok(pid)) = (f[0].parse::<usize>(), f[1].parse::<u32>()) else { continue };
            let fb = f.get(f.len().saturating_sub(3)).and_then(|x| x.parse::<u64>().ok()).unwrap_or(0);
            if let Some(g) = gpus.get_mut(idx) {
                g.procs.push(GpuProc {
                    pid,
                    name: f.last().map(|s| s.to_string()).unwrap_or_default(),
                    usage: f[3].parse().ok(),
                    vram: fb * 1024 * 1024,
                });
            }
        }
        for g in &mut gpus {
            g.procs.sort_by(|a, b| b.usage.unwrap_or(0.0).total_cmp(&a.usage.unwrap_or(0.0)).then(b.vram.cmp(&a.vram)));
        }
    }
    gpus
}

fn sample(cards: &[Card], st: &mut FdinfoState, with_procs: bool) -> Vec<Gpu> {
    let mut out = vec![];
    let mut fd = if with_procs { fdinfo_procs(st) } else { HashMap::new() };
    let mut nvidia_done = false;
    for c in cards {
        match c.vendor {
            Vendor::Nvidia => {
                let status = fs::read_to_string(c.dev.join("power/runtime_status")).unwrap_or_default();
                if status.trim() == "suspended" {
                    let mut g = Gpu::empty(c.pci.clone(), c.name.clone(), c.vendor);
                    g.asleep = true;
                    out.push(g);
                    continue;
                }
                if !nvidia_done {
                    nvidia_done = true;
                    out.extend(nvidia_smi(with_procs));
                }
            }
            _ => {
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
                g.procs = fd.remove(&c.pci).unwrap_or_default();
                out.push(g);
            }
        }
    }
    // NVIDIA primero (la dedicada es la que más interesa)
    out.sort_by_key(|g| g.vendor != Vendor::Nvidia);
    out
}

/// Lee las GPUs en segundo plano; `latest()` devuelve la última muestra.
pub struct GpuPoller {
    data: Arc<Mutex<Vec<Gpu>>>,
}

impl GpuPoller {
    pub fn start(cpu_brand: &str, interval: Duration) -> Self {
        let cards = discover(cpu_brand);
        let mut st = FdinfoState::default();
        let data = Arc::new(Mutex::new(sample(&cards, &mut st, true)));
        let d = data.clone();
        thread::Builder::new()
            .name("rtop-gpu".into())
            .spawn(move || {
                let mut n = 0u64;
                let mut last_procs: HashMap<String, Vec<GpuProc>> = HashMap::new();
                loop {
                    thread::sleep(interval);
                    n += 1;
                    // uso por proceso (`nvidia-smi pmon`, recorrer fdinfo) es lo más caro: cada 3 muestras alcanza
                    let with_procs = n % 3 == 0;
                    let mut s = sample(&cards, &mut st, with_procs);
                    for g in &mut s {
                        if with_procs {
                            last_procs.insert(g.id.clone(), g.procs.clone());
                        } else if let Some(p) = last_procs.get(&g.id) {
                            g.procs = p.clone();
                        }
                    }
                    if let Ok(mut g) = d.lock() {
                        *g = s;
                    }
                }
            })
            .ok();
        GpuPoller { data }
    }

    pub fn latest(&self) -> Vec<Gpu> {
        self.data.lock().map(|g| g.clone()).unwrap_or_default()
    }
}
