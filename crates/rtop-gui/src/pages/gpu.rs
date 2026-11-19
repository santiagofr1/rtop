//! Una GPU: uso, VRAM, temperatura, potencia y procesos.

use super::prelude::*;

impl App {
    pub(crate) fn page_gpu(&self, ui: &mut Ui, idx: usize) {
        let th = &self.th;
        let s = &self.mon.stats;
        let Some(g) = s.gpus.get(idx) else {
            ui.label(meta(th, "No se encontró esta GPU."));
            return;
        };
        let h = self.mon.hist.gpu_detail.get(&g.id);
        let heat = |v: f32| Grad::Heat.at(th, v / 100.0);
        let kind = if g.integrated { "integrada en el procesador" } else { "dedicada" };
        ui.label(meta(th, format!("{}, {kind}, PCI {}", g.name, g.id)));

        if g.asleep {
            surface(ui, th, |ui| {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(strong("Dormida", c(th.ok), 22.0));
                    ui.label(body(th, "Está en ahorro de energía porque ningún programa la usa."));
                    ui.label(meta(th, "rtop no la despierta para no gastar batería. Si se despierta, acá vas a ver quién la usa."));
                });
                ui.add_space(20.0);
            });
            return;
        }

        let power_max = g
            .power_limit_w
            .or_else(|| h.map(|h| h.power.iter().copied().fold(0.0f32, f32::max) * 1.25))
            .unwrap_or(1.0)
            .max(1.0);
        let busy = g.busy.unwrap_or(0.0);
        let mut stats = vec![
            Stat::new("Uso", format!("{busy:.0}%"), heat(busy)).meter(busy, Grad::Heat),
            Stat::new("VRAM", human(g.vram_used), c(th.mem))
                .detail(format!("de {} ({:.0}%)", human(g.vram_total), g.vram_pct()))
                .meter(g.vram_pct(), Grad::Mono(th.mem)),
            Stat::new("Temperatura", g.temp.map(|t| format!("{t:.0} °C")).unwrap_or("sin dato".into()), g.temp.map(heat).unwrap_or(muted(th))),
            Stat::new("Potencia", g.power_w.map(|p| format!("{p:.1} W")).unwrap_or("sin dato".into()), c(th.text))
                .detail(g.power_limit_w.map(|l| format!("límite {l:.0} W")).unwrap_or_default()),
        ];
        if !g.integrated {
            stats.push(
                Stat::new("Estado", "Despierta", c(th.warn))
                    .detail(g.pstate.as_ref().map(|p| format!("modo {p} (P0 rápido, P8 reposo)")).unwrap_or_default()),
            );
        }
        stat_row(ui, th, "gpu_stats", &stats);

        let series = |q: Option<&rtop_core::Hist<f32>>| q.map(|q| tail_f32(q, WINDOW)).unwrap_or_default();
        surface(ui, th, |ui| {
            section(ui, th, "Uso", Some(strong(format!("{busy:.0}%"), heat(busy), 15.0)));
            chart(ui, th, &self.opts(150.0, Some(100.0), pct_fmt), &[Series::new("uso", series(h.map(|h| &h.busy)), Grad::Heat)], None);
        });
        surface(ui, th, |ui| {
            section(
                ui,
                th,
                "VRAM",
                Some(strong(format!("{} de {}", human(g.vram_used), human(g.vram_total)), c(th.mem), 15.0)),
            );
            // en % del total, así la escala no salta
            let mut o = self.opts(110.0, Some(100.0), pct_fmt);
            o.floor = 100.0;
            chart(ui, th, &o, &[Series::new("VRAM", series(h.map(|h| &h.vram)), Grad::Mono(th.mem))], None);
        });

        columns(ui, 2, 2, 12.0, |ui, i| {
            surface(ui, th, |ui| {
                if i == 0 {
                    let t = g.temp.unwrap_or(0.0);
                    section(ui, th, "Temperatura", Some(strong(format!("{t:.0} °C"), heat(t), 15.0)));
                    chart(ui, th, &self.opts(100.0, Some(100.0), temp_fmt), &[Series::new("temperatura", series(h.map(|h| &h.temp)), Grad::Heat)], None);
                } else {
                    section(ui, th, "Potencia", Some(strong(g.power_w.map(|p| format!("{p:.1} W")).unwrap_or("-".into()), c(th.warn), 15.0)));
                    chart(
                        ui,
                        th,
                        &self.opts(100.0, Some(power_max), watt_fmt),
                        &[Series::new("potencia", series(h.map(|h| &h.power)), Grad::Mono(th.warn))],
                        None,
                    );
                }
            });
        });

        surface(ui, th, |ui| {
            section(ui, th, "Procesos que la usan", Some(meta(th, match g.procs.len() { 1 => "1 proceso".to_string(), n => format!("{n} procesos") })));
            let rows = g
                .procs
                .iter()
                .take(12)
                .map(|gp| {
                    let u = gp.usage.unwrap_or(0.0);
                    let mut cols = vec![
                        Col::Meter(u, Grad::Heat, gp.usage.map(|u| format!("{u:.1}%")).unwrap_or("sin dato".into())),
                        Col::Text(human(gp.vram), c(th.mem)),
                    ];
                    match self.mon.proc(gp.pid) {
                        Some(p) => {
                            let (cpu, mem, time) = self.base_cols(p);
                            cols.extend([cpu, mem, time]);
                        }
                        None => cols.extend((0..3).map(|_| Col::Text(String::new(), muted(th)))),
                    }
                    (gp.pid, gp.name.clone(), cols)
                })
                .collect();
            self.proc_table(
                ui,
                "gpu_procs",
                &[("Uso de GPU", w::METER), ("VRAM", w::VRAM), ("CPU", w::METER), ("Memoria", w::MEM), ("Tiempo activo", w::TIME)],
                rows,
            );
        });

        surface(ui, th, |ui| {
            section(ui, th, "Detalle", None);
            let opt = |v: Option<f32>, f: fn(f64) -> String| v.map(|x| f(x as f64)).unwrap_or("sin dato".into());
            let mut rows = vec![
                ("Fabricante", g.vendor.label().to_string()),
                ("Reloj del núcleo", opt(g.clock_mhz, mhz_fmt)),
                ("Reloj de la memoria", opt(g.mem_clock_mhz, mhz_fmt)),
                ("Ventilador", g.fan_pct.map(|f| format!("{f:.0}%")).unwrap_or("sin dato (lo maneja la laptop)".into())),
                ("Potencia", match g.power_limit_w { Some(l) => format!("{} de {}", opt(g.power_w, watt_fmt), watt_fmt(l as f64)), None => opt(g.power_w, watt_fmt) }),
                ("VRAM", format!("{} de {}", human(g.vram_used), human(g.vram_total))),
            ];
            if g.shared_total > 0 {
                rows.push(("Memoria compartida", format!("{} de {} (toma RAM)", human(g.shared_used), human(g.shared_total))));
            }
            kv(ui, th, "gpu_kv", &rows);
        });
    }
}
