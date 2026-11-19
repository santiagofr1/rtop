mod home;
mod kit;
mod pages;

use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    time::{Duration, Instant},
};

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Key, Layout, RichText, Sense, Stroke, Ui, Vec2};
use kit::{body, c, meta, muted, Grad};
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
    scroll_to: Option<usize>,
    details: Option<(u32, ProcDetails)>,
    /// historial de CPU y RAM del proceso seleccionado
    sel_cpu: VecDeque<f32>,
    sel_mem: VecDeque<f32>,
    confirm_kill: Option<(u32, String)>,
    /// pedidos desde páginas que no tienen `&mut self`
    kill_req: RefCell<Option<(u32, String)>>,
    goto: Cell<Option<Page>>,
    select_req: Cell<Option<u32>>,
    show_settings: bool,
    show_keys: bool,
    paused: bool,
    last: Instant,
    flash: Option<(String, bool, Instant)>,
    visuals_for: Option<Theme>,
    started: Instant,
    shot_requested: bool,
    clicks_done: u64,
    /// conexiones por proceso (solo se leen con la página Red abierta)
    conns: Vec<rtop_core::netproc::Conn>,
    conns_at: Option<Instant>,
    /// sensor ampliado en la página de temperaturas
    sensor_open: RefCell<Option<String>>,
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
            scroll_to: None,
            details: None,
            sel_cpu: VecDeque::new(),
            sel_mem: VecDeque::new(),
            confirm_kill: None,
            kill_req: RefCell::new(None),
            goto: Cell::new(None),
            select_req: Cell::new(None),
            show_settings: false,
            show_keys: std::env::var_os("RTOP_KEYS").is_some(),
            paused: false,
            last: Instant::now(),
            flash: None,
            visuals_for: None,
            started: Instant::now(),
            shot_requested: false,
            clicks_done: 0,
            conns: vec![],
            conns_at: None,
            sensor_open: RefCell::new(None),
        };
        if let Ok(l) = std::env::var("RTOP_SENSOR") {
            *a.sensor_open.borrow_mut() = Some(l);
        }
        if let Ok(pid) = std::env::var("RTOP_SELECT").unwrap_or_default().parse::<u32>() {
            a.select(Some(pid));
        }
        a.rebuild();
        a
    }

    fn rebuild(&mut self) {
        self.view = self.mon.view(&self.filter, self.sort, self.reverse);
    }

    fn msg(&mut self, m: impl Into<String>, ok: bool) {
        self.flash = Some((m.into(), ok, Instant::now()));
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
        if self.page == Page::Net && self.conns_at.is_none_or(|t| t.elapsed().as_secs_f32() >= 2.5) {
            self.conns = rtop_core::netproc::scan();
            self.conns_at = Some(Instant::now());
        }
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
        if p == Page::Net && self.page != Page::Net {
            self.conns = rtop_core::netproc::scan();
            self.conns_at = Some(Instant::now());
        }
        self.page = p;
    }

    /// Orden de la barra lateral; también define los atajos 1..8.
    fn pages(&self) -> Vec<Page> {
        let mut v = vec![Page::Home, Page::Cpu];
        v.extend((0..self.mon.stats.gpus.len()).map(Page::Gpu));
        v.extend([Page::Mem, Page::Disk, Page::Net, Page::Sensors]);
        v
    }

    fn page_title(&self, p: Page) -> String {
        match p {
            Page::Home => "Inicio".into(),
            Page::Cpu => "Procesador".into(),
            Page::Gpu(i) => match self.mon.stats.gpus.get(i) {
                Some(g) => g.short_name(),
                None => format!("GPU {}", i + 1),
            },
            Page::Mem => "Memoria".into(),
            Page::Disk => "Disco".into(),
            Page::Net => "Red".into(),
            Page::Sensors => "Temperaturas".into(),
        }
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
        v.text_cursor.stroke = Stroke::new(2.0, c(th.accent));
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, c(th.sel));
        v.widgets.inactive.bg_fill = c(th.sel);
        v.widgets.inactive.weak_bg_fill = c(th.sel);
        v.widgets.hovered.bg_fill = kit::blend(th.sel, th.dim);
        v.widgets.hovered.weak_bg_fill = kit::blend(th.sel, th.dim);
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, c(th.accent).gamma_multiply(0.6));
        v.widgets.active.bg_fill = c(th.accent);
        v.widgets.active.weak_bg_fill = c(th.accent);
        ctx.set_visuals(v);
        ctx.global_style_mut(|s| {
            s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(kit::T_BODY));
            s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.5));
            s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(kit::T_META));
            s.spacing.button_padding = Vec2::new(10.0, 5.0);
            s.spacing.interact_size.y = 28.0;
        });
        self.visuals_for = Some(self.th.clone());
    }

    fn save_cfg(&mut self) {
        self.th = self.cfg.theme();
        if let Err(e) = self.cfg.save() {
            self.msg(format!("No pude guardar la configuración: {e}"), false);
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
        let heat = |v: f32| Grad::Heat.at(&th, v / 100.0);
        let value = |p: Page| -> (String, Color32) {
            match p {
                Page::Home => (String::new(), muted(&th)),
                Page::Cpu => (format!("{:.0}%", s.cpu_total), heat(s.cpu_total)),
                Page::Gpu(i) => match s.gpus.get(i) {
                    Some(g) if g.asleep => ("dormida".into(), c(th.ok)),
                    Some(g) => (format!("{:.0}%", g.busy.unwrap_or(0.0)), heat(g.busy.unwrap_or(0.0))),
                    None => (String::new(), muted(&th)),
                },
                Page::Mem => (format!("{:.0}%", s.mem_pct()), heat(s.mem_pct())),
                Page::Disk => (format!("{}/s", human((s.disk_read_rate + s.disk_write_rate) as u64)), muted(&th)),
                Page::Net => (format!("{}/s", human((s.rx_rate + s.tx_rate) as u64)), muted(&th)),
                Page::Sensors => {
                    let t = s.cpu_temp.or(s.temp_max);
                    (t.map(|t| format!("{t:.0} °C")).unwrap_or_default(), t.map(heat).unwrap_or(muted(&th)))
                }
            }
        };

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            ui.label(kit::strong("rtop", c(th.text), 18.0));
            ui.label(meta(&th, &s.host));
        });
        ui.add_space(14.0);

        let mut clicked = None;
        for (n, p) in self.pages().iter().enumerate() {
            // separación entre grupos: Inicio | componentes | temperaturas
            if matches!(p, Page::Cpu | Page::Sensors) {
                ui.add_space(10.0);
            }
            let active = self.page == *p;
            let w = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::click());
            let painter = ui.painter();
            let fill = if active {
                c(th.sel)
            } else if resp.hovered() {
                c(th.sel).gamma_multiply(0.45)
            } else {
                Color32::TRANSPARENT
            };
            painter.rect_filled(rect, CornerRadius::same(8), fill);
            let font = if active {
                FontId::new(kit::T_BODY, egui::FontFamily::Name("semibold".into()))
            } else {
                FontId::proportional(kit::T_BODY)
            };
            painter.text(rect.left_center() + Vec2::new(12.0, 0.0), Align2::LEFT_CENTER, self.page_title(*p), font, c(th.text));
            let (v, col) = value(*p);
            painter.text(rect.right_center() - Vec2::new(12.0, 0.0), Align2::RIGHT_CENTER, v, FontId::proportional(13.0), col);
            let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("Tecla {}", n + 1));
            if resp.clicked() {
                clicked = Some(*p);
            }
        }
        if let Some(pg) = clicked {
            self.go(pg);
        }

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let lbl = if self.paused { "Reanudar" } else { "Pausar" };
                if ui.button(lbl).on_hover_text("Espacio").clicked() {
                    self.paused = !self.paused;
                }
                if ui.button("Ajustes").clicked() {
                    self.show_settings = !self.show_settings;
                }
                if ui.button("Atajos").on_hover_text("F1").clicked() {
                    self.show_keys = !self.show_keys;
                }
            });
        });
    }

    /// Encabezado de las páginas de detalle, con el botón para volver.
    fn page_bar(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let title = self.page_title(self.page);
        ui.horizontal(|ui| {
            let back = egui::Button::new(body(&th, "‹  Inicio")).corner_radius(CornerRadius::same(8)).min_size(Vec2::new(92.0, 32.0));
            if ui.add(back).on_hover_text("Volver a la pantalla principal (Esc)").clicked() {
                self.go(Page::Home);
            }
            ui.add_space(10.0);
            ui.label(kit::title(&th, title));
        });
    }

    fn pause_banner(&mut self, ui: &mut Ui) {
        if !self.paused {
            return;
        }
        let th = self.th.clone();
        egui::Frame::new()
            .fill(c(th.warn).gamma_multiply(0.18))
            .stroke(Stroke::new(1.0, c(th.warn).gamma_multiply(0.5)))
            .corner_radius(CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(14, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(kit::strong("En pausa", c(th.warn), kit::T_BODY));
                    ui.label(body(&th, format!("los datos son de hace {} s", self.last.elapsed().as_secs())));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Reanudar").clicked() {
                            self.paused = false;
                        }
                    });
                });
            });
    }

    // ---------- ventanas ----------

    fn toast(&mut self, ctx: &egui::Context) {
        let Some((m, ok, t)) = self.flash.clone() else { return };
        if t.elapsed() > Duration::from_secs(5) {
            self.flash = None;
            return;
        }
        let th = &self.th;
        let col = if ok { c(th.ok) } else { c(th.hot) };
        egui::Area::new(egui::Id::new("toast")).anchor(Align2::CENTER_BOTTOM, Vec2::new(-118.0, -24.0)).show(ctx, |ui| {
            egui::Frame::new()
                .fill(c(th.panel))
                .stroke(Stroke::new(1.0, col))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(16, 10))
                .show(ui, |ui| {
                    ui.label(body(th, m));
                });
        });
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn keys_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_keys;
        let th = self.th.clone();
        egui::Window::new("Atajos de teclado").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            let rows = [
                ("1 a 8", "ir a cada sección (en el orden de la barra)"),
                ("Esc", "volver a Inicio, o cerrar el detalle del proceso"),
                ("Alt + flecha izquierda", "volver a Inicio"),
                ("Flechas o j / k", "moverse por la tabla de procesos"),
                ("Re Pág / Av Pág", "saltar de a 15 procesos"),
                ("/", "buscar un proceso"),
                ("Supr", "terminar el proceso elegido (pide confirmación)"),
                ("Espacio", "pausar o reanudar"),
                ("F1", "mostrar esta ayuda"),
            ];
            egui::Grid::new("keys").num_columns(2).spacing([24.0, 8.0]).show(ui, |ui| {
                for (k, d) in rows {
                    ui.label(kit::num(k, c(th.accent)));
                    ui.label(body(&th, d));
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.label(meta(&th, "En la tabla: click en un título para ordenar, click derecho en un proceso para terminarlo."));
        });
        self.show_keys = open;
    }

    fn settings(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        let mut changed = false;
        egui::Window::new("Ajustes").open(&mut open).resizable(false).default_width(340.0).show(ctx, |ui| {
            egui::Grid::new("cfg").num_columns(2).spacing([16.0, 10.0]).show(ui, |ui| {
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

                ui.label("Actualizar cada");
                let mut ms = self.cfg.refresh_ms();
                if ui.add(egui::Slider::new(&mut ms, 250..=5000).step_by(250.0).suffix(" ms")).changed() {
                    self.cfg.refresh_ms = ms;
                    changed = true;
                }
                ui.end_row();

                ui.label("CPU de cada proceso");
                let mut v = self.cfg.cpu_per_core;
                ui.vertical(|ui| {
                    ui.radio_value(&mut v, false, "% del total");
                    ui.radio_value(&mut v, true, "% de un núcleo");
                });
                if v != self.cfg.cpu_per_core {
                    self.cfg.cpu_per_core = v;
                    changed = true;
                }
                ui.end_row();
            });

            ui.separator();
            ui.label(kit::section_title(&self.th, "Colores"));
            ui.label(meta(&self.th, "Pisan los del tema y se guardan en la configuración."));
            let th = self.th.clone();
            egui::Grid::new("colors").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
                let ov = &mut self.cfg.colors;
                for (name, cur, slot) in [
                    ("Fondo", th.bg, &mut ov.bg),
                    ("Paneles", th.panel, &mut ov.panel),
                    ("Texto", th.text, &mut ov.text),
                    ("Tenue", th.dim, &mut ov.dim),
                    ("Acento", th.accent, &mut ov.accent),
                    ("Bien", th.ok, &mut ov.ok),
                    ("Alerta", th.warn, &mut ov.warn),
                    ("Crítico", th.hot, &mut ov.hot),
                    ("Selección", th.sel, &mut ov.sel),
                    ("Memoria", th.mem, &mut ov.mem),
                ] {
                    ui.label(name);
                    let mut rgb = [cur.0, cur.1, cur.2];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        *slot = Some(Rgb(rgb[0], rgb[1], rgb[2]).hex());
                        changed = true;
                    }
                    if slot.is_some() {
                        if ui.small_button("Quitar").clicked() {
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
                    ui.label(meta(&self.th, p.display().to_string()));
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
        let th = self.th.clone();
        let resp = egui::Modal::new(egui::Id::new("kill")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(kit::section_title(&th, format!("¿Terminar {name}?")));
            ui.label(meta(&th, format!("PID {pid}. Si tiene trabajo sin guardar, se pierde.")));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button("Cancelar").clicked() {
                    action = Some(None);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button(RichText::new("Forzar").color(c(th.hot)))
                        .on_hover_text("SIGKILL: lo corta en seco, para cuando no responde")
                        .clicked()
                    {
                        action = Some(Some(true));
                    }
                    if ui
                        .add(egui::Button::new(RichText::new("Terminar").color(c(th.bg))).fill(c(th.hot)))
                        .on_hover_text("SIGTERM: le pide que cierre")
                        .clicked()
                    {
                        action = Some(Some(false));
                    }
                });
            });
        });
        if resp.should_close() && action.is_none() {
            action = Some(None);
        }
        if let Some(a) = action {
            self.confirm_kill = None;
            if let Some(force) = a {
                if self.mon.kill(pid, force) {
                    self.msg(format!("Le pedí a {name} que termine."), true);
                } else {
                    self.msg(format!("No pude terminar {name}: es de otro usuario o del sistema. Probá con sudo."), false);
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

    /// Atajos globales: Esc / Alt+← / botón atrás del mouse, 1..8, Espacio, F1.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.confirm_kill.is_some() || ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let (esc, alt_left, mouse_back, space, f1, num) = ctx.input(|i| {
            let num = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8]
                .iter()
                .position(|k| i.key_pressed(*k) && !i.modifiers.any());
            (
                i.key_pressed(Key::Escape),
                i.modifiers.alt && i.key_pressed(Key::ArrowLeft),
                i.pointer.button_clicked(egui::PointerButton::Extra1),
                i.key_pressed(Key::Space),
                i.key_pressed(Key::F1),
                num,
            )
        });
        if let Some(n) = num {
            if let Some(p) = self.pages().get(n) {
                self.go(*p);
            }
        }
        if space {
            self.paused = !self.paused;
        }
        if f1 {
            self.show_keys = !self.show_keys;
        }
        if self.page != Page::Home && (esc || alt_left || mouse_back) {
            self.go(Page::Home);
        } else if self.page == Page::Home && esc {
            self.select(None);
        }
    }
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
        ctx.request_repaint_after(if self.paused { Duration::from_secs(1) } else { tick.saturating_sub(self.last.elapsed()) });
        self.shortcuts(&ctx);

        egui::Panel::right("nav")
            .exact_size(236.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(kit::layer2(&self.th)).inner_margin(egui::Margin::symmetric(10, 12)))
            .show(ui, |ui| self.nav(ui));

        let page = self.page;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(c(self.th.bg)).inner_margin(egui::Margin::symmetric(16, 14)))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
                self.pause_banner(ui);
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
                    ui.add_space(8.0);
                });
            });

        if let Some(p) = self.goto.take() {
            self.go(p);
        }
        if let Some(pid) = self.select_req.take() {
            self.select(Some(pid));
            self.scroll_to = self.view.iter().position(|&i| self.mon.procs[i].pid == pid);
        }
        if let Some(k) = self.kill_req.take() {
            self.confirm_kill = Some(k);
        }

        self.toast(&ctx);
        self.keys_window(&ctx);
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
    eframe::run_native(
        "rtop",
        opts,
        Box::new(|cc| {
            kit::install_fonts(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}
