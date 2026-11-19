//! GPUs: AMD/Intel vía sysfs (`/sys/class/drm`), NVIDIA vía `nvidia-smi`.
//!
//! Se consulta en un hilo aparte porque `nvidia-smi` tarda ~40-60 ms. Si la
//! NVIDIA está en runtime-suspend (laptops híbridas) no se la despierta.
//!
//! Uso por proceso: AMD/Intel leyendo `/proc/<pid>/fdinfo` (claves `drm-*`),
//! NVIDIA con `nvidia-smi pmon`.

mod drm;
mod nvidia;

use std::{
    collections::HashMap,
    fs,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use drm::{discover, Card, FdinfoState};

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

fn sample(cards: &[Card], st: &mut FdinfoState, with_procs: bool) -> Vec<Gpu> {
    let mut out = vec![];
    let mut fd = if with_procs { drm::fdinfo_procs(st) } else { HashMap::new() };
    let mut nvidia_done = false;
    for c in cards {
        match c.vendor {
            Vendor::Nvidia => {
                // dormida (runtime suspend): no se la despierta preguntándole
                let status = fs::read_to_string(c.dev.join("power/runtime_status")).unwrap_or_default();
                if status.trim() == "suspended" {
                    let mut g = Gpu::empty(c.pci.clone(), c.name.clone(), c.vendor);
                    g.asleep = true;
                    out.push(g);
                } else if !nvidia_done {
                    // nvidia-smi devuelve todas las NVIDIA de una vez
                    nvidia_done = true;
                    out.extend(nvidia::read(with_procs));
                }
            }
            _ => out.push(drm::read_card(c, fd.remove(&c.pci).unwrap_or_default())),
        }
    }
    // NVIDIA primero (la dedicada es la que más interesa)
    out.sort_by_key(|g| g.vendor != Vendor::Nvidia);
    out
}

/// Más uso primero; a igual uso, más VRAM.
fn sort_procs(v: &mut [GpuProc]) {
    v.sort_by(|a, b| b.usage.unwrap_or(0.0).total_cmp(&a.usage.unwrap_or(0.0)).then(b.vram.cmp(&a.vram)));
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
                    let with_procs = n.is_multiple_of(3);
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
