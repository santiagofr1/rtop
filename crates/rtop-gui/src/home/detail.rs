//! Panel con todo lo que se sabe del proceso elegido.

use super::prelude::*;

impl App {
    /// Todo lo que se sabe del proceso elegido. Datos primero, acción al final.
    pub(super) fn detail(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let th = &th;
        let Some(pid) = self.sel.pid else { return };
        let Some(p) = self.mon.proc(pid).cloned() else {
            ui.label(meta(th, "El proceso terminó."));
            return;
        };
        let d = self.sel.details.clone().unwrap_or_default();
        let mut_c = muted(th);

        ui.horizontal(|ui| {
            ui.add(egui::Label::new(strong(&p.name, c(th.text), 20.0)).truncate());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Cerrar").on_hover_text("Esc").clicked() {
                    self.select(None);
                }
            });
        });
        ui.label(meta(th, format!("PID {}, de {}, {}", p.pid, p.user, state_name(p.state))));
        ui.add_space(6.0);

        // valores actuales, 2 x 2
        let cpu = self.cpu_of(&p);
        let disk = p.disk_read + p.disk_write;
        let cells = [
            ("CPU", format!("{cpu:.1}%"), Grad::Heat.at(th, cpu / 100.0)),
            ("Memoria", human(p.mem), c(th.mem)),
            (
                "GPU",
                p.gpu.map(|g| format!("{g:.1}%")).unwrap_or("no usa".into()),
                if p.gpu.is_some() { c(th.text) } else { mut_c },
            ),
            (
                "Disco",
                if disk > 0.0 { rate_fmt(disk) } else { "sin actividad".into() },
                if disk > 0.0 { c(th.accent) } else { mut_c },
            ),
        ];
        egui::Grid::new("detail_now").num_columns(2).spacing([28.0, 10.0]).show(ui, |ui| {
            for (i, (k, v, col)) in cells.iter().enumerate() {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(meta(th, *k));
                    ui.add(egui::Label::new(strong(v.clone(), *col, 18.0)).extend());
                });
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(8.0);

        // historia desde que lo elegiste
        let div = if self.cfg.cpu_per_core { 1.0 } else { self.mon.stats.cores.len().max(1) as f32 };
        let cpu_data: Vec<f32> = self.sel.cpu.iter().map(|v| v / div).collect();
        ui.label(meta(th, format!("CPU desde que lo elegiste ({})", self.cpu_hint())));
        chart(ui, th, &self.opts(70.0, Some(100.0), pct_fmt), &[Series::new("CPU", cpu_data, Grad::Heat)], None);
        let mem_data: Vec<f32> = self.sel.mem.iter().copied().collect();
        ui.label(meta(th, "Memoria (% del total)"));
        chart(ui, th, &self.opts(56.0, Some(100.0), pct_fmt), &[Series::new("memoria", mem_data, Grad::Mono(th.mem))], None);
        ui.add_space(8.0);

        ui.label(section_title(th, "Datos"));
        let mut rows = vec![
            ("Usuario", p.user.clone()),
            ("Activo hace", hms(p.time)),
            ("Hilos", d.threads.map(|t| t.to_string()).unwrap_or("sin dato".into())),
            ("Prioridad", d.nice.map(|n| format!("{n} (nice; menos es más prioridad)")).unwrap_or("sin dato".into())),
            ("Memoria", format!("{} ({:.1}% del total)", human(p.mem), p.mem_pct)),
            ("En swap", d.swap.map(human).unwrap_or("sin dato".into())),
            ("Disco", format!("lee {}, escribe {}", rate_fmt(p.disk_read), rate_fmt(p.disk_write))),
        ];
        for g in &self.mon.stats.gpus {
            if let Some(gp) = g.procs.iter().find(|x| x.pid == p.pid) {
                rows.push((
                    "GPU",
                    format!(
                        "{}: {}, VRAM {}",
                        g.short_name(),
                        gp.usage.map(|u| format!("{u:.1}%")).unwrap_or("sin dato".into()),
                        human(gp.vram)
                    ),
                ));
            }
        }
        kv(ui, th, "proc_kv", &rows);
        if let Some((ppid, pname)) = p.parent.and_then(|pp| self.mon.proc(pp).map(|x| (pp, x.name.clone()))) {
            ui.horizontal(|ui| {
                ui.label(meta(th, "Lo lanzó"));
                if ui.link(format!("{pname} ({ppid})")).on_hover_text("Ver ese proceso").clicked() {
                    self.select(Some(ppid));
                }
            });
        }
        ui.add_space(8.0);

        ui.label(section_title(th, "Ubicación"));
        let path = |ui: &mut Ui, k: &str, v: String| {
            ui.label(meta(th, k));
            ui.add(egui::Label::new(RichText::new(v).monospace().size(12.0).color(c(th.text))).wrap().selectable(true));
        };
        path(ui, "Ejecutable", d.exe.unwrap_or("no se puede leer (es de otro usuario)".into()));
        path(ui, "Carpeta de trabajo", d.cwd.unwrap_or("no se puede leer".into()));
        path(ui, "Comando", if p.cmd.is_empty() { p.name.clone() } else { p.cmd.clone() });
        ui.add_space(14.0);

        let kill = egui::Button::new(RichText::new("Terminar proceso…").color(c(th.hot)))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, c(th.hot).gamma_multiply(0.6)));
        ui.horizontal(|ui| {
            if ui.add(kill).clicked() {
                self.dialogs.confirm_kill = Some((p.pid, p.name.clone()));
            }
            ui.label(meta(th, "o tecla Supr"));
        });
    }
}
