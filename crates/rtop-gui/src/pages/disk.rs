//! Disco: actividad, particiones y procesos que leen/escriben.

use super::prelude::*;

impl App {
    pub(crate) fn page_disk(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;

        stat_row(
            ui,
            th,
            "disk_stats",
            &[
                Stat::new("Lectura", rate_fmt(s.disk_read_rate), c(th.accent)),
                Stat::new("Escritura", rate_fmt(s.disk_write_rate), c(th.mem)),
                Stat::new("Espacio usado en /", format!("{:.0}%", s.disk_pct()), Grad::Heat.at(th, s.disk_pct() / 100.0))
                    .detail(format!("{} de {}", human(s.disk_used), human(s.disk_total)))
                    .meter(s.disk_pct(), Grad::Heat),
            ],
        );

        surface(ui, th, |ui| {
            section(ui, th, "Actividad", None);
            chart(
                ui,
                th,
                &ChartOpts { floor: 64.0 * 1024.0, binary: true, ..self.opts(170.0, None, rate_fmt) },
                &[Series::new("lectura", tail_f64(&m.hist.disk_r, WINDOW), Grad::Mono(th.accent))],
                Some(&Series::new("escritura", tail_f64(&m.hist.disk_w, WINDOW), Grad::Mono(th.mem))),
            );
            legend(ui, th, &[("lectura (arriba)".into(), c(th.accent)), ("escritura (abajo)".into(), c(th.mem))]);
        });

        surface(ui, th, |ui| {
            section(ui, th, "Discos y particiones", None);
            for d in &s.devs {
                ui.horizontal(|ui| {
                    ui.label(strong(d.model.clone().unwrap_or(d.name.clone()), c(th.text), 15.0));
                    ui.label(meta(th, &d.name));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(meta(th, format!("leído {} y escrito {} desde el arranque", human(d.read_total), human(d.write_total))));
                    });
                });
            }
            ui.add_space(4.0);
            egui::Grid::new("mounts").num_columns(4).spacing([20.0, 8.0]).show(ui, |ui| {
                for h in ["Montada en", "Partición", "Ocupado", ""] {
                    ui.label(meta(th, h));
                }
                ui.end_row();
                for mnt in &s.mounts {
                    let p = pct(mnt.used, mnt.total);
                    ui.label(body(th, &mnt.mount));
                    ui.label(meta(th, format!("{} ({})", mnt.device, mnt.fs)));
                    meter(ui, th, 240.0, p, Grad::Heat);
                    ui.label(num(format!("{} de {} ({p:.0}%)", human(mnt.used), human(mnt.total)), c(th.text)));
                    ui.end_row();
                }
            });
        });

        surface(ui, th, |ui| {
            section(ui, th, "Los que más leen y escriben", None);
            let top: Vec<&Proc> = m.top(SortBy::Disk, 10).into_iter().filter(|p| p.disk_read + p.disk_write > 0.0).collect();
            if top.is_empty() {
                ui.label(meta(th, "Ninguno de tus procesos está usando el disco ahora. Los del sistema (root) no se pueden ver sin permisos."));
            } else {
                let rows = top
                    .iter()
                    .map(|p| {
                        {
                            let (cpu, mem, time) = self.base_cols(p);
                            (p.pid, p.name.clone(), vec![Col::Text(rate_fmt(p.disk_read), c(th.accent)), Col::Text(rate_fmt(p.disk_write), c(th.mem)), cpu, mem, time])
                        }
                    })
                    .collect();
                self.proc_table(ui, "disk_top", &[("Lectura", w::RATE), ("Escritura", w::RATE), ("CPU", w::METER), ("Memoria", w::MEM), ("Tiempo activo", w::TIME)], rows);
            }
        });
    }
}
