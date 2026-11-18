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
    let Ok(s) = fs::read_to_string("/proc/diskstats") else { return vec![] };
    s.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 10 || !is_whole_disk(f[2]) {
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
