//! Tabla de procesos de Inicio: filtro, columnas elegibles, orden y teclado.

use super::prelude::*;

/// Columnas posibles de la tabla: (clave en la config, título, orden, ancho).
const COLUMNS: &[(&str, &str, SortBy, f32)] = &[
    ("pid", "PID", SortBy::Pid, w::PID),
    ("name", "Proceso", SortBy::Name, w::NAME),
    ("user", "Usuario", SortBy::User, w::USER),
    ("cpu", "CPU", SortBy::Cpu, w::METER),
    ("mem", "Memoria", SortBy::Mem, w::MEM),
    ("gpu", "GPU", SortBy::Gpu, w::GPU),
    ("vram", "VRAM", SortBy::Vram, w::VRAM),
    ("disk", "Disco", SortBy::Disk, w::RATE),
    ("state", "Estado", SortBy::State, w::STATE),
    ("time", "Tiempo activo", SortBy::Time, w::TIME),
];

type ColDef = (&'static str, &'static str, SortBy, f32);

impl App {
    /// Teclado de la tabla: flechas/j/k, PgUp/PgDn, Inicio/Fin, Supr y "/".
    pub(super) fn keyboard(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if self.dialogs.confirm_kill.is_some() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let (down, up, pgdn, pgup, home, end, del, slash) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::J),
                i.key_pressed(Key::ArrowUp) || i.key_pressed(Key::K),
                i.key_pressed(Key::PageDown),
                i.key_pressed(Key::PageUp),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                i.key_pressed(Key::Delete),
                i.key_pressed(Key::Slash) || i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if t == "/")),
            )
        });
        if slash {
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("filter")));
            return;
        }
        if del {
            if let Some(p) = self.sel.pid.and_then(|pid| self.mon.proc(pid)) {
                self.dialogs.confirm_kill = Some((p.pid, p.name.clone()));
            }
            return;
        }
        let n = self.table.view.len();
        if n == 0 {
            return;
        }
        let cur = self.sel.pid.and_then(|pid| self.table.view.iter().position(|&i| self.mon.procs[i].pid == pid));
        let target = if down {
            Some(cur.map(|c| (c + 1).min(n - 1)).unwrap_or(0))
        } else if up {
            Some(cur.map(|c| c.saturating_sub(1)).unwrap_or(0))
        } else if pgdn {
            Some(cur.map(|c| (c + 15).min(n - 1)).unwrap_or(0))
        } else if pgup {
            Some(cur.map(|c| c.saturating_sub(15)).unwrap_or(0))
        } else if home {
            Some(0)
        } else if end {
            Some(n - 1)
        } else {
            None
        };
        if let Some(t) = target {
            let pid = self.mon.procs[self.table.view[t]].pid;
            self.select(Some(pid));
            self.table.scroll_to = Some(t);
        }
    }

    pub(super) fn toolbar(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        ui.horizontal(|ui| {
            ui.label(section_title(&th, "Procesos"));
            ui.label(meta(&th, format!("{}", self.table.view.len())));
            ui.add_space(12.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.table.filter)
                    .id(egui::Id::new("filter"))
                    .hint_text("Filtrar por nombre, usuario o PID   ( / )")
                    .desired_width(280.0),
            );
            if r.changed() {
                self.rebuild();
            }
            if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                self.table.filter.clear();
                self.rebuild();
                r.surrender_focus();
            }
            if !self.table.filter.is_empty() && ui.button("Limpiar").clicked() {
                self.table.filter.clear();
                self.rebuild();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button("Columnas", |ui| {
                    let mut changed = false;
                    for (key, title, _, _) in COLUMNS {
                        if *key == "name" {
                            continue;
                        }
                        let mut on = self.cfg.columns.iter().any(|c| c == key);
                        if ui.checkbox(&mut on, *title).changed() {
                            if on {
                                self.cfg.columns.push(key.to_string());
                            } else {
                                self.cfg.columns.retain(|c| c != key);
                            }
                            changed = true;
                        }
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.cfg.show_cmd, "Comando completo en vez del nombre").changed() {
                        changed = true;
                    }
                    if ui.button("Volver a las de siempre").clicked() {
                        self.cfg.columns = DEFAULT_COLUMNS.iter().map(|s| s.to_string()).collect();
                        changed = true;
                    }
                    if changed {
                        self.save_cfg();
                    }
                });
                let label = if self.cfg.cpu_per_core { "CPU: % de un núcleo" } else { "CPU: % del total" };
                egui::ComboBox::from_id_salt("cpu_mode").selected_text(label).show_ui(ui, |ui| {
                    let mut v = self.cfg.cpu_per_core;
                    ui.selectable_value(&mut v, false, "% del total (todos suman 100%)");
                    ui.selectable_value(&mut v, true, "% de un núcleo (como htop)");
                    if v != self.cfg.cpu_per_core {
                        self.cfg.cpu_per_core = v;
                        self.save_cfg();
                    }
                });
            });
        });
    }

    pub(super) fn table(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let mut clicked_sort: Option<SortBy> = None;
        let mut clicked_row: Option<u32> = None;
        let mut kill_req: Option<(u32, String)> = None;
        let sort = self.table.sort;
        let reverse = self.table.reverse;
        let show_cmd = self.cfg.show_cmd;
        let mut_c = muted(&th);

        // columnas elegidas, en orden fijo; "Proceso" siempre
        let mut cols: Vec<&ColDef> =
            COLUMNS.iter().filter(|(k, ..)| *k == "name" || self.cfg.columns.iter().any(|c| c == k)).collect();
        // si no entran, se sacan las de menor prioridad
        let spacing = 10.0;
        let avail = ui.available_width() - 12.0;
        let width = |cols: &Vec<&ColDef>| cols.iter().map(|c| c.3 + spacing).sum::<f32>();
        for d in ["time", "state", "user", "vram", "pid", "disk", "gpu"] {
            if width(&cols) <= avail {
                break;
            }
            cols.retain(|c| c.0 != d);
        }

        let hdr = |ui: &mut Ui, title: &str, s: SortBy, right: bool, out: &mut Option<SortBy>| {
            let active = s == sort;
            let col = if active { c(th.text) } else { mut_c };
            let layout = if right { Layout::right_to_left(Align::Center) } else { Layout::left_to_right(Align::Center) };
            ui.with_layout(layout, |ui| {
                let r = ui.add(egui::Label::new(RichText::new(title).size(13.0).color(col).strong()).sense(Sense::click()));
                if active {
                    // triángulo dibujado: hacia abajo = de mayor a menor
                    let center =
                        if right { r.rect.left_center() - Vec2::new(10.0, 0.0) } else { r.rect.right_center() + Vec2::new(10.0, 0.0) };
                    let pts = if !reverse {
                        vec![center + Vec2::new(-4.0, -2.5), center + Vec2::new(4.0, -2.5), center + Vec2::new(0.0, 3.0)]
                    } else {
                        vec![center + Vec2::new(-4.0, 2.5), center + Vec2::new(4.0, 2.5), center + Vec2::new(0.0, -3.0)]
                    };
                    ui.painter().add(Shape::convex_polygon(pts, c(th.accent), Stroke::NONE));
                }
                if r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Ordenar por esta columna").clicked() {
                    *out = Some(s);
                }
            });
        };

        let mut tb = TableBuilder::new(ui)
            .striped(false)
            .sense(Sense::click())
            .auto_shrink([false, false])
            .cell_layout(Layout::left_to_right(Align::Center));
        if let Some(r) = self.table.scroll_to.take() {
            tb = tb.scroll_to_row(r, None);
        }
        for col in &cols {
            tb = tb.column(if col.0 == "name" {
                Column::remainder().at_least(col.3).clip(true)
            } else {
                Column::exact(col.3).clip(true)
            });
        }
        let ncores = self.mon.stats.cores.len().max(1) as f32;
        let per_core = self.cfg.cpu_per_core;
        tb.header(28.0, |mut h| {
            for col in &cols {
                let right = !matches!(col.0, "name" | "user" | "state");
                h.col(|ui| hdr(ui, col.1, col.2, right, &mut clicked_sort));
            }
        })
        .body(|tbody| {
            tbody.rows(26.0, self.table.view.len(), |mut row| {
                let p = &self.mon.procs[self.table.view[row.index()]];
                row.set_selected(self.sel.pid == Some(p.pid));
                let cpu = if per_core { p.cpu } else { p.cpu / ncores };
                let right = |ui: &mut Ui, t: RichText| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(t);
                    });
                };
                // número + medidor (el medidor solo si hay algo que mostrar)
                let meter_cell = |ui: &mut Ui, v: f32, scale: f32, g: Grad, t: String| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(num(t, if v < 0.05 { mut_c } else { c(th.text) }));
                        let (r, _) = ui.allocate_exact_size(Vec2::new(36.0, 6.0), Sense::hover());
                        if v >= 0.5 {
                            paint_meter(ui.painter(), &th, r, (v / scale * 100.0).min(100.0), g);
                        }
                    });
                };
                for col in &cols {
                    row.col(|ui| match col.0 {
                        "pid" => right(ui, num(p.pid.to_string(), mut_c)),
                        "name" => {
                            if show_cmd && !p.cmd.is_empty() {
                                ui.label(body(&th, p.cmd.clone()));
                            } else {
                                // nombre y, al lado y apagados, los argumentos: distinguen
                                // los 12 "chrome" entre sí
                                ui.label(body(&th, p.name.clone()));
                                let args = p.cmd.split_once(' ').map(|(_, a)| a.trim()).unwrap_or("");
                                if !args.is_empty() {
                                    ui.add(egui::Label::new(RichText::new(args).size(12.5).color(mut_c)).truncate());
                                }
                            }
                        }
                        "user" => {
                            ui.label(RichText::new(&p.user).size(13.0).color(if p.user == "root" { c(th.warn) } else { mut_c }));
                        }
                        "cpu" => meter_cell(ui, cpu, if per_core { 100.0 } else { 25.0 }, Grad::Heat, format!("{cpu:.1}%")),
                        "mem" => meter_cell(ui, p.mem_pct, 25.0, Grad::Mono(th.mem), human(p.mem)),
                        "gpu" => match p.gpu {
                            Some(g) if g >= 0.05 => right(ui, num(format!("{g:.1}%"), Grad::Heat.at(&th, g / 100.0))),
                            _ => right(ui, num("", mut_c)),
                        },
                        "vram" => right(ui, num(if p.vram > 0 { human(p.vram) } else { String::new() }, c(th.mem))),
                        "disk" => {
                            let io = p.disk_read + p.disk_write;
                            right(ui, num(if io > 0.0 { format!("{}/s", human(io as u64)) } else { String::new() }, c(th.accent)))
                        }
                        "state" => {
                            let col = match p.state {
                                State::Run => c(th.ok),
                                State::Stop | State::Disk => c(th.warn),
                                State::Zombie => c(th.hot),
                                _ => mut_c,
                            };
                            ui.label(RichText::new(state_name(p.state)).size(13.0).color(col));
                        }
                        "time" => right(ui, num(hms(p.time), mut_c)),
                        _ => {}
                    });
                }
                let r = row.response();
                if r.clicked() {
                    clicked_row = Some(p.pid);
                }
                let (pid, name) = (p.pid, p.name.clone());
                r.context_menu(|ui| {
                    if ui.button("Ver detalle").clicked() {
                        clicked_row = Some(pid);
                        ui.close();
                    }
                    if ui.button("Terminar proceso…").clicked() {
                        kill_req = Some((pid, name.clone()));
                        ui.close();
                    }
                });
            });
        });

        if self.table.view.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(meta(&th, format!("Ningún proceso coincide con \"{}\".", self.table.filter)).size(T_BODY));
            });
        }
        if let Some(s) = clicked_sort {
            self.set_sort(s);
        }
        if let Some(pid) = clicked_row {
            self.select(if self.sel.pid == Some(pid) { None } else { Some(pid) });
        }
        if kill_req.is_some() {
            self.dialogs.confirm_kill = kill_req;
        }
    }

}
