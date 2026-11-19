mod home;
mod kit;
mod pages;

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    time::{Duration, Instant},
};

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, Vec2};
use kit::{c, caption, paint_meter, paint_spark, tail_f32, tail_f64, Grad};
use rtop_core::{config::THEMES, fmt::human, Config, Monitor, ProcDetails, Rgb, SortBy, Theme};

/// muestras visibles en los gráficos
pub const WINDOW: usize = 150;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Home,
    Cpu,
    Gpu(usize),
    Mem,
    Disk,
    Net,
    Sensors,
}

impl Page {
    fn title(self, app: &App) -> String {
        match self {
            Page::Home => "Inicio".into(),
            Page::Cpu => "CPU".into(),
            Page::Gpu(i) => match app.mon.stats.gpus.get(i) {
                Some(g) => format!("GPU {} · {}", i + 1, g.short_name()),
                None => format!("GPU {}", i + 1),
            },
            Page::Mem => "Memoria".into(),
            Page::Disk => "Disco".into(),
            Page::Net => "Red".into(),
            Page::Sensors => "Temperaturas".into(),
        }
    }
}

struct App {
    mon: Monitor,
    cfg: Config,
    th: Theme,
    page: Page,
    // tabla de procesos
    view: Vec<usize>,
    sort: SortBy,
    reverse: bool,
    filter: String,
    selected: Option<u32>,
    details: Option<(u32, ProcDetails)>,
    /// historial de CPU y RAM del proceso seleccionado
    sel_cpu: VecDeque<f32>,
    sel_mem: VecDeque<f32>,
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
    clicks_done: u64,
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
            _ => Page::Home,
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
            details: None,
            sel_cpu: VecDeque::new(),
            sel_mem: VecDeque::new(),
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
            clicks_done: 0,
        };
        if let Ok(pid) = std::env::var("RTOP_SELECT").unwrap_or_default().parse::<u32>() {
            a.select(Some(pid));
        }
        a.rebuild();
        a
    }

    fn rebuild(&mut self) {
        self.view = self.mon.view(&self.filter, self.sort, self.reverse);
    }

    fn msg(&mut self, m: impl Into<String>) {
        self.flash = Some((m.into(), Instant::now()));
    }

    fn select(&mut self, pid: Option<u32>) {
        if pid != self.selected {
            self.sel_cpu.clear();
            self.sel_mem.clear();
        }
        self.selected = pid;
        self.details = pid.map(|p| (p, ProcDetails::read(p)));
    }

    fn refresh(&mut self) {
        self.mon.refresh();
        self.rebuild();
        if let Some(pid) = self.selected {
            match self.mon.proc(pid) {
                Some(p) => {
                    let (cpu, mem) = (p.cpu, p.mem_pct);
                    for (q, v) in [(&mut self.sel_cpu, cpu), (&mut self.sel_mem, mem)] {
                        if q.len() == WINDOW {
                            q.pop_front();
                        }
                        q.push_back(v);
                    }
                    self.details = Some((pid, ProcDetails::read(pid)));
                }
                // el proceso terminó: se deja de mostrar
                None => self.select(None),
            }
        }
    }

    fn go(&mut self, p: Page) {
        self.page = p;
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
        v.faint_bg_color = kit::blend(th.panel, th.sel);
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
        let mut items = vec![Item {
            page: Page::Cpu,
            title: "CPU".into(),
            value: format!("{:.0}%", s.cpu_total),
            pct: Some(s.cpu_total),
            spark: tail_f32(&m.cpu_hist, 60),
            spark_max: Some(100.0),
            grad: Grad::Heat,
        }];
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
            page: Page::Mem,
            title: "Memoria".into(),
            value: format!("{} / {}", human(s.mem_used), human(s.mem_total)),
            pct: Some(s.mem_pct()),
            spark: tail_f32(&m.mem_hist, 60),
            spark_max: Some(100.0),
            grad: Grad::Mono(th.mem),
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
            page: Page::Net,
            title: "Red".into(),
            value: format!("v {}/s", human(s.rx_rate as u64)),
            pct: None,
            spark: tail_f64(&m.rx_hist, 60),
            spark_max: None,
            grad: Grad::Mono(th.ok),
        });
        let main_temp = s.cpu_temp.or(s.temp_max);
        items.push(Item {
            page: Page::Sensors,
            title: "Temperaturas".into(),
            value: main_temp.map(|t| format!("{t:.0}°C")).unwrap_or("-".into()),
            pct: main_temp,
            spark: s
                .temps
                .iter()
                .find(|t| Some(t.value) == main_temp)
                .and_then(|t| m.temp_hist.get(&t.label))
                .map(|h| tail_f32(h, 60))
                .unwrap_or_default(),
            spark_max: Some(100.0),
            grad: Grad::Heat,
        });

        ui.add_space(4.0);
        ui.label(RichText::new("rtop").size(20.0).strong().color(c(th.accent)));
        ui.add_space(6.0);

        let mut clicked = None;
        // Inicio: separado y siempre arriba
        if nav_button(ui, &th, self.page == Page::Home, "Inicio", &s.host, 44.0, |_, _| {}).clicked() {
            clicked = Some(Page::Home);
        }
        ui.add_space(8.0);
        ui.label(caption(&th, "Componentes"));
        ui.add_space(2.0);
        for it in &items {
            let active = self.page == it.page;
            let vcol = it.pct.map(|v| it.grad.at(&th, v / 100.0));
            let resp = nav_button_colored(ui, &th, active, &it.title, &it.value, vcol, 58.0, |p, inner| {
                if !it.spark.is_empty() {
                    paint_spark(p, &th, inner, &it.spark, it.spark_max, it.grad);
                } else if let Some(v) = it.pct {
                    let r = Rect::from_min_size(inner.left_top() + Vec2::new(0.0, 4.0), Vec2::new(inner.width(), 8.0));
                    paint_meter(p, &th, r, v, it.grad);
                }
            });
            if resp.clicked() {
                clicked = Some(it.page);
            }
        }
        if let Some(pg) = clicked {
            self.go(pg);
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(4.0);
            ui.label(dim_small(&th, "Esc o Alt + flecha izquierda: volver al inicio"));
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

    /// Barra de arriba de las páginas de detalle, con el botón para volver.
    fn page_bar(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let title = self.page.title(self);
        ui.horizontal(|ui| {
            let back = egui::Button::new(RichText::new("‹  Inicio").size(14.0).color(c(th.text)))
                .fill(c(th.panel))
                .corner_radius(CornerRadius::same(8))
                .min_size(Vec2::new(96.0, 30.0));
            if ui.add(back).on_hover_text("volver a la pantalla principal (Esc)").clicked() {
                self.go(Page::Home);
            }
            ui.add_space(8.0);
            ui.label(RichText::new(title).size(20.0).strong().color(c(th.text)));
        });
    }

    // ---------- ventanas ----------

    fn settings(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        let mut changed = false;
        egui::Window::new("Ajustes").open(&mut open).resizable(false).default_width(320.0).show(ctx, |ui| {
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

    /// Volver al inicio con Esc, Alt+← o el botón "atrás" del mouse.
    fn back_shortcuts(&mut self, ctx: &egui::Context) {
        if self.confirm_kill.is_some() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let (esc, alt_left, mouse_back) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::Escape),
                i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft),
                i.pointer.button_clicked(egui::PointerButton::Extra1),
            )
        });
        if self.page != Page::Home && (esc || alt_left || mouse_back) {
            self.go(Page::Home);
        } else if self.page == Page::Home && esc {
            self.select(None);
        }
    }
}

fn dim_small(th: &Theme, t: &str) -> RichText {
    RichText::new(t).color(c(th.dim)).size(10.5)
}

fn nav_button(
    ui: &mut Ui,
    th: &Theme,
    active: bool,
    title: &str,
    value: &str,
    h: f32,
    extra: impl FnOnce(&egui::Painter, Rect),
) -> egui::Response {
    nav_button_colored(ui, th, active, title, value, None, h, extra)
}

#[allow(clippy::too_many_arguments)]
fn nav_button_colored(
    ui: &mut Ui,
    th: &Theme,
    active: bool,
    title: &str,
    value: &str,
    value_color: Option<Color32>,
    h: f32,
    extra: impl FnOnce(&egui::Painter, Rect),
) -> egui::Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let p = ui.painter();
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
    let top = if h < 50.0 { (h - 16.0) / 2.0 } else { 9.0 };
    p.text(
        rect.left_top() + Vec2::new(pad, top),
        Align2::LEFT_TOP,
        title,
        FontId::proportional(13.5),
        if active { c(th.text) } else { c(th.text).gamma_multiply(0.85) },
    );
    p.text(
        rect.right_top() + Vec2::new(-pad, top + 1.0),
        Align2::RIGHT_TOP,
        value,
        FontId::proportional(12.0),
        value_color.unwrap_or(c(th.dim)),
    );
    if h >= 50.0 {
        let inner = Rect::from_min_max(rect.left_top() + Vec2::new(pad, 32.0), rect.right_bottom() - Vec2::new(pad, 10.0));
        extra(p, inner);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl eframe::App for App {
    /// `RTOP_CLICKS="6:1320,64;9:1300,110"`: clicks simulados (segundo:x,y) para probar la UI.
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let Ok(spec) = std::env::var("RTOP_CLICKS") else { return };
        let t = self.started.elapsed().as_secs_f32();
        for (i, c) in spec.split(';').enumerate() {
            let Some((ts, xy)) = c.split_once(':') else { continue };
            let Some((x, y)) = xy.split_once(',') else { continue };
            let (Ok(ts), Ok(x), Ok(y)) = (ts.parse::<f32>(), x.parse::<f32>(), y.parse::<f32>()) else { continue };
            let bit = 1u64 << i;
            if t >= ts && self.clicks_done & bit == 0 {
                self.clicks_done |= bit;
                let pos = egui::pos2(x, y);
                raw.events.push(egui::Event::PointerMoved(pos));
                for pressed in [true, false] {
                    raw.events.push(egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    });
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_visuals(&ctx);

        let tick = Duration::from_millis(self.cfg.refresh_ms());
        if !self.paused && self.last.elapsed() >= tick {
            self.refresh();
            self.last = Instant::now();
        }
        ctx.request_repaint_after(tick.saturating_sub(self.last.elapsed()));
        self.back_shortcuts(&ctx);

        egui::Panel::right("nav")
            .exact_size(236.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(c(self.th.panel)).inner_margin(egui::Margin::symmetric(10, 10)))
            .show(ui, |ui| self.nav(ui));

        let page = self.page;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(c(self.th.bg)).inner_margin(12.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                if page == Page::Home {
                    self.page_home(ui);
                    return;
                }
                self.page_bar(ui);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_max_width(ui.available_width());
                    match page {
                        Page::Cpu => self.page_cpu(ui),
                        Page::Mem => self.page_mem(ui),
                        Page::Gpu(i) => self.page_gpu(ui, i),
                        Page::Net => self.page_net(ui),
                        Page::Disk => self.page_disk(ui),
                        Page::Sensors => self.page_sensors(ui),
                        Page::Home => {}
                    }
                });
            });

        if let Some(p) = self.goto.take() {
            self.go(p);
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
    let size = std::env::var("RTOP_SIZE")
        .ok()
        .and_then(|s| s.split_once('x').and_then(|(w, h)| Some([w.parse().ok()?, h.parse().ok()?])))
        .unwrap_or([1440.0, 960.0]);
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
            .with_inner_size(size)
            .with_min_inner_size([1000.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native("rtop", opts, Box::new(|_cc| Ok(Box::new(App::new()))))
}
