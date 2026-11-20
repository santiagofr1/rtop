//! Tipos de datos que ven las interfaces: procesos y la foto del sistema.

use sysinfo::ProcessStatus;

use crate::{battery::Power, gpu::Gpu, meminfo::MemInfo};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Run,
    Sleep,
    Stop,
    Zombie,
    Disk,
    Other,
}

impl State {
    pub fn letter(self) -> &'static str {
        match self {
            State::Run => "R",
            State::Sleep => "S",
            State::Stop => "T",
            State::Zombie => "Z",
            State::Disk => "D",
            State::Other => "?",
        }
    }
}

impl From<ProcessStatus> for State {
    fn from(s: ProcessStatus) -> Self {
        match s {
            ProcessStatus::Run => State::Run,
            ProcessStatus::Sleep | ProcessStatus::Idle => State::Sleep,
            ProcessStatus::Stop | ProcessStatus::Tracing => State::Stop,
            ProcessStatus::Zombie | ProcessStatus::Dead => State::Zombie,
            ProcessStatus::UninterruptibleDiskSleep => State::Disk,
            _ => State::Other,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Proc {
    pub pid: u32,
    pub user: String,
    pub cpu: f32,
    pub mem: u64,
    pub mem_pct: f32,
    pub state: State,
    pub time: u64,
    pub name: String,
    pub cmd: String,
    /// bytes/s
    pub disk_read: f64,
    pub disk_write: f64,
    pub parent: Option<u32>,
    /// segundos desde epoch
    pub start_time: u64,
    /// uso de GPU sumado entre todas las placas (None = no usa GPU)
    pub gpu: Option<f32>,
    pub vram: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortBy {
    Cpu,
    Mem,
    Time,
    Pid,
    Name,
    Disk,
    User,
    Gpu,
    Vram,
    State,
}

impl SortBy {
    pub const CYCLE: [SortBy; 5] = [SortBy::Cpu, SortBy::Mem, SortBy::Time, SortBy::Pid, SortBy::Name];

    pub fn next(self) -> Self {
        let i = Self::CYCLE.iter().position(|s| *s == self).unwrap_or(0);
        Self::CYCLE[(i + 1) % Self::CYCLE.len()]
    }
    pub fn label(self) -> &'static str {
        match self {
            SortBy::Cpu => "CPU%",
            SortBy::Mem => "MEM%",
            SortBy::Time => "TIEMPO",
            SortBy::Pid => "PID",
            SortBy::Name => "NOMBRE",
            SortBy::Disk => "DISCO",
            SortBy::User => "USUARIO",
            SortBy::Gpu => "GPU%",
            SortBy::Vram => "VRAM",
            SortBy::State => "ESTADO",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Temp {
    pub label: String,
    pub value: f32,
    pub max: Option<f32>,
    pub crit: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct NetIf {
    pub name: String,
    pub rx_rate: f64,
    pub tx_rate: f64,
    pub total_rx: u64,
    pub total_tx: u64,
    pub ips: Vec<String>,
    pub mac: String,
}

impl NetIf {
    /// interfaces virtuales (docker, bridges, veth)
    pub fn is_virtual(&self) -> bool {
        ["docker", "br-", "veth", "virbr", "vnet"].iter().any(|p| self.name.starts_with(p))
    }
}

#[derive(Clone, Debug)]
pub struct Mount {
    pub device: String,
    pub mount: String,
    pub fs: String,
    pub total: u64,
    pub used: u64,
    pub ssd: bool,
    pub removable: bool,
}

#[derive(Clone, Debug)]
pub struct DiskDev {
    pub name: String,
    pub model: Option<String>,
    pub read_rate: f64,
    pub write_rate: f64,
    pub read_total: u64,
    pub write_total: u64,
}

/// Snapshot del sistema (lo que no son procesos).
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub host: String,
    pub os: String,
    pub kernel: String,
    pub cpu_brand: String,
    /// "Ryzen 7 7445HS" en vez del nombre larguísimo
    pub cpu_model: String,
    /// MHz promedio
    pub cpu_freq: u64,
    pub core_freq: Vec<u64>,
    pub cores: Vec<f32>,
    pub cpu_total: f32,
    pub mem_total: u64,
    pub mem_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub meminfo: MemInfo,
    pub load: (f64, f64, f64),
    pub uptime: u64,
    pub rx_rate: f64,
    pub tx_rate: f64,
    pub nets: Vec<NetIf>,
    pub temps: Vec<Temp>,
    pub temp_max: Option<f32>,
    /// temperatura del CPU (Tctl / Package / coretemp)
    pub cpu_temp: Option<f32>,
    pub disk_read_rate: f64,
    pub disk_write_rate: f64,
    pub devs: Vec<DiskDev>,
    pub mounts: Vec<Mount>,
    /// espacio del disco raíz
    pub disk_used: u64,
    pub disk_total: u64,
    pub gpus: Vec<Gpu>,
    /// baterías, cargadores y perfil de energía
    pub power: Power,
    pub tasks: usize,
    pub running: usize,
}

impl Stats {
    pub fn mem_pct(&self) -> f32 {
        pct(self.mem_used, self.mem_total)
    }
    pub fn swap_pct(&self) -> f32 {
        pct(self.swap_used, self.swap_total)
    }
    pub fn disk_pct(&self) -> f32 {
        pct(self.disk_used, self.disk_total)
    }
}

pub fn pct(a: u64, b: u64) -> f32 {
    if b == 0 { 0.0 } else { a as f32 * 100.0 / b as f32 }
}
