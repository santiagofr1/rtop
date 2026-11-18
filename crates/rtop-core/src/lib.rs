//! Recolección de datos del sistema + config/temas, compartido por la TUI y la GUI.

pub mod config;
pub mod fmt;

use std::{
    collections::VecDeque,
    time::Instant,
};

use sysinfo::{
    Components, Networks, Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, Signal,
    System, Users,
};

pub use config::{Config, Rgb, Theme};

pub const HIST: usize = 300;

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
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortBy {
    Cpu,
    Mem,
    Time,
    Pid,
    Name,
}

impl SortBy {
    pub const ALL: [SortBy; 5] = [SortBy::Cpu, SortBy::Mem, SortBy::Time, SortBy::Pid, SortBy::Name];

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|s| *s == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
    pub fn label(self) -> &'static str {
        match self {
            SortBy::Cpu => "CPU%",
            SortBy::Mem => "MEM%",
            SortBy::Time => "TIEMPO",
            SortBy::Pid => "PID",
            SortBy::Name => "NOMBRE",
        }
    }
}

/// Snapshot del sistema (lo que no son procesos).
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub host: String,
    pub cpu_brand: String,
    pub cores: Vec<f32>,
    pub cpu_total: f32,
    pub mem_total: u64,
    pub mem_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub load: (f64, f64, f64),
    pub uptime: u64,
    pub rx_rate: f64,
    pub tx_rate: f64,
    pub temp_max: Option<f32>,
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
}

fn pct(a: u64, b: u64) -> f32 {
    if b == 0 { 0.0 } else { a as f32 * 100.0 / b as f32 }
}

pub struct Monitor {
    sys: System,
    users: Users,
    nets: Networks,
    comps: Components,
    last: Instant,
    pub stats: Stats,
    pub procs: Vec<Proc>,
    pub cpu_hist: VecDeque<f32>,
    pub mem_hist: VecDeque<f32>,
    pub rx_hist: VecDeque<f64>,
    pub tx_hist: VecDeque<f64>,
}

impl Monitor {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        sys.refresh_all();
        let mut m = Monitor {
            sys,
            users: Users::new_with_refreshed_list(),
            nets: Networks::new_with_refreshed_list(),
            comps: Components::new_with_refreshed_list(),
            last: Instant::now(),
            stats: Stats::default(),
            procs: Vec::new(),
            cpu_hist: VecDeque::with_capacity(HIST),
            mem_hist: VecDeque::with_capacity(HIST),
            rx_hist: VecDeque::with_capacity(HIST),
            tx_hist: VecDeque::with_capacity(HIST),
        };
        m.collect(0.0);
        m
    }

    pub fn refresh(&mut self) {
        let dt = self.last.elapsed().as_secs_f64().max(0.001);
        self.last = Instant::now();
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything(),
        );
        self.nets.refresh(true);
        self.comps.refresh(false);
        self.collect(dt);
    }

    fn collect(&mut self, dt: f64) {
        let (rx, tx) = self
            .nets
            .iter()
            .filter(|(n, _)| *n != "lo")
            .fold((0u64, 0u64), |(r, t), (_, d)| (r + d.received(), t + d.transmitted()));
        let (rx_rate, tx_rate) = if dt > 0.0 { (rx as f64 / dt, tx as f64 / dt) } else { (0.0, 0.0) };

        let mem_total = self.sys.total_memory();
        let mt = mem_total.max(1) as f32;
        let users = &self.users;
        self.procs = self
            .sys
            .processes()
            .values()
            .filter(|p| p.thread_kind().is_none())
            .map(|p| Proc {
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
                cmd: p
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
            })
            .collect();

        let la = System::load_average();
        let cpus = self.sys.cpus();
        self.stats = Stats {
            host: System::host_name().unwrap_or_else(|| "sistema".into()),
            cpu_brand: cpus.first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
            cores: cpus.iter().map(|c| c.cpu_usage()).collect(),
            cpu_total: self.sys.global_cpu_usage(),
            mem_total,
            mem_used: self.sys.used_memory(),
            swap_total: self.sys.total_swap(),
            swap_used: self.sys.used_swap(),
            load: (la.one, la.five, la.fifteen),
            uptime: System::uptime(),
            rx_rate,
            tx_rate,
            temp_max: self
                .comps
                .iter()
                .filter_map(|c| c.temperature())
                .filter(|t| t.is_finite())
                .reduce(f32::max),
            tasks: self.procs.len(),
            running: self.procs.iter().filter(|p| p.state == State::Run).count(),
        };

        push(&mut self.cpu_hist, self.stats.cpu_total);
        push(&mut self.mem_hist, self.stats.mem_pct());
        push(&mut self.rx_hist, rx_rate);
        push(&mut self.tx_hist, tx_rate);
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
            };
            if reverse { o.reverse() } else { o }
        });
        idx
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

fn push<T>(q: &mut VecDeque<T>, v: T) {
    if q.len() == HIST {
        q.pop_front();
    }
    q.push_back(v);
}
