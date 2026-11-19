//! Memoria: cómo se reparte la RAM y quién la usa.

use super::prelude::*;

impl App {
    pub(crate) fn page_mem(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let mi = &s.meminfo;
        let m = &self.mon;
        let used_pct = pct(mi.used(), mi.total);

        stat_row(
            ui,
            th,
            "mem_stats",
            &[
                Stat::new("En uso", human(mi.used()), Grad::Heat.at(th, used_pct / 100.0))
                    .detail(format!("{used_pct:.0}% de {}", human(mi.total)))
                    .meter(used_pct, Grad::Heat),
                Stat::new("Disponible", human(mi.available), c(th.text)).detail("lo que pueden pedir los programas"),
                Stat::new("Caché", human(mi.cache()), c(th.accent)).detail("archivos recientes; se libera solo"),
                Stat::new("Swap", human(s.swap_used), Grad::Heat.at(th, s.swap_pct() / 100.0))
                    .detail(format!("de {}", human(s.swap_total)))
                    .meter(s.swap_pct(), Grad::Heat),
            ],
        );

        surface(ui, th, |ui| {
            section(ui, th, "Cómo se reparte", None);
            let w = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 20.0), Sense::hover());
            let p = ui.painter();
            let parts = [
                (mi.used(), c(th.mem), "En uso por programas"),
                (mi.cache(), c(th.accent), "Caché y buffers"),
                (mi.free, c(th.sel), "Libre"),
            ];
            // las partes se solapan un poco en /proc/meminfo: se reparten sobre su suma
            let total = parts.iter().map(|p| p.0).sum::<u64>().max(1) as f32;
            let mut x = rect.left();
            for (v, col, _) in parts {
                let ww = rect.width() * v as f32 / total;
                p.rect_filled(Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(ww, rect.height())), CornerRadius::same(4), col);
                x += ww;
            }
            legend(ui, th, &parts.iter().map(|(v, col, n)| (format!("{n}: {}", human(*v)), *col)).collect::<Vec<_>>());
            ui.add_space(6.0);
            chart(
                ui,
                th,
                &self.opts(150.0, Some(100.0), pct_fmt),
                &[
                    Series::new(
                        "en uso + caché",
                        m.hist.mem.iter().zip(&m.hist.cache).rev().take(WINDOW).rev().map(|(a, b)| (a + b).min(100.0)).collect(),
                        Grad::Mono(th.accent),
                    ),
                    Series::new("en uso", tail_f32(&m.hist.mem, WINDOW), Grad::Mono(th.mem)),
                ],
                None,
            );
        });

        surface(ui, th, |ui| {
            section(ui, th, "Los que más memoria usan", None);
            let top = m.top(SortBy::Mem, 12);
            self.proc_table(
                ui,
                "mem_top",
                &[("Memoria", w::MEM), ("% del total", w::SMALL), ("VRAM", w::VRAM), ("CPU", w::METER), ("Tiempo activo", w::TIME)],
                self.mem_rows(&top),
            );
        });

        surface(ui, th, |ui| {
            section(ui, th, "Detalle", None);
            kv(
                ui,
                th,
                "mem_kv",
                &[
                    ("Total instalada", human(mi.total)),
                    ("En uso", format!("{} ({used_pct:.0}%)", human(mi.used()))),
                    ("Disponible", human(mi.available)),
                    ("Caché de archivos", human(mi.cached)),
                    ("Buffers", human(mi.buffers)),
                    ("Compartida entre procesos", human(mi.shmem)),
                    ("Pendiente de escribir a disco", human(mi.dirty)),
                    ("Swap", format!("{} de {}", human(s.swap_used), human(s.swap_total))),
                ],
            );
        });
    }
}
