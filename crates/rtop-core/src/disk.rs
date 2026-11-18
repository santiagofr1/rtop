//! Throughput de disco a partir de `/proc/diskstats` (solo discos enteros).

use std::fs;

/// (bytes leídos, bytes escritos) acumulados desde el arranque.
pub fn totals() -> (u64, u64) {
    let Ok(s) = fs::read_to_string("/proc/diskstats") else { return (0, 0) };
    let mut r = 0u64;
    let mut w = 0u64;
    for l in s.lines() {
        let f: Vec<&str> = l.split_whitespace().collect();
        if f.len() < 10 {
            continue;
        }
        let name = f[2];
        if !is_whole_disk(name) {
            continue;
        }
        // campos 6 y 10: sectores leídos / escritos (siempre de 512 bytes)
        r += f[5].parse::<u64>().unwrap_or(0) * 512;
        w += f[9].parse::<u64>().unwrap_or(0) * 512;
    }
    (r, w)
}

fn is_whole_disk(name: &str) -> bool {
    if ["loop", "ram", "zram", "dm-", "md", "sr", "fd"].iter().any(|p| name.starts_with(p)) {
        return false;
    }
    // en /sys/block solo aparecen discos enteros, no particiones
    std::path::Path::new("/sys/block").join(name).exists()
}
