//! Tabla de procesos de las páginas, con el mismo formato que la de Inicio.

use super::prelude::*;

/// Columnas extra de las tablas de procesos de las páginas.
pub enum Col {
    /// número alineado a la derecha
    Text(String, Color32),
    /// número con medidor
    Meter(f32, Grad, String),
    /// texto libre, a la izquierda y apagado
    Note(String),
}

impl App {
    /// PID, nombre con sus argumentos, usuario y las columnas propias de cada
    /// página (`heads`: título y ancho). Click = verlo en Inicio, click
    /// derecho = terminar.
    pub(crate) fn proc_table(&self, ui: &mut Ui, id: &str, heads: &[(&str, f32)], rows: Vec<(u32, String, Vec<Col>)>) {
        let th = &self.th;
        let mut_c = muted(th);
        if rows.is_empty() {
            ui.label(meta(th, "Ninguno por ahora."));
            return;
        }
        // los títulos se alinean como su contenido
        let left: Vec<bool> = rows[0].2.iter().map(|c| matches!(c, Col::Note(_))).collect();
        // si no entra, primero se va el usuario y después el PID
        let extra: f32 = heads.iter().map(|h| h.1 + 10.0).sum();
        let avail = ui.available_width();
        let show_user = avail - extra > w::NAME + w::PID + w::USER + 30.0;
        let show_pid = avail - extra > w::NAME + w::PID + 20.0;
        let mut tb = egui_extras::TableBuilder::new(ui)
            .id_salt(id)
            .striped(false)
            .vscroll(false)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center));
        if show_pid {
            tb = tb.column(egui_extras::Column::exact(w::PID));
        }
        tb = tb.column(egui_extras::Column::remainder().at_least(w::NAME).clip(true));
        if show_user {
            tb = tb.column(egui_extras::Column::exact(w::USER).clip(true));
        }
        for h in heads {
            tb = tb.column(egui_extras::Column::exact(h.1).clip(true));
        }
        let head = |ui: &mut Ui, t: &str, right: bool| {
            let l = if right { Layout::right_to_left(Align::Center) } else { Layout::left_to_right(Align::Center) };
            ui.with_layout(l, |ui| ui.label(meta(th, t)));
        };
        let right = |ui: &mut Ui, t: egui::RichText| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(t));
        };
        tb.header(24.0, |mut h| {
            if show_pid {
                h.col(|ui| head(ui, "PID", true));
            }
            h.col(|ui| head(ui, "Proceso", false));
            if show_user {
                h.col(|ui| head(ui, "Usuario", false));
            }
            for (k, (t, _)) in heads.iter().enumerate() {
                h.col(|ui| head(ui, t, !left.get(k).copied().unwrap_or(false)));
            }
        })
        .body(|mut tbody| {
            for (pid, name, cols) in rows {
                let proc = self.mon.proc(pid);
                tbody.row(26.0, |mut row| {
                    if show_pid {
                        row.col(|ui| right(ui, num(pid.to_string(), mut_c)));
                    }
                    row.col(|ui| {
                        ui.label(body(th, &name));
                        let args = proc.and_then(|p| p.cmd.split_once(' ')).map(|(_, a)| a.trim()).unwrap_or("");
                        if !args.is_empty() {
                            ui.add(egui::Label::new(egui::RichText::new(args).size(12.5).color(mut_c)).truncate());
                        }
                    });
                    if show_user {
                        row.col(|ui| {
                            let u = proc.map(|p| p.user.as_str()).unwrap_or("");
                            ui.label(egui::RichText::new(u).size(13.0).color(if u == "root" { c(th.warn) } else { mut_c }));
                        });
                    }
                    for col in cols {
                        row.col(|ui| match col {
                            Col::Text(t, color) => right(ui, num(t, color)),
                            Col::Note(t) => {
                                ui.add(egui::Label::new(egui::RichText::new(t).size(13.0).color(mut_c)).truncate());
                            }
                            Col::Meter(v, g, t) => {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(num(t, if v < 0.05 { mut_c } else { c(th.text) }));
                                    let (r, _) = ui.allocate_exact_size(Vec2::new(36.0, 6.0), Sense::hover());
                                    if v >= 0.5 {
                                        crate::kit::paint_meter(ui.painter(), th, r, v.min(100.0), g);
                                    }
                                });
                            }
                        });
                    }
                    let r = row.response().on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Click para verlo en Inicio");
                    if r.clicked() {
                        self.act(Action::ShowProc(pid));
                    }
                    r.context_menu(|ui| {
                        if ui.button("Ver en Inicio").clicked() {
                            self.act(Action::ShowProc(pid));
                            ui.close();
                        }
                        if ui.button("Terminar proceso…").clicked() {
                            self.act(Action::AskKill(pid, name.clone()));
                            ui.close();
                        }
                    });
                });
            }
        });
    }

    /// Columnas comunes a todas las tablas: CPU, memoria y tiempo activo.
    pub(super) fn base_cols(&self, p: &Proc) -> (Col, Col, Col) {
        let th = &self.th;
        let v = self.cpu_of(p);
        let scale = if self.cfg.cpu_per_core { 1.0 } else { 4.0 };
        (
            Col::Meter((v * scale).min(100.0), Grad::Heat, format!("{v:.1}%")),
            Col::Meter(p.mem_pct * 4.0, Grad::Mono(th.mem), human(p.mem)),
            Col::Text(rtop_core::fmt::hms(p.time), muted(th)),
        )
    }

    pub(super) fn cpu_rows(&self, procs: &[&Proc]) -> Vec<(u32, String, Vec<Col>)> {
        let th = &self.th;
        procs
            .iter()
            .map(|p| {
                let (cpu, mem, time) = self.base_cols(p);
                let gpu = match p.gpu {
                    Some(g) if g >= 0.05 => Col::Text(format!("{g:.1}%"), Grad::Heat.at(th, g / 100.0)),
                    _ => Col::Text(String::new(), muted(th)),
                };
                let state = Col::Note(crate::home::state_name(p.state).to_string());
                (p.pid, p.name.clone(), vec![cpu, mem, gpu, state, time])
            })
            .collect()
    }

    pub(super) fn mem_rows(&self, procs: &[&Proc]) -> Vec<(u32, String, Vec<Col>)> {
        let th = &self.th;
        procs
            .iter()
            .map(|p| {
                let (cpu, mem, time) = self.base_cols(p);
                let vram = Col::Text(if p.vram > 0 { human(p.vram) } else { String::new() }, c(th.mem));
                (p.pid, p.name.clone(), vec![mem, Col::Text(format!("{:.1}%", p.mem_pct), muted(th)), vram, cpu, time])
            })
            .collect()
    }
}
