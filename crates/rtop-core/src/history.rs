//! Historial de las últimas `HIST` muestras de cada serie, para los gráficos.

use std::collections::{BTreeMap, VecDeque};

use crate::model::{pct, Stats};

pub const HIST: usize = 300;

pub type Hist<T> = VecDeque<T>;

/// Series de una GPU, por id (dirección PCI).
#[derive(Default)]
pub struct GpuHist {
    pub busy: Hist<f32>,
    pub vram: Hist<f32>,
    pub temp: Hist<f32>,
    pub power: Hist<f32>,
    pub clock: Hist<f32>,
}

/// (bajada o lectura, subida o escritura)
pub type RatePair = (Hist<f64>, Hist<f64>);

#[derive(Default)]
pub struct History {
    /// uso total de CPU en %
    pub cpu: Hist<f32>,
    pub cores: Vec<Hist<f32>>,
    /// MHz promedio
    pub freq: Hist<f32>,
    pub load: Hist<f32>,
    /// % de RAM en uso, en caché y de swap
    pub mem: Hist<f32>,
    pub cache: Hist<f32>,
    pub swap: Hist<f32>,
    /// bytes/s totales de red y disco
    pub rx: Hist<f64>,
    pub tx: Hist<f64>,
    pub disk_r: Hist<f64>,
    pub disk_w: Hist<f64>,
    /// por interfaz de red
    pub nets: BTreeMap<String, RatePair>,
    /// por disco
    pub devs: BTreeMap<String, RatePair>,
    /// por nombre de sensor
    pub temps: BTreeMap<String, Hist<f32>>,
    /// uso y VRAM en %, mismo orden que `stats.gpus`
    pub gpu: Vec<Hist<f32>>,
    pub vram: Vec<Hist<f32>>,
    pub gpu_detail: BTreeMap<String, GpuHist>,
}

impl History {
    pub fn record(&mut self, s: &Stats) {
        push(&mut self.cpu, s.cpu_total);
        self.cores.resize_with(s.cores.len(), Hist::new);
        for (h, &v) in self.cores.iter_mut().zip(&s.cores) {
            push(h, v);
        }
        push(&mut self.freq, s.cpu_freq as f32);
        push(&mut self.load, s.load.0 as f32);
        push(&mut self.mem, s.mem_pct());
        push(&mut self.cache, pct(s.meminfo.cache(), s.meminfo.total));
        push(&mut self.swap, s.swap_pct());
        push(&mut self.rx, s.rx_rate);
        push(&mut self.tx, s.tx_rate);
        push(&mut self.disk_r, s.disk_read_rate);
        push(&mut self.disk_w, s.disk_write_rate);
        for n in &s.nets {
            push_pair(self.nets.entry(n.name.clone()).or_default(), n.rx_rate, n.tx_rate);
        }
        for d in &s.devs {
            push_pair(self.devs.entry(d.name.clone()).or_default(), d.read_rate, d.write_rate);
        }
        for t in &s.temps {
            push(self.temps.entry(t.label.clone()).or_default(), t.value);
        }
        let n = s.gpus.len();
        self.gpu.resize_with(n, Hist::new);
        self.vram.resize_with(n, Hist::new);
        for (i, g) in s.gpus.iter().enumerate() {
            push(&mut self.gpu[i], g.busy.unwrap_or(0.0));
            push(&mut self.vram[i], g.vram_pct());
            let d = self.gpu_detail.entry(g.id.clone()).or_default();
            push(&mut d.busy, g.busy.unwrap_or(0.0));
            push(&mut d.vram, g.vram_pct());
            push(&mut d.temp, g.temp.unwrap_or(0.0));
            push(&mut d.power, g.power_w.unwrap_or(0.0));
            push(&mut d.clock, g.clock_mhz.unwrap_or(0.0));
        }
    }
}

fn push_pair(p: &mut RatePair, a: f64, b: f64) {
    push(&mut p.0, a);
    push(&mut p.1, b);
}

pub(crate) fn push<T>(q: &mut Hist<T>, v: T) {
    if q.len() == HIST {
        q.pop_front();
    }
    q.push_back(v);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_last_hist() {
        let mut q = Hist::new();
        for i in 0..HIST + 5 {
            push(&mut q, i);
        }
        assert_eq!(q.len(), HIST);
        assert_eq!(q.front(), Some(&5));
    }
}
