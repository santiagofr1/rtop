//! Throughput de disco a partir de `/proc/diskstats` (solo discos enteros).

use std::fs;

/// Bytes leídos/escritos acumulados de un disco desde el arranque.
#[derive(Clone, Debug)]
pub struct DevTotals {
    pub name: String,
    pub read: u64,
    pub write: u64,
}

pub fn devices() -> Vec<DevTotals> {
    fs::read_to_string("/proc/diskstats").map(|s| parse_diskstats(&s, is_whole_disk)).unwrap_or_default()
}

/// Una fila por disco que cumpla `keep` (se le pasa el nombre).
pub fn parse_diskstats(s: &str, keep: impl Fn(&str) -> bool) -> Vec<DevTotals> {
    s.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 10 || !keep(f[2]) {
                return None;
            }
            // campos 6 y 10: sectores leídos / escritos (siempre de 512 bytes)
            Some(DevTotals {
                name: f[2].to_string(),
                read: f[5].parse::<u64>().unwrap_or(0) * 512,
                write: f[9].parse::<u64>().unwrap_or(0) * 512,
            })
        })
        .collect()
}

fn is_whole_disk(name: &str) -> bool {
    if ["loop", "ram", "zram", "dm-", "md", "sr", "fd"].iter().any(|p| name.starts_with(p)) {
        return false;
    }
    // en /sys/block solo aparecen discos enteros, no particiones
    std::path::Path::new("/sys/block").join(name).exists()
}

/// Modelo del disco, ej. "Samsung SSD 980 1TB".
pub fn model(name: &str) -> Option<String> {
    let m = fs::read_to_string(format!("/sys/block/{name}/device/model")).ok()?;
    let m = m.trim();
    (!m.is_empty()).then(|| m.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_diskstats() {
        let s = " 259       0 nvme0n1 1000 0 2048 0 500 0 4096 0 0 0 0\n 259       1 nvme0n1p1 10 0 8 0 1 0 8 0 0 0 0\n roto\n";
        let d = parse_diskstats(s, |n| !n.contains('p'));
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].name.as_str(), d[0].read, d[0].write), ("nvme0n1", 2048 * 512, 4096 * 512));
    }
}
