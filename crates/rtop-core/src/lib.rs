//! Recolección de datos del sistema + config/temas, compartido por la TUI y la GUI.

pub mod config;
pub mod disk;
pub mod fmt;
pub mod gpu;

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fs,
    time::Instant,
};

use sysinfo::{
    Components, DiskKind, Disks, Networks, Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate,
    Signal, System, UpdateKind, Users,
};

pub use config::{Config, Rgb, Theme};
pub use gpu::{Gpu, GpuProc, Vendor};

pub const HIST: usize = 300;

pub type Hist<T> = VecDeque<T>;

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

/// Datos extra de un proceso, leídos de `/proc` solo cuando hacen falta.
#[derive(Clone, Debug, Default)]
pub struct ProcDetails {
    pub threads: Option<u32>,
    pub nice: Option<i32>,
    pub swap: Option<u64>,
    pub exe: Option<String>,
    pub cwd: Option<String>,
}

impl ProcDetails {
    pub fn read(pid: u32) -> ProcDetails {
        let base = format!("/proc/{pid}");
        let mut d = ProcDetails::default();
        if let Ok(st) = fs::read_to_string(format!("{base}/status")) {
            for l in st.lines() {
                let Some((k, v)) = l.split_once(':') else { continue };
                let v = v.trim();
                match k {
                    "Threads" => d.threads = v.parse().ok(),
                    "VmSwap" => d.swap = v.split_whitespace().next().and_then(|x| x.parse::<u64>().ok()).map(|kb| kb * 1024),
                    _ => {}
                }
            }
        }
        // /proc/pid/stat: el campo 19 es nice (el nombre va entre paréntesis y puede tener espacios)
        if let Ok(st) = fs::read_to_string(format!("{base}/stat")) {
            if let Some(rest) = st.rsplit_once(')').map(|(_, r)| r) {
                d.nice = rest.split_whitespace().nth(16).and_then(|x| x.parse().ok());
            }
        }
        let link = |f: &str| fs::read_link(format!("{base}/{f}")).ok().map(|p| p.to_string_lossy().into_owned());
        d.exe = link("exe");
        d.cwd = link("cwd");
        d
    }
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

/// Desglose de `/proc/meminfo` (bytes).
#[derive(Clone, Debug, Default)]
pub struct MemInfo {
    pub total: u64,
    pub free: u64,
    pub available: u64,
    pub buffers: u64,
    pub cached: u64,
    pub shmem: u64,
    pub sreclaimable: u64,
    pub dirty: u64,
    pub swap_cached: u64,
}

impl MemInfo {
    /// lo que de verdad usan los programas
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
    /// caché + buffers (se libera si hace falta)
    pub fn cache(&self) -> u64 {
        (self.cached + self.buffers + self.sreclaimable).saturating_sub(self.shmem)
    }

    fn read() -> MemInfo {
        let mut m = MemInfo::default();
        let Ok(s) = fs::read_to_string("/proc/meminfo") else { return m };
        for l in s.lines() {
            let Some((k, v)) = l.split_once(':') else { continue };
            let kb: u64 = v.split_whitespace().next().and_then(|x| x.parse().ok()).unwrap_or(0);
            let b = kb * 1024;
            match k {
                "MemTotal" => m.total = b,
                "MemFree" => m.free = b,
                "MemAvailable" => m.available = b,
                "Buffers" => m.buffers = b,
                "Cached" => m.cached = b,
                "Shmem" => m.shmem = b,
                "SReclaimable" => m.sreclaimable = b,
                "Dirty" => m.dirty = b,
                "SwapCached" => m.swap_cached = b,
                _ => {}
            }
        }
        m
    }
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

#[derive(Default)]
pub struct GpuHist {
    pub busy: Hist<f32>,
    pub vram: Hist<f32>,
    pub temp: Hist<f32>,
    pub power: Hist<f32>,
    pub clock: Hist<f32>,
}

pub struct Monitor {
    sys: System,
    users: Users,
    nets: Networks,
    comps: Components,
    disks: Disks,
    gpu: gpu::GpuPoller,
    dev_prev: HashMap<String, (u64, u64)>,
    ticks: u64,
    last: Instant,
    pub stats: Stats,
    pub procs: Vec<Proc>,
    pub cpu_hist: Hist<f32>,
    pub core_hist: Vec<Hist<f32>>,
    pub freq_hist: Hist<f32>,
    pub load_hist: Hist<f32>,
    pub mem_hist: Hist<f32>,
    pub cache_hist: Hist<f32>,
    pub swap_hist: Hist<f32>,
    pub rx_hist: Hist<f64>,
    pub tx_hist: Hist<f64>,
    pub disk_r_hist: Hist<f64>,
    pub disk_w_hist: Hist<f64>,
    pub net_hist: BTreeMap<String, (Hist<f64>, Hist<f64>)>,
    pub dev_hist: BTreeMap<String, (Hist<f64>, Hist<f64>)>,
    pub temp_hist: BTreeMap<String, Hist<f32>>,
    /// por GPU (mismo orden que `stats.gpus`)
    pub gpu_hist: Vec<Hist<f32>>,
    pub vram_hist: Vec<Hist<f32>>,
    /// por id de GPU, con más series
    pub gpu_detail: BTreeMap<String, GpuHist>,
}

impl Monitor {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        sys.refresh_all();
        let brand = sys.cpus().first().map(|c| c.brand().to_string()).unwrap_or_default();
        let mut m = Monitor {
            sys,
            users: Users::new_with_refreshed_list(),
            nets: Networks::new_with_refreshed_list(),
            comps: Components::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            gpu: gpu::GpuPoller::start(&brand, std::time::Duration::from_millis(1000)),
            dev_prev: disk::devices().into_iter().map(|d| (d.name, (d.read, d.write))).collect(),
            ticks: 0,
            last: Instant::now(),
            stats: Stats::default(),
            procs: Vec::new(),
            cpu_hist: Hist::with_capacity(HIST),
            core_hist: vec![],
            freq_hist: Hist::with_capacity(HIST),
            load_hist: Hist::with_capacity(HIST),
            mem_hist: Hist::with_capacity(HIST),
            cache_hist: Hist::with_capacity(HIST),
            swap_hist: Hist::with_capacity(HIST),
            rx_hist: Hist::with_capacity(HIST),
            tx_hist: Hist::with_capacity(HIST),
            disk_r_hist: Hist::with_capacity(HIST),
            disk_w_hist: Hist::with_capacity(HIST),
            net_hist: BTreeMap::new(),
            dev_hist: BTreeMap::new(),
            temp_hist: BTreeMap::new(),
            gpu_hist: vec![],
            vram_hist: vec![],
            gpu_detail: BTreeMap::new(),
        };
        m.collect(0.0);
        m
    }

    pub fn refresh(&mut self) {
        let dt = self.last.elapsed().as_secs_f64().max(0.001);
        self.last = Instant::now();
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            // solo lo que se muestra: `everything()` lee entorno, exe, cwd... y es lento
            ProcessRefreshKind::nothing()
                .with_cpu()
                .with_memory()
                .with_disk_usage()
                .with_user(UpdateKind::OnlyIfNotSet)
                .with_cmd(UpdateKind::OnlyIfNotSet),
        );
        self.nets.refresh(true);
        self.ticks += 1;
        // los sensores (hwmon, nvme) tardan ~10 ms en leerse y cambian lento
        if self.ticks % 3 == 0 {
            self.comps.refresh(false);
        }
        self.disks.refresh(false);
        self.collect(dt);
    }

    fn collect(&mut self, dt: f64) {
        let rate = |b: u64| if dt > 0.0 { b as f64 / dt } else { 0.0 };

        // red
        let mut nets: Vec<NetIf> = self
            .nets
            .iter()
            .filter(|(n, _)| *n != "lo")
            .map(|(n, d)| NetIf {
                name: n.clone(),
                rx_rate: rate(d.received()),
                tx_rate: rate(d.transmitted()),
                total_rx: d.total_received(),
                total_tx: d.total_transmitted(),
                ips: d.ip_networks().iter().map(|ip| format!("{}/{}", ip.addr, ip.prefix)).collect(),
                mac: d.mac_address().to_string(),
            })
            .collect();
        // primero las que tienen tráfico, después las virtuales
        nets.sort_by(|a, b| {
            a.is_virtual()
                .cmp(&b.is_virtual())
                .then((b.total_rx + b.total_tx).cmp(&(a.total_rx + a.total_tx)))
        });
        let rx_rate: f64 = nets.iter().map(|n| n.rx_rate).sum();
        let tx_rate: f64 = nets.iter().map(|n| n.tx_rate).sum();

        // disco
        let devs: Vec<DiskDev> = disk::devices()
            .into_iter()
            .map(|d| {
                let (pr, pw) = self.dev_prev.get(&d.name).copied().unwrap_or((d.read, d.write));
                self.dev_prev.insert(d.name.clone(), (d.read, d.write));
                DiskDev {
                    model: disk::model(&d.name),
                    read_rate: rate(d.read.saturating_sub(pr)),
                    write_rate: rate(d.write.saturating_sub(pw)),
                    read_total: d.read,
                    write_total: d.write,
                    name: d.name,
                }
            })
            .collect();
        let dr: f64 = devs.iter().map(|d| d.read_rate).sum();
        let dw: f64 = devs.iter().map(|d| d.write_rate).sum();
        let mounts: Vec<Mount> = self
            .disks
            .iter()
            .filter(|d| d.total_space() > 0)
            .map(|d| Mount {
                device: d.name().to_string_lossy().into_owned(),
                mount: d.mount_point().to_string_lossy().into_owned(),
                fs: d.file_system().to_string_lossy().into_owned(),
                total: d.total_space(),
                used: d.total_space().saturating_sub(d.available_space()),
                ssd: d.kind() == DiskKind::SSD,
                removable: d.is_removable(),
            })
            .collect();
        let root = mounts
            .iter()
            .find(|m| m.mount == "/")
            .or_else(|| mounts.iter().max_by_key(|m| m.total));
        let (disk_total, disk_used) = root.map(|m| (m.total, m.used)).unwrap_or((0, 0));

        // temperaturas
        let temps: Vec<Temp> = self
            .comps
            .iter()
            .filter_map(|c| {
                let v = c.temperature()?;
                v.is_finite().then(|| Temp {
                    label: pretty_sensor(c.label()),
                    value: v,
                    max: c.max().filter(|m| m.is_finite()),
                    crit: c.critical().filter(|m| m.is_finite()),
                })
            })
            .collect();
        let cpu_temp = temps
            .iter()
            .find(|t| ["CPU", "Tctl", "Tdie", "Package"].iter().any(|k| t.label.contains(k)))
            .map(|t| t.value);

        let gpus = self.gpu.latest();
        // pid -> (uso sumado, vram sumada) entre todas las GPUs
        let mut gpu_use: HashMap<u32, (Option<f32>, u64)> = HashMap::new();
        for g in &gpus {
            for gp in &g.procs {
                let e = gpu_use.entry(gp.pid).or_insert((None, 0));
                if let Some(u) = gp.usage {
                    e.0 = Some(e.0.unwrap_or(0.0) + u);
                }
                e.1 += gp.vram;
            }
        }

        // procesos
        let mem_total = self.sys.total_memory();
        let mt = mem_total.max(1) as f32;
        let users = &self.users;
        self.procs = self
            .sys
            .processes()
            .values()
            .filter(|p| p.thread_kind().is_none())
            .map(|p| {
                let du = p.disk_usage();
                Proc {
                    pid: p.pid().as_u32(),
                    user: p
                        .user_id()
                        .and_then(|u| users.get_user_by_id(u))
                        .map(|u| u.name().to_string())
                        .unwrap_or_else(|| "?".into()),
                    cpu: p.cpu_usage(),
                    mem: p.memory(),
                    mem_pct: p.memory() as f32 * 100.0 / mt,
                    state: p.status().into(),
                    time: p.run_time(),
                    name: p.name().to_string_lossy().into_owned(),
                    cmd: p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" "),
                    disk_read: rate(du.read_bytes),
                    disk_write: rate(du.written_bytes),
                    parent: p.parent().map(|x| x.as_u32()),
                    start_time: p.start_time(),
                    gpu: gpu_use.get(&p.pid().as_u32()).and_then(|g| g.0),
                    vram: gpu_use.get(&p.pid().as_u32()).map(|g| g.1).unwrap_or(0),
                }
            })
            .collect();

        let la = System::load_average();
        let cpus = self.sys.cpus();
        let core_freq: Vec<u64> = cpus.iter().map(|c| c.frequency()).collect();
        self.stats = Stats {
            host: System::host_name().unwrap_or_else(|| "sistema".into()),
            os: System::long_os_version().unwrap_or_default(),
            kernel: System::kernel_version().unwrap_or_default(),
            cpu_brand: cpus.first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
            cpu_model: short_cpu_name(cpus.first().map(|c| c.brand()).unwrap_or("")),
            cpu_freq: if core_freq.is_empty() { 0 } else { core_freq.iter().sum::<u64>() / core_freq.len() as u64 },
            core_freq,
            cores: cpus.iter().map(|c| c.cpu_usage()).collect(),
            cpu_total: self.sys.global_cpu_usage(),
            mem_total,
            mem_used: self.sys.used_memory(),
            swap_total: self.sys.total_swap(),
            swap_used: self.sys.used_swap(),
            meminfo: MemInfo::read(),
            load: (la.one, la.five, la.fifteen),
            uptime: System::uptime(),
            rx_rate,
            tx_rate,
            nets,
            temp_max: temps.iter().map(|t| t.value).reduce(f32::max),
            cpu_temp,
            temps,
            disk_read_rate: dr,
            disk_write_rate: dw,
            devs,
            mounts,
            disk_used,
            disk_total,
            gpus,
            tasks: self.procs.len(),
            running: self.procs.iter().filter(|p| p.state == State::Run).count(),
        };
        self.push_history();
    }

    fn push_history(&mut self) {
        let s = &self.stats;
        push(&mut self.cpu_hist, s.cpu_total);
        self.core_hist.resize_with(s.cores.len(), Hist::new);
        for (h, &v) in self.core_hist.iter_mut().zip(&s.cores) {
            push(h, v);
        }
        push(&mut self.freq_hist, s.cpu_freq as f32);
        push(&mut self.load_hist, s.load.0 as f32);
        push(&mut self.mem_hist, s.mem_pct());
        push(&mut self.cache_hist, pct(s.meminfo.cache(), s.meminfo.total));
        push(&mut self.swap_hist, s.swap_pct());
        push(&mut self.rx_hist, s.rx_rate);
        push(&mut self.tx_hist, s.tx_rate);
        push(&mut self.disk_r_hist, s.disk_read_rate);
        push(&mut self.disk_w_hist, s.disk_write_rate);
        for n in &s.nets {
            let e = self.net_hist.entry(n.name.clone()).or_default();
            push(&mut e.0, n.rx_rate);
            push(&mut e.1, n.tx_rate);
        }
        for d in &s.devs {
            let e = self.dev_hist.entry(d.name.clone()).or_default();
            push(&mut e.0, d.read_rate);
            push(&mut e.1, d.write_rate);
        }
        for t in &s.temps {
            push(self.temp_hist.entry(t.label.clone()).or_default(), t.value);
        }
        let n = s.gpus.len();
        self.gpu_hist.resize_with(n, Hist::new);
        self.vram_hist.resize_with(n, Hist::new);
        for (i, g) in s.gpus.iter().enumerate() {
            push(&mut self.gpu_hist[i], g.busy.unwrap_or(0.0));
            push(&mut self.vram_hist[i], g.vram_pct());
            let d = self.gpu_detail.entry(g.id.clone()).or_default();
            push(&mut d.busy, g.busy.unwrap_or(0.0));
            push(&mut d.vram, g.vram_pct());
            push(&mut d.temp, g.temp.unwrap_or(0.0));
            push(&mut d.power, g.power_w.unwrap_or(0.0));
            push(&mut d.clock, g.clock_mhz.unwrap_or(0.0));
        }
    }

    /// Procesos filtrados y ordenados (índices sobre `self.procs`).
    pub fn view(&self, filter: &str, sort: SortBy, reverse: bool) -> Vec<usize> {
        let f = filter.to_lowercase();
        let mut idx: Vec<usize> = self
            .procs
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                f.is_empty()
                    || p.name.to_lowercase().contains(&f)
                    || p.cmd.to_lowercase().contains(&f)
                    || p.user.to_lowercase().contains(&f)
                    || p.pid.to_string().contains(&f)
            })
            .map(|(i, _)| i)
            .collect();
        let ps = &self.procs;
        idx.sort_by(|&a, &b| {
            let (a, b) = (&ps[a], &ps[b]);
            let o = match sort {
                SortBy::Cpu => b.cpu.total_cmp(&a.cpu),
                SortBy::Mem => b.mem.cmp(&a.mem),
                SortBy::Time => b.time.cmp(&a.time),
                SortBy::Pid => a.pid.cmp(&b.pid),
                SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortBy::Disk => (b.disk_read + b.disk_write).total_cmp(&(a.disk_read + a.disk_write)),
                SortBy::User => a.user.cmp(&b.user),
                SortBy::Gpu => b.gpu.unwrap_or(-1.0).total_cmp(&a.gpu.unwrap_or(-1.0)),
                SortBy::Vram => b.vram.cmp(&a.vram),
                SortBy::State => a.state.letter().cmp(b.state.letter()),
            };
            if reverse { o.reverse() } else { o }
        });
        idx
    }

    /// Los `n` procesos que más consumen según `sort`.
    pub fn top(&self, sort: SortBy, n: usize) -> Vec<&Proc> {
        self.view("", sort, false).into_iter().take(n).map(|i| &self.procs[i]).collect()
    }

    pub fn proc(&self, pid: u32) -> Option<&Proc> {
        self.procs.iter().find(|p| p.pid == pid)
    }

    /// Manda SIGTERM (o SIGKILL si `force`). Devuelve true si se pudo.
    pub fn kill(&self, pid: u32, force: bool) -> bool {
        let sig = if force { Signal::Kill } else { Signal::Term };
        self.sys
            .process(Pid::from_u32(pid))
            .and_then(|p| p.kill_with(sig))
            .unwrap_or(false)
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Nombres de sensores más legibles.
fn pretty_sensor(label: &str) -> String {
    let l = label.trim();
    let map = [
        ("Tctl", "CPU (Tctl)"),
        ("Tdie", "CPU (Tdie)"),
        ("edge", "GPU integrada"),
        ("junction", "GPU (junction)"),
        ("Composite", "NVMe"),
        ("Sensor 1", "NVMe sensor 1"),
        ("Sensor 2", "NVMe sensor 2"),
        ("spd5118 temp1", "RAM"),
        ("acpitz temp1", "Placa (ACPI 1)"),
        ("acpitz temp2", "Placa (ACPI 2)"),
    ];
    for (k, v) in map {
        if l == k || l.ends_with(&format!(" {k}")) {
            return v.to_string();
        }
    }
    if l.starts_with("Package id") {
        return "CPU (Package)".into();
    }
    l.to_string()
}

/// "AMD Ryzen 7 7445HS w/ Radeon 740M Graphics" -> "Ryzen 7 7445HS"
/// "Intel(R) Core(TM) i7-10750H CPU @ 2.60GHz" -> "Core i7-10750H"
pub fn short_cpu_name(brand: &str) -> String {
    let mut s = brand.to_string();
    for cut in [" w/ ", " with ", " CPU @", " @ "] {
        if let Some(i) = s.find(cut) {
            s.truncate(i);
        }
    }
    for junk in ["(R)", "(TM)", "(tm)", "AMD ", "Intel ", " Processor", " CPU", "-Core", " Core Processor"] {
        s = s.replace(junk, "");
    }
    // "Ryzen 9 5900X 12" (cores al final) -> fuera
    let mut w: Vec<&str> = s.split_whitespace().collect();
    if w.len() > 3 && w.last().is_some_and(|t| t.len() <= 3 && t.chars().all(|c| c.is_ascii_digit())) {
        w.pop();
    }
    let s = w.join(" ");
    if s.is_empty() { brand.trim().to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_names() {
        assert_eq!(short_cpu_name("AMD Ryzen 7 7445HS w/ Radeon 740M Graphics"), "Ryzen 7 7445HS");
        assert_eq!(short_cpu_name("Intel(R) Core(TM) i7-10750H CPU @ 2.60GHz"), "Core i7-10750H");
        assert_eq!(short_cpu_name("AMD Ryzen 9 5900X 12-Core Processor"), "Ryzen 9 5900X");
    }
    #[test]
    fn sensors() {
        assert_eq!(pretty_sensor("Tctl"), "CPU (Tctl)");
        assert_eq!(pretty_sensor("nvme Composite"), "NVMe");
        assert_eq!(pretty_sensor("raro"), "raro");
    }
}

fn push<T>(q: &mut VecDeque<T>, v: T) {
    if q.len() == HIST {
        q.pop_front();
    }
    q.push_back(v);
}

