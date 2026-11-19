//! Inicio: contesta "¿qué me está consumiendo la máquina?".
//! Arriba una franja de estado (CPU, RAM, GPUs); la tabla de procesos es lo
//! principal; el proceso elegido se detalla a la derecha.

use eframe::egui::{self, Align, Color32, CornerRadius, Key, Layout, RichText, Sense, Shape, Stroke, Ui, Vec2};
use egui_extras::{Column, TableBuilder};
use rtop_core::{
    config::DEFAULT_COLUMNS,
    fmt::{hms, human},
    Proc, SortBy, State,
};

use crate::{
    kit::{self, body, c, chart, kv, meta, muted, num, paint_meter, section_title, stat_row, strong, Grad, Series, Stat, T_BODY},
    pages::{pct_fmt, rate_fmt},
    App, Page,
};

pub fn state_name(s: State) -> &'static str {
    match s {
        State::Run => "corriendo",
        State::Sleep => "en espera",
        State::Stop => "detenido",
        State::Zombie => "zombie",
        State::Disk => "esperando al disco",
        State::Other => "desconocido",
    }
}

/// Columnas posibles de la tabla: (clave en la config, título, orden, ancho).
const COLUMNS: &[(&str, &str, SortBy, f32)] = &[
    ("pid", "PID", SortBy::Pid, 64.0),
    ("name", "Proceso", SortBy::Name, 160.0),
    ("user", "Usuario", SortBy::User, 96.0),
    ("cpu", "CPU", SortBy::Cpu, 116.0),
    ("mem", "Memoria", SortBy::Mem, 124.0),
    ("gpu", "GPU", SortBy::Gpu, 70.0),
    ("vram", "VRAM", SortBy::Vram, 76.0),
    ("disk", "Disco", SortBy::Disk, 88.0),
    ("state", "Estado", SortBy::State, 112.0),
    ("time", "Tiempo activo", SortBy::Time, 104.0),
];

type ColDef = (&'static str, &'static str, SortBy, f32);

impl App {
    /// CPU de un proceso según el criterio elegido (total de la máquina o un núcleo).
    pub(crate) fn cpu_of(&self, p: &Proc) -> f32 {
        if self.cfg.cpu_per_core {
            p.cpu
        } else {
            p.cpu / self.mon.stats.cores.len().max(1) as f32
        }
    }

    pub(crate) fn cpu_hint(&self) -> String {
        if self.cfg.cpu_per_core {
            format!("100% = un núcleo entero (tenés {})", self.mon.stats.cores.len())
        } else {
            "100% = todo el procesador".into()
        }
    }

    pub(crate) fn page_home(&mut self, ui: &mut Ui) {
        self.home_strip(ui);
        ui.add_space(4.0);
        self.keyboard(ui);

        let th = self.th.clone();
        let total_w = ui.available_width();
        let h = ui.available_height();
        let gap = 12.0;
        let detail_w = if self.selected.is_some() { (total_w * 0.3).clamp(280.0, 380.0) } else { 0.0 };
        let table_w = total_w - detail_w - if detail_w > 0.0 { gap } else { 0.0 };
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(Vec2::new(table_w, h), Layout::top_down(Align::Min), |ui| {
                egui::Frame::new()
                    .fill(c(th.panel))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(table_w - 32.0);
                        ui.set_height(h - 24.0);
                        self.toolbar(ui);
                        ui.add_space(6.0);
                        self.table(ui);
                    });
            });
            if detail_w > 0.0 {
                ui.allocate_ui_with_layout(Vec2::new(detail_w, h), Layout::top_down(Align::Min), |ui| {
                    egui::Frame::new()
                        .fill(kit::layer2(&th))
                        .stroke(Stroke::new(1.0, c(th.sel)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(16, 14))
                        .show(ui, |ui| {
                            ui.set_width(detail_w - 32.0);
                            ui.set_height(h - 28.0);
                            egui::ScrollArea::vertical().id_salt("detail").auto_shrink([false, false]).show(ui, |ui| {
                                self.detail(ui);
                            });
                        });
                });
            }
        });
    }

    /// Franja de estado: un valor por recurso, sin gráficos. Click = detalle.
    fn home_strip(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let s = &self.mon.stats;
        let heat = |v: f32| Grad::Heat.at(&th, v / 100.0);
        let mut stats = vec![];
        let mut pages = vec![];

        let mut cpu_d = format!("{}   {:.1} GHz", s.cpu_model, s.cpu_freq as f32 / 1000.0);
        if let Some(t) = s.cpu_temp {
            cpu_d.push_str(&format!("   {t:.0} °C"));
        }
        stats.push(
            Stat::new("CPU", format!("{:.0}%", s.cpu_total), heat(s.cpu_total))
                .detail(cpu_d)
                .meter(s.cpu_total, Grad::Heat)
                .link("Ver detalle del procesador (tecla 2)"),
        );
        pages.push(Page::Cpu);

        stats.push(
            Stat::new("Memoria", format!("{:.0}%", s.mem_pct()), heat(s.mem_pct()))
                .detail(format!("{} de {} en uso", human(s.mem_used), human(s.mem_total)))
                .meter(s.mem_pct(), Grad::Heat)
                .link(format!("Ver detalle de la memoria (tecla {})", s.gpus.len() + 3)),
        );
        pages.push(Page::Mem);

        for (i, g) in s.gpus.iter().enumerate() {
            let label = if g.integrated { format!("{} (integrada)", g.short_name()) } else { g.short_name() };
            let stat = if g.asleep {
                Stat::new(label, "Dormida", c(th.ok)).detail("ahorrando energía, sin uso")
            } else {
                let v = g.busy.unwrap_or(0.0);
                let mut d = format!("VRAM {} de {}", human(g.vram_used), human(g.vram_total));
                if let Some(t) = g.temp {
                    d.push_str(&format!("   {t:.0} °C"));
                }
                if !g.integrated {
                    d.push_str("   despierta");
                }
                Stat::new(label, format!("{v:.0}%"), heat(v)).detail(d).meter(v, Grad::Heat)
            };
            stats.push(stat.link(format!("Ver detalle de {} (tecla {})", g.short_name(), i + 3)));
            pages.push(Page::Gpu(i));
        }

        if let Some(i) = stat_row(ui, &th, "home_strip", &stats) {
            self.go(pages[i]);
        }
    }

    /// Teclado de la tabla: flechas/j/k, PgUp/PgDn, Inicio/Fin, Supr y "/".
    fn keyboard(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if self.confirm_kill.is_some() || ctx.memory(|m| m.focused().is_some()) {
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
            if let Some(p) = self.selected.and_then(|pid| self.mon.proc(pid)) {
                self.confirm_kill = Some((p.pid, p.name.clone()));
            }
            return;
        }
        let n = self.view.len();
        if n == 0 {
            return;
        }
        let cur = self.selected.and_then(|pid| self.view.iter().position(|&i| self.mon.procs[i].pid == pid));
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
            let pid = self.mon.procs[self.view[t]].pid;
            self.select(Some(pid));
            self.scroll_to = Some(t);
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        ui.horizontal(|ui| {
            ui.label(section_title(&th, "Procesos"));
            ui.label(meta(&th, format!("{}", self.view.len())));
            ui.add_space(12.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .id(egui::Id::new("filter"))
                    .hint_text("Filtrar por nombre, usuario o PID   ( / )")
                    .desired_width(280.0),
            );
            if r.changed() {
                self.rebuild();
            }
            if r.has_focus() && ui.input(|i| i.key_pressed(Key::Escape)) {
                self.filter.clear();
                self.rebuild();
                r.surrender_focus();
            }
            if !self.filter.is_empty() && ui.button("Limpiar").clicked() {
                self.filter.clear();
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

    fn table(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let mut clicked_sort: Option<SortBy> = None;
        let mut clicked_row: Option<u32> = None;
        let mut kill_req: Option<(u32, String)> = None;
        let sort = self.sort;
        let reverse = self.reverse;
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
        if let Some(r) = self.scroll_to.take() {
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
            tbody.rows(26.0, self.view.len(), |mut row| {
                let p = &self.mon.procs[self.view[row.index()]];
                row.set_selected(self.selected == Some(p.pid));
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

        if self.view.is_empty() {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(meta(&th, format!("Ningún proceso coincide con \"{}\".", self.filter)).size(T_BODY));
            });
        }
        if let Some(s) = clicked_sort {
            self.set_sort(s);
        }
        if let Some(pid) = clicked_row {
            self.select(if self.selected == Some(pid) { None } else { Some(pid) });
        }
        if kill_req.is_some() {
            self.confirm_kill = kill_req;
        }
    }

    /// Todo lo que se sabe del proceso elegido. Datos primero, acción al final.
    fn detail(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let th = &th;
        let Some(pid) = self.selected else { return };
        let Some(p) = self.mon.proc(pid).cloned() else {
            ui.label(meta(th, "El proceso terminó."));
            return;
        };
        let d = self.details.as_ref().filter(|(x, _)| *x == pid).map(|(_, d)| d.clone()).unwrap_or_default();
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
        let cpu_data: Vec<f32> = self.sel_cpu.iter().map(|v| v / div).collect();
        ui.label(meta(th, format!("CPU desde que lo elegiste ({})", self.cpu_hint())));
        chart(ui, th, &self.opts(70.0, Some(100.0), pct_fmt), &[Series::new("CPU", cpu_data, Grad::Heat)], None);
        let mem_data: Vec<f32> = self.sel_mem.iter().copied().collect();
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
                self.confirm_kill = Some((p.pid, p.name.clone()));
            }
            ui.label(meta(th, "o tecla Supr"));
        });
    }
}
