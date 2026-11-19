//! `Monitor`: junta todas las fuentes (sysinfo, `/proc`, `/sys`, GPUs) en una
//! foto (`Stats` + `procs`) y guarda el historial. Las interfaces llaman a
//! `refresh()` una vez por intervalo y después solo leen.

use std::{collections::HashMap, time::Instant};

use sysinfo::{Components, DiskKind, Disks, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System, UpdateKind, Users};

use crate::{
    disk,
    gpu::{self, Gpu},
    history::History,
    meminfo::MemInfo,
    model::{DiskDev, Mount, NetIf, Proc, SortBy, State, Stats, Temp},
    names::{pretty_sensor, short_cpu_name},
};

pub struct Monitor {
    sys: System,
    users: Users,
    nets: Networks,
    comps: Components,
    disks: Disks,
    gpu: gpu::GpuPoller,
    /// bytes leídos/escritos por disco en la muestra anterior
    dev_prev: HashMap<String, (u64, u64)>,
    ticks: u64,
    last: Instant,
    pub stats: Stats,
    pub procs: Vec<Proc>,
    pub hist: History,
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
            hist: History::default(),
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
        if self.ticks.is_multiple_of(3) {
            self.comps.refresh(false);
        }
        self.disks.refresh(false);
        self.collect(dt);
    }

    fn collect(&mut self, dt: f64) {
        let rate = |b: u64| if dt > 0.0 { b as f64 / dt } else { 0.0 };
        let nets = self.collect_nets(rate);
        let devs = self.collect_devs(rate);
        let mounts = self.collect_mounts();
        let temps = self.collect_temps();
        let gpus = self.gpu.latest();
        self.procs = self.collect_procs(rate, &gpus);

        let root = mounts.iter().find(|m| m.mount == "/").or_else(|| mounts.iter().max_by_key(|m| m.total));
        let (disk_total, disk_used) = root.map(|m| (m.total, m.used)).unwrap_or((0, 0));
        let cpu_temp = temps
            .iter()
            .find(|t| ["CPU", "Tctl", "Tdie", "Package"].iter().any(|k| t.label.contains(k)))
            .map(|t| t.value);
        let la = System::load_average();
        let cpus = self.sys.cpus();
        let brand = cpus.first().map(|c| c.brand()).unwrap_or("");
        let core_freq: Vec<u64> = cpus.iter().map(|c| c.frequency()).collect();
        self.stats = Stats {
            host: System::host_name().unwrap_or_else(|| "sistema".into()),
            os: System::long_os_version().unwrap_or_default(),
            kernel: System::kernel_version().unwrap_or_default(),
            cpu_brand: brand.trim().to_string(),
            cpu_model: short_cpu_name(brand),
            cpu_freq: if core_freq.is_empty() { 0 } else { core_freq.iter().sum::<u64>() / core_freq.len() as u64 },
            core_freq,
            cores: cpus.iter().map(|c| c.cpu_usage()).collect(),
            cpu_total: self.sys.global_cpu_usage(),
            mem_total: self.sys.total_memory(),
            mem_used: self.sys.used_memory(),
            swap_total: self.sys.total_swap(),
            swap_used: self.sys.used_swap(),
            meminfo: MemInfo::read(),
            load: (la.one, la.five, la.fifteen),
            uptime: System::uptime(),
            rx_rate: nets.iter().map(|n| n.rx_rate).sum(),
            tx_rate: nets.iter().map(|n| n.tx_rate).sum(),
            nets,
            temp_max: temps.iter().map(|t| t.value).reduce(f32::max),
            cpu_temp,
            temps,
            disk_read_rate: devs.iter().map(|d| d.read_rate).sum(),
            disk_write_rate: devs.iter().map(|d| d.write_rate).sum(),
            devs,
            mounts,
            disk_used,
            disk_total,
            gpus,
            tasks: self.procs.len(),
            running: self.procs.iter().filter(|p| p.state == State::Run).count(),
        };
        self.hist.record(&self.stats);
    }

    fn collect_nets(&self, rate: impl Fn(u64) -> f64) -> Vec<NetIf> {
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
        nets.sort_by(|a, b| a.is_virtual().cmp(&b.is_virtual()).then((b.total_rx + b.total_tx).cmp(&(a.total_rx + a.total_tx))));
        nets
    }

    fn collect_devs(&mut self, rate: impl Fn(u64) -> f64) -> Vec<DiskDev> {
        disk::devices()
            .into_iter()
            .map(|d| {
                let (pr, pw) = self.dev_prev.insert(d.name.clone(), (d.read, d.write)).unwrap_or((d.read, d.write));
                DiskDev {
                    model: disk::model(&d.name),
                    read_rate: rate(d.read.saturating_sub(pr)),
                    write_rate: rate(d.write.saturating_sub(pw)),
                    read_total: d.read,
                    write_total: d.write,
                    name: d.name,
                }
            })
            .collect()
    }

    fn collect_mounts(&self) -> Vec<Mount> {
        self.disks
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
            .collect()
    }

    fn collect_temps(&self) -> Vec<Temp> {
        self.comps
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
            .collect()
    }

    fn collect_procs(&self, rate: impl Fn(u64) -> f64, gpus: &[Gpu]) -> Vec<Proc> {
        // pid -> (uso sumado, vram sumada) entre todas las GPUs
        let mut gpu_use: HashMap<u32, (Option<f32>, u64)> = HashMap::new();
        for gp in gpus.iter().flat_map(|g| &g.procs) {
            let e = gpu_use.entry(gp.pid).or_insert((None, 0));
            if let Some(u) = gp.usage {
                e.0 = Some(e.0.unwrap_or(0.0) + u);
            }
            e.1 += gp.vram;
        }
        let mt = self.sys.total_memory().max(1) as f32;
        self.sys
            .processes()
            .values()
            .filter(|p| p.thread_kind().is_none())
            .map(|p| {
                let du = p.disk_usage();
                let pid = p.pid().as_u32();
                let (gpu, vram) = gpu_use.get(&pid).copied().unwrap_or((None, 0));
                Proc {
                    pid,
                    user: p
                        .user_id()
                        .and_then(|u| self.users.get_user_by_id(u))
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
                    gpu,
                    vram,
                }
            })
            .collect()
    }

    /// Procesos filtrados y ordenados (índices sobre `self.procs`).
    pub fn view(&self, filter: &str, sort: SortBy, reverse: bool) -> Vec<usize> {
        view(&self.procs, filter, sort, reverse)
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
        self.sys.process(Pid::from_u32(pid)).and_then(|p| p.kill_with(sig)).unwrap_or(false)
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

/// Filtra por nombre, comando, usuario o PID y ordena. Devuelve índices.
pub fn view(procs: &[Proc], filter: &str, sort: SortBy, reverse: bool) -> Vec<usize> {
    let f = filter.to_lowercase();
    let mut idx: Vec<usize> = procs
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
    idx.sort_by(|&a, &b| {
        let (a, b) = (&procs[a], &procs[b]);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: u32, name: &str, cpu: f32, mem: u64) -> Proc {
        Proc {
            pid,
            user: "yo".into(),
            cpu,
            mem,
            mem_pct: 0.0,
            state: State::Sleep,
            time: 0,
            name: name.into(),
            cmd: format!("/usr/bin/{name} --flag"),
            disk_read: 0.0,
            disk_write: 0.0,
            parent: None,
            start_time: 0,
            gpu: None,
            vram: 0,
        }
    }

    #[test]
    fn sorts_and_filters() {
        let ps = vec![p(1, "bash", 1.0, 30), p(2, "chrome", 50.0, 10), p(3, "Zed", 5.0, 20)];
        assert_eq!(view(&ps, "", SortBy::Cpu, false), vec![1, 2, 0]);
        assert_eq!(view(&ps, "", SortBy::Cpu, true), vec![0, 2, 1]);
        assert_eq!(view(&ps, "", SortBy::Mem, false), vec![0, 2, 1]);
        // el nombre se ordena sin distinguir mayúsculas
        assert_eq!(view(&ps, "", SortBy::Name, false), vec![0, 1, 2]);
        // filtra por nombre, comando o PID
        assert_eq!(view(&ps, "CHROME", SortBy::Cpu, false), vec![1]);
        assert_eq!(view(&ps, "--flag", SortBy::Pid, false), vec![0, 1, 2]);
        assert_eq!(view(&ps, "3", SortBy::Pid, false), vec![2]);
    }
}
