//! GPUs: AMD/Intel vía sysfs (`/sys/class/drm`), NVIDIA vía `nvidia-smi`.
//!
//! Se consulta en un hilo aparte porque `nvidia-smi` tarda ~40 ms. Si la
//! NVIDIA está en runtime-suspend (laptops híbridas) no se la despierta.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vendor {
    Amd,
    Nvidia,
    Intel,
    Other,
}

#[derive(Clone, Debug)]
pub struct Gpu {
    pub name: String,
    pub vendor: Vendor,
    /// uso 0..100, None si no se puede leer
    pub busy: Option<f32>,
    pub vram_used: u64,
    pub vram_total: u64,
    /// memoria compartida (GTT) en integradas
    pub shared_used: u64,
    pub shared_total: u64,
    pub temp: Option<f32>,
    pub power_w: Option<f32>,
    /// dGPU dormida (runtime suspend): no hay datos, y está bien así
    pub asleep: bool,
}

impl Gpu {
    pub fn vram_pct(&self) -> f32 {
        if self.vram_total == 0 { 0.0 } else { self.vram_used as f32 * 100.0 / self.vram_total as f32 }
    }
    pub fn short_name(&self) -> String {
        short_gpu_name(&self.name)
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
        cards.push(Card { dev, vendor, name });
    }
    cards
}

fn sample(cards: &[Card]) -> Vec<Gpu> {
    let mut out = vec![];
    for c in cards {
        match c.vendor {
            Vendor::Nvidia => {
                let status = fs::read_to_string(c.dev.join("power/runtime_status")).unwrap_or_default();
                if status.trim() == "suspended" {
                    out.push(Gpu {
                        name: c.name.clone(),
                        vendor: c.vendor,
                        busy: None,
                        vram_used: 0,
                        vram_total: 0,
                        shared_used: 0,
                        shared_total: 0,
                        temp: None,
                        power_w: None,
                        asleep: true,
                    });
                    continue;
                }
                out.extend(nvidia_smi());
            }
            _ => {
                let busy = read_u64(c.dev.join("gpu_busy_percent")).map(|v| v as f32);
                out.push(Gpu {
                    name: c.name.clone(),
                    vendor: c.vendor,
                    busy,
                    vram_used: read_u64(c.dev.join("mem_info_vram_used")).unwrap_or(0),
                    vram_total: read_u64(c.dev.join("mem_info_vram_total")).unwrap_or(0),
                    shared_used: read_u64(c.dev.join("mem_info_gtt_used")).unwrap_or(0),
                    shared_total: read_u64(c.dev.join("mem_info_gtt_total")).unwrap_or(0),
                    temp: hwmon(&c.dev, "temp1_input").map(|t| t as f32 / 1000.0),
                    power_w: hwmon(&c.dev, "power1_average")
                        .or_else(|| hwmon(&c.dev, "power1_input"))
                        .map(|p| p as f32 / 1e6),
                    asleep: false,
                });
            }
        }
    }
    // NVIDIA primero (la dedicada es la que más interesa)
    out.sort_by_key(|g| g.vendor != Vendor::Nvidia);
    out
}

fn nvidia_smi() -> Vec<Gpu> {
    let Ok(o) = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw",
            "--format=csv,noheader,nounits",
        ])
        .output()
    else {
        return vec![];
    };
    let num = |s: &str| s.trim().parse::<f32>().ok();
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            if f.len() < 6 {
                return None;
            }
            const MIB: u64 = 1024 * 1024;
            Some(Gpu {
                name: f[0].trim().to_string(),
                vendor: Vendor::Nvidia,
                busy: num(f[1]),
                vram_used: num(f[2]).unwrap_or(0.0) as u64 * MIB,
                vram_total: num(f[3]).unwrap_or(0.0) as u64 * MIB,
                shared_used: 0,
                shared_total: 0,
                temp: num(f[4]),
                power_w: num(f[5]),
                asleep: false,
            })
        })
        .collect()
}

/// Lee las GPUs en segundo plano; `latest()` devuelve la última muestra.
pub struct GpuPoller {
    data: Arc<Mutex<Vec<Gpu>>>,
}

impl GpuPoller {
    pub fn start(cpu_brand: &str, interval: Duration) -> Self {
        let cards = discover(cpu_brand);
        let data = Arc::new(Mutex::new(sample(&cards)));
        let d = data.clone();
        thread::Builder::new()
            .name("rtop-gpu".into())
            .spawn(move || loop {
                thread::sleep(interval);
                let s = sample(&cards);
                if let Ok(mut g) = d.lock() {
                    *g = s;
                }
            })
            .ok();
        GpuPoller { data }
    }

    pub fn latest(&self) -> Vec<Gpu> {
        self.data.lock().map(|g| g.clone()).unwrap_or_default()
    }
}
