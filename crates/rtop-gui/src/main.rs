use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, RichText, Sense, Stroke, Ui, Vec2,
};
use egui_extras::{Column, TableBuilder};
use egui_plot::{Line, Plot, PlotPoints};
use rtop_core::{
    config::THEMES,
    fmt::{hms, human},
    Config, Monitor, Rgb, SortBy, State, Theme,
};

fn c(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

struct App {
    mon: Monitor,
    cfg: Config,
    th: Theme,
    view: Vec<usize>,
    sort: SortBy,
    reverse: bool,
    filter: String,
    selected: Option<u32>,
    confirm_kill: Option<(u32, String)>,
    show_settings: bool,
    paused: bool,
    last: Instant,
    flash: Option<(String, Instant)>,
    visuals_for: Option<Theme>,
}

impl App {
    fn new() -> Self {
        let cfg = Config::load();
        let mut a = App {
            mon: Monitor::new(),
            th: cfg.theme(),
            cfg,
            view: vec![],
            sort: SortBy::Cpu,
            reverse: false,
            filter: String::new(),
            selected: None,
            confirm_kill: None,
            show_settings: false,
            paused: false,
            last: Instant::now(),
            flash: None,
            visuals_for: None,
        };
        a.rebuild();
        a
    }

    fn rebuild(&mut self) {
        self.view = self.mon.view(&self.filter, self.sort, self.reverse);
    }

    fn msg(&mut self, m: impl Into<String>) {
        self.flash = Some((m.into(), Instant::now()));
    }

    fn apply_visuals(&mut self, ctx: &egui::Context) {
        if self.visuals_for.as_ref() == Some(&self.th) {
            return;
        }
        let th = &self.th;
        let mut v = egui::Visuals::dark();
        v.panel_fill = c(th.bg);
        v.window_fill = c(th.panel);
        v.extreme_bg_color = c(th.panel);
        v.faint_bg_color = c(th.panel);
        v.override_text_color = Some(c(th.text));
        v.selection.bg_fill = c(th.sel);
        v.selection.stroke = Stroke::new(1.0, c(th.accent));
        v.hyperlink_color = c(th.accent);
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, c(th.sel));
        v.widgets.inactive.bg_fill = c(th.sel);
        v.widgets.inactive.weak_bg_fill = c(th.sel);
        v.widgets.hovered.bg_fill = c(th.dim);
        v.widgets.hovered.weak_bg_fill = c(th.dim);
        v.widgets.active.bg_fill = c(th.accent);
        v.widgets.active.weak_bg_fill = c(th.accent);
        ctx.set_visuals(v);
        self.visuals_for = Some(self.th.clone());
    }

    fn save_cfg(&mut self) {
        self.th = self.cfg.theme();
        if let Err(e) = self.cfg.save() {
            self.msg(format!("no pude guardar la config: {e}"));
        }
    }

    fn set_sort(&mut self, s: SortBy) {
        if self.sort == s {
            self.reverse = !self.reverse;
        } else {
            self.sort = s;
            self.reverse = false;
        }
        self.rebuild();
    }

    // ---------- secciones ----------

    fn card<R>(&self, ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
        egui::Frame::new()
            .fill(c(self.th.panel))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(10.0)
            .show(ui, |ui| {
                ui.label(RichText::new(title).color(c(self.th.accent)).strong());
                ui.add_space(4.0);
                add(ui)
            })
            .inner
    }

    fn bar(&self, ui: &mut Ui, label: &str, pct: f32, text: &str, width: f32) {
        let th = &self.th;
        let h = 16.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
        let p = ui.painter();
        let lw = 34.0;
        p.text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            FontId::monospace(12.0),
            c(th.accent),
        );
        let track = egui::Rect::from_min_max(rect.min + Vec2::new(lw, 2.0), rect.max - Vec2::new(0.0, 2.0));
        p.rect_filled(track, CornerRadius::same(4), c(th.sel));
        let mut fill = track;
        fill.set_width(track.width() * (pct.clamp(0.0, 100.0) / 100.0));
        p.rect_filled(fill, CornerRadius::same(4), c(th.heat(pct)));
        p.text(
            track.right_center() - Vec2::new(6.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            text,
            FontId::monospace(11.0),
            c(th.text),
        );
    }

    fn header(&self, ui: &mut Ui) {
        let s = &self.mon.stats;
        let total_w = ui.available_width();
        ui.horizontal_top(|ui| {
            let cpu_w = (total_w * 0.6).max(300.0);
            ui.allocate_ui(Vec2::new(cpu_w, 0.0), |ui| {
                self.card(ui, &format!("CPU  {}", s.cpu_brand), |ui| {
                    let cols = if s.cores.len() > 16 { 4 } else { 2 };
                    let col_w = (ui.available_width() - 12.0 * (cols as f32 - 1.0)) / cols as f32;
                    let per_col = s.cores.len().div_ceil(cols).max(1);
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        for ci in 0..cols {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 3.0;
                                for (i, &u) in s.cores.iter().enumerate().skip(ci * per_col).take(per_col) {
                                    self.bar(ui, &format!("{i:>2}"), u, &format!("{u:.1}%"), col_w);
                                }
                            });
                        }
                    });
                });
            });
            ui.vertical(|ui| {
                self.card(ui, &s.host, |ui| {
                    let w = ui.available_width();
                    ui.spacing_mut().item_spacing.y = 4.0;
                    self.bar(ui, "Mem", s.mem_pct(), &format!("{}/{}", human(s.mem_used), human(s.mem_total)), w);
                    self.bar(ui, "Swp", s.swap_pct(), &format!("{}/{}", human(s.swap_used), human(s.swap_total)), w);
                    ui.add_space(4.0);
                    let th = &self.th;
                    let load_pct = s.load.0 as f32 * 100.0 / s.cores.len().max(1) as f32;
                    egui::Grid::new("info").num_columns(2).spacing([14.0, 3.0]).show(ui, |ui| {
                        let k = |ui: &mut Ui, t: &str| {
                            ui.label(RichText::new(t).color(c(th.accent)).strong());
                        };
                        k(ui, "Carga");
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("{:.2}", s.load.0)).color(c(th.heat(load_pct))).strong());
                            ui.label(format!("{:.2} {:.2}", s.load.1, s.load.2));
                        });
                        ui.end_row();
                        k(ui, "Tareas");
                        ui.horizontal(|ui| {
                            ui.label(format!("{}", s.tasks));
                            ui.label(RichText::new(format!("{} corriendo", s.running)).color(c(th.ok)));
                        });
                        ui.end_row();
                        k(ui, "Uptime");
                        ui.label(hms(s.uptime));
                        ui.end_row();
                        k(ui, "Red");
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("v {}/s", human(s.rx_rate as u64))).color(c(th.ok)));
                            ui.label(RichText::new(format!("^ {}/s", human(s.tx_rate as u64))).color(c(th.warn)));
                        });
                        ui.end_row();
                        if let Some(t) = s.temp_max {
                            k(ui, "Temp");
                            ui.label(RichText::new(format!("{t:.0}°C max")).color(c(th.heat(t))).strong());
                            ui.end_row();
                        }
                    });
                });
            });
        });
    }

    fn graphs(&self, ui: &mut Ui) {
        let th = &self.th;
        let w = (ui.available_width() - 8.0) / 2.0;
        ui.horizontal(|ui| {
            for (id, hist, title, color) in [
                ("cpu", &self.mon.cpu_hist, "CPU total", th.accent),
                ("mem", &self.mon.mem_hist, "Memoria", th.mem),
            ] {
                ui.allocate_ui(Vec2::new(w, 120.0), |ui| {
                    let last = hist.back().copied().unwrap_or(0.0);
                    self.card(ui, &format!("{title}  {last:.0}%"), |ui| {
                        let n = hist.len();
                        let pts: PlotPoints = hist
                            .iter()
                            .enumerate()
                            .map(|(i, &v)| [i as f64 - n as f64, v as f64])
                            .collect::<Vec<_>>()
                            .into();
                        Plot::new(id)
                            .height(80.0)
                            .include_y(0.0)
                            .include_y(100.0)
                            .include_x(-(rtop_core::HIST as f64))
                            .include_x(0.0)
                            .show_axes([false, true])
                            .show_background(false)
                            .allow_drag(false)
                            .allow_zoom(false)
                            .allow_scroll(false)
                            .allow_boxed_zoom(false)
                            .allow_double_click_reset(false)
                            .show(ui, |p| {
                                p.line(Line::new(title, pts).color(c(color)).fill(0.0).width(1.5));
                            });
                    });
                });
            }
        });
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Filtrar").color(c(self.th.accent)).strong());
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("nombre, comando, usuario o pid")
                    .desired_width(260.0),
            );
            if r.changed() {
                self.rebuild();
            }
            if !self.filter.is_empty() && ui.button("x").clicked() {
                self.filter.clear();
                self.rebuild();
            }
            ui.separator();
            ui.label(format!("{} procesos", self.view.len()));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Ajustes").clicked() {
                    self.show_settings = !self.show_settings;
                }
                let lbl = if self.paused { "Reanudar" } else { "Pausar" };
                if ui.button(lbl).clicked() {
                    self.paused = !self.paused;
                }
                let can_kill = self.selected.is_some();
                if ui.add_enabled(can_kill, egui::Button::new("Matar proceso")).clicked() {
                    if let Some(pid) = self.selected {
                        let name = self
                            .mon
                            .procs
                            .iter()
                            .find(|p| p.pid == pid)
                            .map(|p| p.name.clone())
                            .unwrap_or_default();
                        self.confirm_kill = Some((pid, name));
                    }
                }
                if let Some((m, t)) = &self.flash {
                    if t.elapsed() < Duration::from_secs(4) {
                        ui.label(RichText::new(m).color(c(self.th.warn)));
                    }
                }
            });
        });
    }

    fn table(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let arrow = if self.reverse { " ^" } else { " v" };
        let mut clicked_sort: Option<SortBy> = None;
        let mut clicked_row: Option<u32> = None;
        let mut kill_req: Option<(u32, String)> = None;
        let sort = self.sort;
        let show_cmd = self.cfg.show_cmd;

        let hdr = |ui: &mut Ui, name: &str, s: Option<SortBy>, out: &mut Option<SortBy>| {
            let active = s == Some(sort);
            let txt = if active { format!("{name}{arrow}") } else { name.to_string() };
            let rt = RichText::new(txt).strong().color(if active { c(th.bg) } else { c(th.accent) });
            match s {
                Some(s) => {
                    let b = egui::Button::new(rt).fill(if active { c(th.accent) } else { Color32::TRANSPARENT });
                    if ui.add(b).clicked() {
                        *out = Some(s);
                    }
                }
                None => {
                    ui.label(rt);
                }
            }
        };

        TableBuilder::new(ui)
            .striped(true)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            .column(Column::exact(70.0))
            .column(Column::exact(100.0))
            .column(Column::exact(64.0))
            .column(Column::exact(64.0))
            .column(Column::exact(70.0))
            .column(Column::exact(24.0))
            .column(Column::exact(84.0))
            .column(Column::remainder().clip(true))
            .header(24.0, |mut h| {
                h.col(|ui| hdr(ui, "PID", Some(SortBy::Pid), &mut clicked_sort));
                h.col(|ui| hdr(ui, "USUARIO", None, &mut clicked_sort));
                h.col(|ui| hdr(ui, "CPU%", Some(SortBy::Cpu), &mut clicked_sort));
                h.col(|ui| hdr(ui, "MEM%", Some(SortBy::Mem), &mut clicked_sort));
                h.col(|ui| hdr(ui, "RES", None, &mut clicked_sort));
                h.col(|ui| hdr(ui, "S", None, &mut clicked_sort));
                h.col(|ui| hdr(ui, "TIEMPO", Some(SortBy::Time), &mut clicked_sort));
                h.col(|ui| {
                    hdr(ui, if show_cmd { "COMANDO" } else { "NOMBRE" }, Some(SortBy::Name), &mut clicked_sort)
                });
            })
            .body(|body| {
                body.rows(20.0, self.view.len(), |mut row| {
                    let p = &self.mon.procs[self.view[row.index()]];
                    row.set_selected(self.selected == Some(p.pid));
                    let mono = |t: String, col: Rgb| RichText::new(t).monospace().color(c(col));
                    let cpu_c = if p.cpu < 1.0 { th.dim } else { th.heat(p.cpu) };
                    let mem_c = if p.mem_pct < 1.0 { th.dim } else { th.heat(p.mem_pct * 4.0) };
                    let user_c = if p.user == "root" { th.hot } else { th.text };
                    let st_c = match p.state {
                        State::Run => th.ok,
                        State::Stop | State::Disk => th.warn,
                        State::Zombie => th.hot,
                        _ => th.dim,
                    };
                    let label = if show_cmd && !p.cmd.is_empty() { &p.cmd } else { &p.name };
                    row.col(|ui| { ui.label(mono(format!("{:>7}", p.pid), th.dim)); });
                    row.col(|ui| { ui.label(mono(p.user.clone(), user_c)); });
                    row.col(|ui| { ui.label(mono(format!("{:>5.1}", p.cpu), cpu_c)); });
                    row.col(|ui| { ui.label(mono(format!("{:>5.1}", p.mem_pct), mem_c)); });
                    row.col(|ui| { ui.label(mono(format!("{:>6}", human(p.mem)), th.text)); });
                    row.col(|ui| { ui.label(mono(p.state.letter().into(), st_c)); });
                    row.col(|ui| { ui.label(mono(hms(p.time), th.dim)); });
                    row.col(|ui| { ui.label(mono(label.clone(), th.text)); });
                    let r = row.response();
                    if r.clicked() {
                        clicked_row = Some(p.pid);
                    }
                    let (pid, name) = (p.pid, p.name.clone());
                    r.context_menu(|ui| {
                        if ui.button("Terminar (SIGTERM)").clicked() {
                            kill_req = Some((pid, name.clone()));
                            ui.close();
                        }
                    });
                });
            });

        if let Some(s) = clicked_sort {
            self.set_sort(s);
        }
        if let Some(pid) = clicked_row {
            self.selected = if self.selected == Some(pid) { None } else { Some(pid) };
        }
        if kill_req.is_some() {
            self.confirm_kill = kill_req;
        }
    }

    fn settings(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        let mut changed = false;
        egui::Window::new("Ajustes")
            .open(&mut open)
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                egui::Grid::new("cfg").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Tema");
                    egui::ComboBox::from_id_salt("theme")
                        .selected_text(self.cfg.theme.clone())
                        .show_ui(ui, |ui| {
                            for t in THEMES {
                                if ui.selectable_label(self.cfg.theme == t.name, t.name).clicked() {
                                    self.cfg.theme = t.name.to_string();
                                    changed = true;
                                }
                            }
                        });
                    ui.end_row();

                    ui.label("Refresco (ms)");
                    let mut ms = self.cfg.refresh_ms();
                    if ui.add(egui::Slider::new(&mut ms, 250..=5000).step_by(250.0)).changed() {
                        self.cfg.refresh_ms = ms;
                        changed = true;
                    }
                    ui.end_row();

                    ui.label("Gráficos");
                    changed |= ui.checkbox(&mut self.cfg.show_graphs, "mostrar").changed();
                    ui.end_row();

                    ui.label("Comando completo");
                    changed |= ui.checkbox(&mut self.cfg.show_cmd, "mostrar").changed();
                    ui.end_row();
                });

                ui.separator();
                ui.label(RichText::new("Colores personalizados").color(c(self.th.accent)).strong());
                ui.label(RichText::new("pisan los del tema; se guardan en la config").color(c(self.th.dim)).small());
                let th = self.th.clone();
                egui::Grid::new("colors").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
                    let ov = &mut self.cfg.colors;
                    for (name, cur, slot) in [
                        ("fondo", th.bg, &mut ov.bg),
                        ("paneles", th.panel, &mut ov.panel),
                        ("texto", th.text, &mut ov.text),
                        ("tenue", th.dim, &mut ov.dim),
                        ("acento", th.accent, &mut ov.accent),
                        ("ok", th.ok, &mut ov.ok),
                        ("alerta", th.warn, &mut ov.warn),
                        ("crítico", th.hot, &mut ov.hot),
                        ("selección", th.sel, &mut ov.sel),
                        ("memoria", th.mem, &mut ov.mem),
                    ] {
                        ui.label(name);
                        let mut rgb = [cur.0, cur.1, cur.2];
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            *slot = Some(Rgb(rgb[0], rgb[1], rgb[2]).hex());
                            changed = true;
                        }
                        if slot.is_some() {
                            if ui.small_button("reset").clicked() {
                                *slot = None;
                                changed = true;
                            }
                        } else {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Restablecer todo").clicked() {
                        self.cfg = Config::default();
                        changed = true;
                    }
                    if let Some(p) = Config::path() {
                        ui.label(RichText::new(p.display().to_string()).color(c(self.th.dim)).small());
                    }
                });
            });
        self.show_settings = open;
        if changed {
            self.save_cfg();
        }
    }

    fn confirm(&mut self, ctx: &egui::Context) {
        let Some((pid, name)) = self.confirm_kill.clone() else { return };
        let mut action: Option<Option<bool>> = None; // Some(None)=cancelar, Some(Some(force))
        let hot = c(self.th.hot);
        let resp = egui::Modal::new(egui::Id::new("kill")).show(ctx, |ui| {
            ui.set_width(320.0);
            ui.heading("Matar proceso");
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new(&name).color(hot).strong());
                ui.label(format!("(pid {pid})"));
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Terminar (TERM)").clicked() {
                    action = Some(Some(false));
                }
                if ui.button(RichText::new("Forzar (KILL)").color(hot)).clicked() {
                    action = Some(Some(true));
                }
                if ui.button("Cancelar").clicked() {
                    action = Some(None);
                }
            });
        });
        if resp.should_close() && action.is_none() {
            action = Some(None);
        }
        if let Some(a) = action {
            self.confirm_kill = None;
            if let Some(force) = a {
                let s = if force { "SIGKILL" } else { "SIGTERM" };
                if self.mon.kill(pid, force) {
                    self.msg(format!("{s} -> {name} ({pid})"));
                } else {
                    self.msg(format!("no pude matar {name} ({pid}) - permisos?"));
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_visuals(&ctx);

        let tick = Duration::from_millis(self.cfg.refresh_ms());
        if !self.paused && self.last.elapsed() >= tick {
            self.mon.refresh();
            self.rebuild();
            self.last = Instant::now();
        }
        ctx.request_repaint_after(tick.saturating_sub(self.last.elapsed()));

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.confirm_kill.is_none() {
            self.selected = None;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(c(self.th.bg)).inner_margin(10.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                self.header(ui);
                if self.cfg.show_graphs {
                    self.graphs(ui);
                }
                self.toolbar(ui);
                self.table(ui);
            });

        self.settings(&ctx);
        self.confirm(&ctx);
    }
}

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("rtop")
            .with_app_id("rtop")
            .with_inner_size([1200.0, 820.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native("rtop", opts, Box::new(|_cc| Ok(Box::new(App::new()))))
}
