//! Desglose de la memoria según `/proc/meminfo`.

use std::fs;

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

    pub fn read() -> MemInfo {
        fs::read_to_string("/proc/meminfo").map(|s| Self::parse(&s)).unwrap_or_default()
    }

    pub fn parse(s: &str) -> MemInfo {
        let mut m = MemInfo::default();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_meminfo() {
        let m = MemInfo::parse(
            "MemTotal:       16000000 kB\nMemFree:  1000000 kB\nMemAvailable:  8000000 kB\nBuffers: 100000 kB\n\
             Cached:  5000000 kB\nShmem: 200000 kB\nSReclaimable: 300000 kB\nraro sin dos puntos\n",
        );
        assert_eq!(m.total, 16_000_000 * 1024);
        assert_eq!(m.used(), 8_000_000 * 1024);
        // caché + buffers + reclaimable - shmem
        assert_eq!(m.cache(), (5_000_000 + 100_000 + 300_000 - 200_000) * 1024);
    }

    #[test]
    fn empty_is_zero() {
        let m = MemInfo::parse("");
        assert_eq!((m.total, m.used(), m.cache()), (0, 0, 0));
    }
}
