mod kit;
mod pages;

use std::{
    cell::{Cell, RefCell},
    time::{Duration, Instant},
};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, Vec2,
};
use egui_extras::{Column, TableBuilder};
use kit::{c, caption, paint_meter, paint_spark, tail_f32, tail_f64, Grad};
use rtop_core::{
    config::THEMES,
    fmt::{hms, human},
    Config, Monitor, Rgb, SortBy, State, Theme,
};

/// muestras visibles en los gráficos
pub const WINDOW: usize = 150;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Overview,
    Cpu,
    Mem,
    Gpu(usize),
    Net,
    Disk,
    Sensors,
    Procs,
}

struct App {
    mon: Monitor,
    cfg: Config,
    th: Theme,
    page: Page,
    view: Vec<usize>,
    sort: SortBy,
    reverse: bool,
    filter: String,
    selected: Option<u32>,
    confirm_kill: Option<(u32, String)>,
    /// pedidos desde páginas que no tienen `&mut self`
    kill_req: RefCell<Option<(u32, String)>>,
    goto: Cell<Option<Page>>,
    show_settings: bool,
    paused: bool,
    last: Instant,
    flash: Option<(String, Instant)>,
    visuals_for: Option<Theme>,
    started: Instant,
    shot_requested: bool,
}

impl App {
    fn new() -> Self {
        let cfg = Config::load();
        let page = match std::env::var("RTOP_PAGE").as_deref() {
            Ok("cpu") => Page::Cpu,
            Ok("mem") => Page::Mem,
            Ok("gpu0") => Page::Gpu(0),
            Ok("gpu1") => Page::Gpu(1),
            Ok("net") => Page::Net,
            Ok("disk") => Page::Disk,
            Ok("sensors") => Page::Sensors,
            Ok("procs") => Page::Procs,
            _ => Page::Overview,
        };
        let mut a = App {
            mon: Monitor::new(),
            th: cfg.theme(),
            cfg,
            page,
            view: vec![],
            sort: SortBy::Cpu,
            reverse: false,
            filter: String::new(),
            selected: None,
            confirm_kill: None,
            kill_req: RefCell::new(None),
            goto: Cell::new(None),
            show_settings: false,
            paused: false,
            last: Instant::now(),
            flash: None,
            visuals_for: None,
            started: Instant::now(),
            shot_requested: false,
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
        v.extreme_bg_color = c(th.bg);
        v.faint_bg_color = kit_blend(th.panel, th.sel);
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

    // ---------- barra lateral ----------

    fn nav(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let s = &self.mon.stats;
        let m = &self.mon;
        struct Item {
            page: Page,
            title: String,
            value: String,
            pct: Option<f32>,
            spark: Vec<f32>,
            spark_max: Option<f32>,
            grad: Grad,
        }
        let mut items = vec![
            Item {
                page: Page::Overview,
                title: "Resumen".into(),
                value: s.host.clone(),
                pct: None,
                spark: vec![],
                spark_max: None,
                grad: Grad::Heat,
            },
            Item {
                page: Page::Cpu,
                title: "CPU".into(),
                value: format!("{:.0}%", s.cpu_total),
                pct: Some(s.cpu_total),
                spark: tail_f32(&m.cpu_hist, 60),
                spark_max: Some(100.0),
                grad: Grad::Heat,
            },
            Item {
                page: Page::Mem,
                title: "Memoria".into(),
                value: format!("{} / {}", human(s.mem_used), human(s.mem_total)),
                pct: Some(s.mem_pct()),
                spark: tail_f32(&m.mem_hist, 60),
                spark_max: Some(100.0),
                grad: Grad::Mono(th.mem),
            },
        ];
        for (i, g) in s.gpus.iter().enumerate() {
            items.push(Item {
                page: Page::Gpu(i),
                title: format!("GPU {} · {}", i + 1, g.short_name()),
                value: if g.asleep { "dormida".into() } else { format!("{:.0}%", g.busy.unwrap_or(0.0)) },
                pct: if g.asleep { None } else { g.busy },
                spark: m.gpu_hist.get(i).map(|h| tail_f32(h, 60)).unwrap_or_default(),
                spark_max: Some(100.0),
                grad: Grad::Heat,
            });
        }
        items.push(Item {
            page: Page::Net,
            title: "Red".into(),
            value: format!("v {}/s", human(s.rx_rate as u64)),
            pct: None,
            spark: tail_f64(&m.rx_hist, 60),
            spark_max: None,
            grad: Grad::Mono(th.ok),
        });
        items.push(Item {
            page: Page::Disk,
            title: "Disco".into(),
            value: format!("{}/s", human((s.disk_read_rate + s.disk_write_rate) as u64)),
            pct: None,
            spark: m.disk_r_hist.iter().zip(&m.disk_w_hist).rev().take(60).rev().map(|(a, b)| (a + b) as f32).collect(),
            spark_max: None,
            grad: Grad::Mono(th.accent),
        });
        items.push(Item {
            page: Page::Sensors,
            title: "Temperaturas".into(),
            value: s.temp_max.map(|t| format!("{t:.0}°C máx")).unwrap_or("-".into()),
            pct: s.temp_max,
            spark: vec![],
            spark_max: None,
            grad: Grad::Heat,
        });
        items.push(Item {
            page: Page::Procs,
            title: "Procesos".into(),
            value: format!("{}", s.tasks),
            pct: None,
            spark: vec![],
            spark_max: None,
            grad: Grad::Heat,
        });

        ui.add_space(6.0);
        ui.label(RichText::new("rtop").size(20.0).strong().color(c(th.accent)));
        ui.add_space(4.0);
        let mut clicked = None;
        for it in &items {
            let w = ui.available_width();
            let has_extra = it.pct.is_some() || !it.spark.is_empty();
            let h = if has_extra { 58.0 } else { 40.0 };
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
            let p = ui.painter();
            let active = self.page == it.page;
            let fill = if active {
                c(th.sel)
            } else if resp.hovered() {
                c(th.sel).gamma_multiply(0.5)
            } else {
                Color32::TRANSPARENT
            };
            p.rect_filled(rect, CornerRadius::same(8), fill);
            if active {
                p.rect_filled(
                    Rect::from_min_size(rect.min + Vec2::new(0.0, 8.0), Vec2::new(3.0, h - 16.0)),
                    CornerRadius::same(2),
                    c(th.accent),
                );
            }
            let pad = 12.0;
            p.text(
                rect.left_top() + Vec2::new(pad, 9.0),
                Align2::LEFT_TOP,
                &it.title,
                FontId::proportional(13.5),
                if active { c(th.text) } else { c(th.text).gamma_multiply(0.85) },
            );
            let vcol = it.pct.map(|v| it.grad.at(&th, v / 100.0)).unwrap_or(c(th.dim));
            p.text(rect.right_top() + Vec2::new(-pad, 10.0), Align2::RIGHT_TOP, &it.value, FontId::proportional(12.0), vcol);
            if has_extra {
                let inner = Rect::from_min_max(rect.left_top() + Vec2::new(pad, 32.0), rect.right_bottom() - Vec2::new(pad, 10.0));
                if !it.spark.is_empty() {
                    paint_spark(p, &th, inner, &it.spark, it.spark_max, it.grad);
                } else if let Some(v) = it.pct {
                    let r = Rect::from_min_size(inner.left_top() + Vec2::new(0.0, 4.0), Vec2::new(inner.width(), 8.0));
                    paint_meter(p, &th, r, v, it.grad);
                }
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                clicked = Some(it.page);
            }
        }
        if let Some(pg) = clicked {
            self.page = pg;
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Ajustes").clicked() {
                    self.show_settings = !self.show_settings;
                }
                let lbl = if self.paused { "Reanudar" } else { "Pausar" };
                if ui.button(lbl).clicked() {
                    self.paused = !self.paused;
                }
            });
            if let Some((m, t)) = &self.flash {
                if t.elapsed() < Duration::from_secs(4) {
                    ui.label(RichText::new(m).color(c(self.th.warn)).size(11.0));
                }
            }
        });
    }

    // ---------- página de procesos ----------

    fn page_procs(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        egui::Frame::new()
            .fill(c(th.panel))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                self.toolbar(ui);
                ui.add_space(4.0);
                self.table(ui);
            });
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(caption(&self.th, "Procesos"));
            ui.add_space(6.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("filtrar por nombre, comando, usuario o pid")
                    .desired_width(300.0),
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
                let mut cmd = self.cfg.show_cmd;
                if ui.checkbox(&mut cmd, "comando completo").changed() {
                    self.cfg.show_cmd = cmd;
                    self.save_cfg();
                }
                if ui.add_enabled(self.selected.is_some(), egui::Button::new("Matar proceso")).clicked() {
                    if let Some(pid) = self.selected {
                        let name = self.mon.proc(pid).map(|p| p.name.clone()).unwrap_or_default();
                        self.confirm_kill = Some((pid, name));
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
            .column(Column::exact(120.0))
            .column(Column::exact(110.0))
            .column(Column::exact(70.0))
            .column(Column::exact(90.0))
            .column(Column::exact(24.0))
            .column(Column::exact(84.0))
            .column(Column::remainder().clip(true))
            .header(26.0, |mut h| {
                h.col(|ui| hdr(ui, "PID", Some(SortBy::Pid), &mut clicked_sort));
                h.col(|ui| hdr(ui, "USUARIO", None, &mut clicked_sort));
                h.col(|ui| hdr(ui, "CPU%", Some(SortBy::Cpu), &mut clicked_sort));
                h.col(|ui| hdr(ui, "MEM%", Some(SortBy::Mem), &mut clicked_sort));
                h.col(|ui| hdr(ui, "RES", None, &mut clicked_sort));
                h.col(|ui| hdr(ui, "DISCO", Some(SortBy::Disk), &mut clicked_sort));
                h.col(|ui| hdr(ui, "S", None, &mut clicked_sort));
                h.col(|ui| hdr(ui, "TIEMPO", Some(SortBy::Time), &mut clicked_sort));
                h.col(|ui| hdr(ui, if show_cmd { "COMANDO" } else { "NOMBRE" }, Some(SortBy::Name), &mut clicked_sort));
            })
            .body(|body| {
                body.rows(22.0, self.view.len(), |mut row| {
                    let p = &self.mon.procs[self.view[row.index()]];
                    row.set_selected(self.selected == Some(p.pid));
                    let mono = |t: String, col: Color32| RichText::new(t).monospace().color(col);
                    let user_c = if p.user == "root" { th.hot } else { th.text };
                    let st_c = match p.state {
                        State::Run => th.ok,
                        State::Stop | State::Disk => th.warn,
                        State::Zombie => th.hot,
                        _ => th.dim,
                    };
                    let label = if show_cmd && !p.cmd.is_empty() { &p.cmd } else { &p.name };
                    let meter_cell = |ui: &mut Ui, v: f32, g: Grad, t: String, faint: bool| {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(44.0, 8.0), Sense::hover());
                        paint_meter(ui.painter(), &th, r, v, g);
                        ui.label(mono(t, if faint { c(th.dim) } else { c(th.text) }));
                    };
                    row.col(|ui| {
                        ui.label(mono(format!("{:>7}", p.pid), c(th.dim)));
                    });
                    row.col(|ui| {
                        ui.label(mono(p.user.clone(), c(user_c)));
                    });
                    row.col(|ui| meter_cell(ui, p.cpu.min(100.0), Grad::Heat, format!("{:>5.1}", p.cpu), p.cpu < 1.0));
                    row.col(|ui| meter_cell(ui, p.mem_pct * 4.0, Grad::Mono(th.mem), format!("{:>4.1}", p.mem_pct), p.mem_pct < 1.0));
                    row.col(|ui| {
                        ui.label(mono(format!("{:>6}", human(p.mem)), c(th.text)));
                    });
                    row.col(|ui| {
                        let io = p.disk_read + p.disk_write;
                        ui.label(mono(
                            if io > 0.0 { format!("{}/s", human(io as u64)) } else { "-".into() },
                            if io > 0.0 { c(th.accent) } else { c(th.dim) },
                        ));
                    });
                    row.col(|ui| {
                        ui.label(mono(p.state.letter().into(), c(st_c)));
                    });
                    row.col(|ui| {
                        ui.label(mono(hms(p.time), c(th.dim)));
                    });
                    row.col(|ui| {
                        ui.label(mono(label.clone(), c(th.text)));
                    });
                    let r = row.response();
                    if r.clicked() {
                        clicked_row = Some(p.pid);
                    }
                    let (pid, name) = (p.pid, p.name.clone());
                    r.context_menu(|ui| {
                        if ui.button("Terminar proceso").clicked() {
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

    // ---------- ventanas ----------

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
                    egui::ComboBox::from_id_salt("theme").selected_text(self.cfg.theme.clone()).show_ui(ui, |ui| {
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

    /// `RTOP_SHOT=/ruta.ppm rtop-gui`: se saca una captura y se cierra
    /// (`RTOP_SHOT_DELAY` segundos, 4 por defecto). Sirve para probar la UI.
    fn debug_shot(&mut self, ctx: &egui::Context) {
        let Some(path) = std::env::var_os("RTOP_SHOT") else { return };
        let delay = std::env::var("RTOP_SHOT_DELAY").ok().and_then(|s| s.parse().ok()).unwrap_or(4);
        if !self.shot_requested && self.started.elapsed() > Duration::from_secs(delay) {
            self.shot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        ctx.request_repaint_after(Duration::from_millis(200));
        let img = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = img {
            let [w, h] = img.size;
            let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
            for px in &img.pixels {
                out.extend_from_slice(&[px.r(), px.g(), px.b()]);
            }
            let _ = std::fs::write(path, out);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn kit_blend(a: Rgb, b: Rgb) -> Color32 {
    Color32::from_rgb(
        ((a.0 as u16 + b.0 as u16) / 2) as u8,
        ((a.1 as u16 + b.1 as u16) / 2) as u8,
        ((a.2 as u16 + b.2 as u16) / 2) as u8,
    )
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

        egui::Panel::right("nav")
            .exact_size(230.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(c(self.th.panel)).inner_margin(egui::Margin::symmetric(10, 10)))
            .show(ui, |ui| self.nav(ui));

        let page = self.page;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(c(self.th.bg)).inner_margin(12.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                if page == Page::Procs {
                    self.page_procs(ui);
                    return;
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match page {
                    Page::Overview => self.page_overview(ui),
                    Page::Cpu => self.page_cpu(ui),
                    Page::Mem => self.page_mem(ui),
                    Page::Gpu(i) => self.page_gpu(ui, i),
                    Page::Net => self.page_net(ui),
                    Page::Disk => self.page_disk(ui),
                    Page::Sensors => self.page_sensors(ui),
                    Page::Procs => {}
                });
            });

        if let Some(p) = self.goto.take() {
            self.page = p;
        }
        if let Some(k) = self.kill_req.take() {
            self.confirm_kill = Some(k);
        }

        self.settings(&ctx);
        self.confirm(&ctx);
        self.debug_shot(&ctx);
    }
}

fn main() -> eframe::Result {
    // En laptops híbridas wgpu elige la GPU potente por defecto: eso despierta
    // la dedicada y gasta batería. Un monitor tiene que usar la integrada.
    if std::env::var_os("WGPU_POWER_PREF").is_none() {
        std::env::set_var("WGPU_POWER_PREF", "low");
    }
    let opts = eframe::NativeOptions {
        // OpenGL gasta bastante menos CPU que wgpu para una UI así
        renderer: if std::env::var("RTOP_RENDERER").as_deref() == Ok("wgpu") {
            eframe::Renderer::Wgpu
        } else {
            eframe::Renderer::Glow
        },
        viewport: egui::ViewportBuilder::default()
            .with_title("rtop")
            .with_app_id("rtop")
            .with_inner_size([1440.0, 960.0])
            .with_min_inner_size([1000.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native("rtop", opts, Box::new(|_cc| Ok(Box::new(App::new()))))
}

