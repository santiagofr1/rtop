//! Datos extra de un proceso, leídos de `/proc/<pid>` solo para el elegido.

use std::fs;

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
        let link = |f: &str| fs::read_link(format!("{base}/{f}")).ok().map(|p| p.to_string_lossy().into_owned());
        let mut d = ProcDetails::default();
        if let Ok(st) = fs::read_to_string(format!("{base}/status")) {
            d.parse_status(&st);
        }
        if let Ok(st) = fs::read_to_string(format!("{base}/stat")) {
            d.nice = parse_nice(&st);
        }
        d.exe = link("exe");
        d.cwd = link("cwd");
        d
    }

    fn parse_status(&mut self, st: &str) {
        for l in st.lines() {
            let Some((k, v)) = l.split_once(':') else { continue };
            let v = v.trim();
            match k {
                "Threads" => self.threads = v.parse().ok(),
                "VmSwap" => self.swap = v.split_whitespace().next().and_then(|x| x.parse::<u64>().ok()).map(|kb| kb * 1024),
                _ => {}
            }
        }
    }
}

/// `/proc/pid/stat`: el campo 19 es nice. El nombre va entre paréntesis y
/// puede tener espacios, por eso se cuenta desde el último ')'.
fn parse_nice(stat: &str) -> Option<i32> {
    stat.rsplit_once(')')?.1.split_whitespace().nth(16)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_and_nice() {
        let mut d = ProcDetails::default();
        d.parse_status("Name:\tfoo\nThreads:\t12\nVmSwap:\t  2048 kB\n");
        assert_eq!((d.threads, d.swap), (Some(12), Some(2048 * 1024)));
        let stat = "123 (Web Content (x)) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 20 -5 7";
        assert_eq!(parse_nice(stat), Some(-5));
        assert_eq!(parse_nice("basura"), None);
    }
}
