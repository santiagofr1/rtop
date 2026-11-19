//! NVIDIA vía `nvidia-smi` (tarda ~40-60 ms, por eso corre en el hilo de GPU).

use std::process::Command;

use super::{Gpu, GpuProc, Vendor};

const QUERY: &str = "--query-gpu=index,pci.bus_id,name,utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,power.limit,clocks.gr,clocks.mem,fan.speed,pstate";

pub(super) fn read(with_procs: bool) -> Vec<Gpu> {
    let run = |args: &[&str]| Command::new("nvidia-smi").args(args).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    let Some(q) = run(&[QUERY, "--format=csv,noheader,nounits"]) else { return vec![] };
    let mut gpus = parse_query(&q);
    if with_procs {
        if let Some(p) = run(&["pmon", "-c", "1", "-s", "um"]) {
            add_pmon(&mut gpus, &p);
        }
    }
    gpus
}

/// Salida CSV de `--query-gpu` (una línea por placa, en el orden de QUERY).
fn parse_query(out: &str) -> Vec<Gpu> {
    let num = |s: &str| s.trim().parse::<f32>().ok();
    out.lines()
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
        .collect()
}

/// `nvidia-smi pmon -s um`: gpu pid type sm mem enc dec jpg ofa fb ccpm command
fn add_pmon(gpus: &mut [Gpu], out: &str) {
    for l in out.lines().filter(|l| !l.starts_with('#')) {
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
    for g in gpus {
        super::sort_procs(&mut g.procs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_query_and_pmon() {
        let q = "0, 00000000:01:00.0, NVIDIA GeForce RTX 4050 Laptop GPU, 12, 61, 6141, 50, 4.07, 35.00, 210, 405, [N/A], P8\n";
        let mut g = parse_query(q);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].id, "0000:01:00.0");
        assert_eq!((g[0].busy, g[0].vram_used, g[0].fan_pct), (Some(12.0), 61 * 1024 * 1024, None));
        assert_eq!(g[0].pstate.as_deref(), Some("P8"));

        let pmon = "# gpu         pid   type     sm    mem    enc    dec    jpg    ofa     fb   ccpm    command\n\
                    # Idx           #    C/G      %      %      %      %      %      %     MB     MB    name\n\
                    0       1910     G     15      3      -      -      -      -     41      0    gnome-shell\n\
                    0       2464     G      -      -      -      -      -      -      5      0    Xwayland\n";
        add_pmon(&mut g, pmon);
        assert_eq!(g[0].procs.len(), 2);
        assert_eq!((g[0].procs[0].pid, g[0].procs[0].usage, g[0].procs[0].vram), (1910, Some(15.0), 41 * 1024 * 1024));
        assert_eq!(g[0].procs[1].usage, None);
    }
}
