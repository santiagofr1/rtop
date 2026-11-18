use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, RichText, Sense, Stroke, Ui, Vec2,
};
use egui_extras::{Column, TableBuilder};
use egui_plot::{HoverPosition, Line, Plot, PlotPoints};
use rtop_core::{
    config::THEMES,
    fmt::{hms, human},
    Config, Monitor, Rgb, SortBy, State, Theme,
};

/// muestras visibles en los gráficos
const WINDOW: usize = 120;
const PLOT_H: f32 = 90.0;

fn pct_fmt(v: f64) -> String {
    format!("{v:.0}%")
}

fn rate_fmt(v: f64) -> String {
    if v <= 0.0 { "0".into() } else { format!("{}/s", human(v as u64)) }
}

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
    started: Instant,
    shot_requested: bool,
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

    fn card<R>(&self, ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
        egui::Frame::new()
            .fill(c(self.th.panel))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(14, 12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 4.0;
                add(ui)
            })
            .inner
    }

    fn caption(&self, t: &str) -> RichText {
        RichText::new(t.to_uppercase()).color(c(self.th.dim)).size(11.0).strong()
    }

    /// Bloque CPU: el uso es lo protagonista, el modelo va chiquito.
    fn cpu_hero(&self, ui: &mut Ui) {
        let s = &self.mon.stats;
        let th = &self.th;
        self.card(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(150.0, 0.0), Layout::top_down(Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(self.caption("CPU"));
                    ui.label(
                        RichText::new(format!("{:.0}%", s.cpu_total))
                            .size(40.0)
                            .strong()
                            .color(c(th.heat(s.cpu_total))),
                    );
                    ui.label(RichText::new(&s.cpu_model).color(c(th.dim)).size(12.0))
                        .on_hover_text(&s.cpu_brand);
                    let mut extra = format!("{} hilos · {:.1} GHz", s.cores.len(), s.cpu_freq as f32 / 1000.0);
                    if let Some(t) = s.temp_max {
                        extra.push_str(&format!(" · {t:.0}°C"));
                    }
                    ui.label(RichText::new(extra).color(c(th.dim)).size(12.0));
                });
                ui.add_space(8.0);
                self.core_tiles(ui, ui.available_width());
            });
        });
    }

    fn core_tiles(&self, ui: &mut Ui, width: f32) {
        let th = &self.th;
        let cores = &self.mon.stats.cores;
        let n = cores.len().max(1);
        let per_row = n.min(16);
        let rows = n.div_ceil(per_row);
        let gap = 6.0;
        let tile_w = (width - gap * (per_row as f32 - 1.0)) / per_row as f32;
        let tile_h = if rows == 1 { 96.0 } else { 56.0 };
        let (rect, resp) = ui.allocate_exact_size(
            Vec2::new(width, rows as f32 * tile_h + (rows as f32 - 1.0) * gap),
            Sense::hover(),
        );
        let p = ui.painter_at(rect);
        let mut hovered = None;
        for (i, &u) in cores.iter().enumerate() {
            let (r, col) = (i / per_row, i % per_row);
            let min = rect.min + Vec2::new(col as f32 * (tile_w + gap), r as f32 * (tile_h + gap));
            let tile = egui::Rect::from_min_size(min, Vec2::new(tile_w, tile_h));
            let rad = CornerRadius::same(6);
            p.rect_filled(tile, rad, c(th.sel));
            let frac = (u / 100.0).clamp(0.0, 1.0);
            if frac > 0.0 {
                let mut fill = tile;
                fill.set_top(tile.bottom() - tile_h * frac);
                p.rect_filled(fill, rad, c(th.heat(u)).gamma_multiply(0.85));
            }
            p.text(
                tile.center(),
                egui::Align2::CENTER_CENTER,
                format!("{u:.0}%"),
                FontId::proportional(if rows == 1 { 18.0 } else { 13.0 }),
                c(th.text),
            );
            p.text(
                tile.left_top() + Vec2::new(6.0, 4.0),
                egui::Align2::LEFT_TOP,
                format!("{i}"),
                FontId::monospace(10.0),
                c(th.dim),
            );
            if resp.hover_pos().is_some_and(|h| tile.contains(h)) {
                hovered = Some((i, u));
            }
        }
        if let Some((i, u)) = hovered {
            resp.on_hover_text(format!("núcleo {i}: {u:.1}%"));
        }
    }

    /// Tarjeta con título, valor grande, subtítulo y gráfico.
    #[allow(clippy::too_many_arguments)]
    fn metric(
        &self,
        ui: &mut Ui,
        id: &str,
        title: &str,
        value: RichText,
        sub: Vec<RichText>,
        series: Vec<(String, Vec<f64>, Color32)>,
        fmt_y: fn(f64) -> String,
        percent: bool,
    ) {
        self.card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(self.caption(title));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(value);
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                for s in sub {
                    ui.label(s.size(12.0));
                }
            });
            let step = self.cfg.refresh_ms() as f64 / 1000.0;
            let window = WINDOW as f64 * step;
            let mut plot = Plot::new(id)
                .height(PLOT_H)
                .include_x(-window)
                .include_x(0.0)
                .include_y(0.0)
                .show_axes([false, true])
                .show_grid([false, true])
                .show_background(false)
                .y_axis_min_width(46.0)
                .y_axis_formatter(move |m, _| fmt_y(m.value))
                .label_formatter(move |hp| match hp {
                    HoverPosition::NearDataPoint { plot_name, position, .. } => {
                        Some(format!("{plot_name}: {}\nhace {:.0}s", fmt_y(position.y), -position.x))
                    }
                    _ => None,
                })
                .allow_drag(false)
                .allow_zoom(false)
                .allow_scroll(false)
                .allow_boxed_zoom(false)
                .allow_double_click_reset(false)
                .set_margin_fraction(Vec2::new(0.0, 0.08));
            if percent {
                plot = plot.include_y(100.0);
            }
            plot.show(ui, |p| {
                for (name, data, col) in series {
                    let n = data.len();
                    let pts: PlotPoints = data
                        .iter()
                        .enumerate()
                        .map(|(i, &v)| [(i as f64 - (n as f64 - 1.0)) * step, v])
                        .collect::<Vec<_>>()
                        .into();
                    p.line(Line::new(name, pts).color(col).width(1.8).fill(0.0).fill_alpha(0.18));
                }
            });
        });
    }

    fn metrics(&self, ui: &mut Ui) {
        let s = &self.mon.stats;
        let th = &self.th;
        let m = &self.mon;
        let big = |t: String, col: Rgb| RichText::new(t).size(20.0).strong().color(c(col));
        let dim = |t: String| RichText::new(t).color(c(th.dim));
        let tag = |t: String, col: Rgb| RichText::new(t).color(c(col));
        let last = |q: &std::collections::VecDeque<f32>| -> Vec<f64> {
            q.iter().rev().take(WINDOW).rev().map(|&v| v as f64).collect()
        };
        let last64 = |q: &std::collections::VecDeque<f64>| -> Vec<f64> {
            q.iter().rev().take(WINDOW).rev().copied().collect()
        };
        let gpu_cols = [th.accent, th.mem, th.ok, th.warn];

        let cols = if ui.available_width() > 1100.0 { 3 } else { 2 };
        let gap = 10.0;
        let card_w = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32;

        let mut cards: Vec<Box<dyn FnOnce(&mut Ui) + '_>> = vec![];

        cards.push(Box::new(move |ui| {
            self.metric(
                ui,
                "cpu",
                "CPU",
                big(format!("{:.0}%", s.cpu_total), th.heat(s.cpu_total)),
                vec![
                    dim(format!("carga {:.2} {:.2} {:.2}", s.load.0, s.load.1, s.load.2)),
                    dim(format!("{} tareas", s.tasks)),
                ],
                vec![("CPU".into(), last(&m.cpu_hist), c(th.accent))],
                pct_fmt,
                true,
            )
        }));
        cards.push(Box::new(move |ui| {
            self.metric(
                ui,
                "mem",
                "Memoria",
                big(format!("{:.0}%", s.mem_pct()), th.heat(s.mem_pct())),
                vec![
                    dim(format!("{} / {}", human(s.mem_used), human(s.mem_total))),
                    dim(format!("swap {} / {}", human(s.swap_used), human(s.swap_total))),
                ],
                vec![("RAM".into(), last(&m.mem_hist), c(th.mem))],
                pct_fmt,
                true,
            )
        }));
        cards.push(Box::new(move |ui| {
            self.metric(
                ui,
                "net",
                "Red",
                big(format!("{}/s", human((s.rx_rate + s.tx_rate) as u64)), th.text),
                vec![
                    tag(format!("bajada {}/s", human(s.rx_rate as u64)), th.ok),
                    tag(format!("subida {}/s", human(s.tx_rate as u64)), th.warn),
                ],
                vec![
                    ("bajada".into(), last64(&m.rx_hist), c(th.ok)),
                    ("subida".into(), last64(&m.tx_hist), c(th.warn)),
                ],
                rate_fmt,
                false,
            )
        }));
        cards.push(Box::new(move |ui| {
            self.metric(
                ui,
                "disk",
                "Disco",
                big(format!("{}/s", human((s.disk_read_rate + s.disk_write_rate) as u64)), th.text),
                vec![
                    tag(format!("lectura {}/s", human(s.disk_read_rate as u64)), th.accent),
                    tag(format!("escritura {}/s", human(s.disk_write_rate as u64)), th.mem),
                    dim(format!("/ {} de {} ({:.0}%)", human(s.disk_used), human(s.disk_total), s.disk_pct())),
                ],
                vec![
                    ("lectura".into(), last64(&m.disk_r_hist), c(th.accent)),
                    ("escritura".into(), last64(&m.disk_w_hist), c(th.mem)),
                ],
                rate_fmt,
                false,
            )
        }));
        cards.push(Box::new(move |ui| {
            let g0 = s.gpus.iter().find(|g| !g.asleep);
            let v = g0.and_then(|g| g.busy).unwrap_or(0.0);
            let sub = s
                .gpus
                .iter()
                .enumerate()
                .map(|(i, g)| {
                    let col = gpu_cols[i % gpu_cols.len()];
                    if g.asleep {
                        return tag(format!("{} dormida", g.short_name()), th.dim);
                    }
                    let mut t = format!("{} {:.0}%", g.short_name(), g.busy.unwrap_or(0.0));
                    if let Some(x) = g.temp {
                        t.push_str(&format!(" {x:.0}°C"));
                    }
                    if let Some(w) = g.power_w {
                        t.push_str(&format!(" {w:.1}W"));
                    }
                    tag(t, col)
                })
                .collect::<Vec<_>>();
            let series = s
                .gpus
                .iter()
                .enumerate()
                .map(|(i, g)| (g.short_name(), last(&m.gpu_hist[i]), c(gpu_cols[i % gpu_cols.len()])))
                .collect();
            self.metric(
                ui,
                "gpu",
                "GPU",
                big(if s.gpus.is_empty() { "-".into() } else { format!("{v:.0}%") }, th.heat(v)),
                if sub.is_empty() { vec![dim("no se detectó GPU".into())] } else { sub },
                series,
                pct_fmt,
                true,
            )
        }));
        cards.push(Box::new(move |ui| {
            let g0 = s.gpus.iter().find(|g| !g.asleep && g.vram_total > 0);
            let sub = s
                .gpus
                .iter()
                .enumerate()
                .filter(|(_, g)| !g.asleep && g.vram_total > 0)
                .map(|(i, g)| {
                    let mut t = format!("{} {} / {}", g.short_name(), human(g.vram_used), human(g.vram_total));
                    if g.shared_total > 0 {
                        t.push_str(&format!(" (+{} compartida)", human(g.shared_used)));
                    }
                    tag(t, gpu_cols[i % gpu_cols.len()])
                })
                .collect::<Vec<_>>();
            let series = s
                .gpus
                .iter()
                .enumerate()
                .filter(|(_, g)| !g.asleep)
                .map(|(i, g)| (g.short_name(), last(&m.vram_hist[i]), c(gpu_cols[i % gpu_cols.len()])))
                .collect();
            self.metric(
                ui,
                "vram",
                "VRAM",
                match g0 {
                    Some(g) => big(format!("{:.0}%", g.vram_pct()), th.heat(g.vram_pct())),
                    None => big("-".into(), th.dim),
                },
                if sub.is_empty() { vec![dim("sin datos de VRAM".into())] } else { sub },
                series,
                pct_fmt,
                true,
            )
        }));

        let mut it = cards.into_iter().peekable();
        while it.peek().is_some() {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for _ in 0..cols {
                    if let Some(f) = it.next() {
                        ui.allocate_ui_with_layout(
                            Vec2::new(card_w, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| f(ui),
                        );
                    }
                }
            });
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(self.caption("Procesos"));
            ui.add_space(6.0);
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

impl App {
    /// `RTOP_SHOT=/ruta.ppm rtop-gui`: se saca una captura a los 4 s y se cierra.
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
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                self.cpu_hero(ui);
                if self.cfg.show_graphs {
                    self.metrics(ui);
                }
                egui::Frame::new()
                    .fill(c(self.th.panel))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.set_min_height(ui.available_height());
                        self.toolbar(ui);
                        ui.add_space(4.0);
                        self.table(ui);
                    });
            });

        self.settings(&ctx);
        self.confirm(&ctx);
        self.debug_shot(&ctx);
    }
}

fn main() -> eframe::Result {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("rtop")
            .with_app_id("rtop")
            .with_inner_size([1360.0, 960.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("rtop", opts, Box::new(|_cc| Ok(Box::new(App::new()))))
}
