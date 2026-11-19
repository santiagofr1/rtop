//! Procesador: uso total, por núcleo, frecuencia, temperatura y procesos.

use super::prelude::*;

impl App {
    pub(crate) fn page_cpu(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;
        let heat = |v: f32| Grad::Heat.at(th, v / 100.0);
        ui.label(meta(th, &s.cpu_brand));

        stat_row(
            ui,
            th,
            "cpu_stats",
            &[
                Stat::new("Uso", format!("{:.0}%", s.cpu_total), heat(s.cpu_total)).meter(s.cpu_total, Grad::Heat),
                Stat::new("Frecuencia", mhz_fmt(s.cpu_freq as f64), c(th.text)).detail("promedio de los núcleos"),
                Stat::new(
                    "Temperatura",
                    s.cpu_temp.map(|t| format!("{t:.0} °C")).unwrap_or("sin dato".into()),
                    s.cpu_temp.map(heat).unwrap_or(muted(th)),
                ),
                Stat::new("Carga", format!("{:.2}", s.load.0), c(th.text))
                    .detail(format!("en cola; 5 min {:.2}, 15 min {:.2}", s.load.1, s.load.2)),
                Stat::new("Procesos", s.tasks.to_string(), c(th.text)).detail(format!("{} corriendo ahora", s.running)),
            ],
        );

        surface(ui, th, |ui| {
            section(ui, th, "Uso total", None);
            chart(ui, th, &self.opts(170.0, Some(100.0), pct_fmt), &[Series::new("CPU", tail_f32(&m.hist.cpu, WINDOW), Grad::Heat)], None);
        });

        surface(ui, th, |ui| {
            section(ui, th, "Núcleos", Some(meta(th, format!("{} hilos", s.cores.len()))));
            self.core_bars(ui);
        });

        columns(ui, 2, 2, 12.0, |ui, i| {
            surface(ui, th, |ui| {
                if i == 0 {
                    section(ui, th, "Frecuencia", Some(strong(mhz_fmt(s.cpu_freq as f64), c(th.accent), 15.0)));
                    chart(
                        ui,
                        th,
                        &self.opts(110.0, None, mhz_fmt),
                        &[Series::new("frecuencia", tail_f32(&m.hist.freq, WINDOW), Grad::Mono(th.accent))],
                        None,
                    );
                } else {
                    let t = s.cpu_temp.unwrap_or(0.0);
                    section(ui, th, "Temperatura", Some(strong(format!("{t:.0} °C"), heat(t), 15.0)));
                    let data = s
                        .temps
                        .iter()
                        .find(|x| Some(x.value) == s.cpu_temp)
                        .and_then(|x| m.hist.temps.get(&x.label))
                        .map(|h| tail_f32(h, WINDOW))
                        .unwrap_or_default();
                    chart(ui, th, &self.opts(110.0, Some(100.0), temp_fmt), &[Series::new("temperatura", data, Grad::Heat)], None);
                }
            });
        });

        surface(ui, th, |ui| {
            section(ui, th, "Los que más CPU usan", Some(meta(th, self.cpu_hint())));
            let top = m.top(SortBy::Cpu, 10);
            self.proc_table(
                ui,
                "cpu_top",
                &[("CPU", w::METER), ("Memoria", w::MEM), ("GPU", w::GPU), ("Estado", w::STATE), ("Tiempo activo", w::TIME)],
                self.cpu_rows(&top),
            );
        });
    }

    /// Una barra vertical por núcleo, con % adentro y frecuencia abajo.
    fn core_bars(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let n = s.cores.len().max(1);
        let per_row = n.min(16);
        let rows = n.div_ceil(per_row);
        let w = ui.available_width();
        let gap = 8.0;
        let bar_w = (w - gap * (per_row as f32 - 1.0)) / per_row as f32;
        let bar_h = 96.0;
        let row_h = bar_h + 40.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, rows as f32 * row_h), Sense::hover());
        let p = ui.painter_at(rect);
        let mut hovered = None;
        for (i, &u) in s.cores.iter().enumerate() {
            let (r, col) = (i / per_row, i % per_row);
            let x = rect.left() + col as f32 * (bar_w + gap);
            let y = rect.top() + r as f32 * row_h;
            let bar = Rect::from_min_size(Pos2::new(x, y), Vec2::new(bar_w, bar_h));
            p.rect_filled(bar, CornerRadius::same(6), c(th.sel).gamma_multiply(0.6));
            let frac = (u / 100.0).clamp(0.0, 1.0);
            if frac > 0.005 {
                let mut f = bar;
                f.set_top(bar.bottom() - (bar_h * frac).max(3.0));
                p.rect_filled(f, CornerRadius::same(6), Grad::Heat.at(th, frac).gamma_multiply(0.85));
            }
            p.text(bar.center_top() + Vec2::new(0.0, 8.0), Align2::CENTER_TOP, format!("{u:.0}%"), FontId::proportional(14.0), c(th.text));
            p.text(
                Pos2::new(bar.center().x, bar.bottom() + 6.0),
                Align2::CENTER_TOP,
                format!("Núcleo {i}"),
                FontId::proportional(12.0),
                c(th.text),
            );
            p.text(
                Pos2::new(bar.center().x, bar.bottom() + 22.0),
                Align2::CENTER_TOP,
                mhz_fmt(*s.core_freq.get(i).unwrap_or(&0) as f64),
                FontId::proportional(11.0),
                muted(th),
            );
            if resp.hover_pos().is_some_and(|h| bar.contains(h)) {
                hovered = Some((i, u));
            }
        }
        if let Some((i, u)) = hovered {
            resp.on_hover_text(format!("Núcleo {i}: {u:.1}%"));
        }
    }
}
