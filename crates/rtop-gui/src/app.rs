//! Estado de la aplicación y el ciclo de cada frame.
//!
//! Las páginas dibujan con `&self` y piden cambios con `act(Action)`; las
//! acciones se aplican todas juntas al final del frame (`apply_actions`).
//! Así hay un solo lugar donde cambia el estado a pedido de la interfaz.

use std::{
    cell::RefCell,
    collections::VecDeque,
    time::{Duration, Instant},
};

use eframe::egui::{self, Ui, Vec2};
use rtop_core::{netproc::Conn, Config, Monitor, ProcDetails, SortBy, Theme};

use crate::{devtools, kit};

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

/// Pedidos de las páginas, que se aplican al final del frame.
pub enum Action {
    /// ir a Inicio con ese proceso elegido
    ShowProc(u32),
    /// pedir confirmación para terminar (pid, nombre)
    AskKill(u32, String),
    /// sensor ampliado en Temperaturas (None = cerrar)
    OpenSensor(Option<String>),
}

/// Tabla de procesos de Inicio.
pub struct TableState {
    /// índices sobre `mon.procs`, ya filtrados y ordenados
    pub view: Vec<usize>,
    pub sort: SortBy,
    pub reverse: bool,
    pub filter: String,
    pub scroll_to: Option<usize>,
}

/// Proceso elegido y su historia desde que se lo eligió.
#[derive(Default)]
pub struct Selection {
    pub pid: Option<u32>,
    pub details: Option<ProcDetails>,
    pub cpu: VecDeque<f32>,
    pub mem: VecDeque<f32>,
}

/// Conexiones por proceso: solo se leen con la página Red abierta.
#[derive(Default)]
pub struct NetState {
    pub conns: Vec<Conn>,
    pub at: Option<Instant>,
}

#[derive(Default)]
pub struct SensorsState {
    pub open: Option<String>,
}

#[derive(Default)]
pub struct Dialogs {
    pub settings: bool,
    pub keys: bool,
    pub confirm_kill: Option<(u32, String)>,
    /// mensaje abajo (texto, salió bien, cuándo)
    pub flash: Option<(String, bool, Instant)>,
}

pub struct App {
    pub mon: Monitor,
    pub cfg: Config,
    pub th: Theme,
    pub page: Page,
    pub table: TableState,
    pub sel: Selection,
    pub net: NetState,
    pub sensors: SensorsState,
    pub dialogs: Dialogs,
    pub paused: bool,
    /// última actualización de datos
    pub last: Instant,
    pub dev: devtools::DevState,
    actions: RefCell<Vec<Action>>,
    visuals_for: Option<Theme>,
}

impl App {
    pub fn new() -> Self {
        let cfg = Config::load();
        let mut a = App {
            mon: Monitor::new(),
            th: cfg.theme(),
            cfg,
            page: Page::Home,
            table: TableState { view: vec![], sort: SortBy::Cpu, reverse: false, filter: String::new(), scroll_to: None },
            sel: Selection::default(),
            net: NetState::default(),
            sensors: SensorsState::default(),
            dialogs: Dialogs::default(),
            paused: false,
            last: Instant::now(),
            dev: devtools::DevState::default(),
            actions: RefCell::new(vec![]),
            visuals_for: None,
        };
        devtools::setup(&mut a);
        a.rebuild();
        a
    }

    /// Pedir un cambio desde código que solo tiene `&self`.
    pub fn act(&self, a: Action) {
        self.actions.borrow_mut().push(a);
    }

    fn apply_actions(&mut self) {
        for a in self.actions.take() {
            match a {
                Action::ShowProc(pid) => {
                    self.go(Page::Home);
                    self.select(Some(pid));
                    self.table.scroll_to = self.table.view.iter().position(|&i| self.mon.procs[i].pid == pid);
                }
                Action::AskKill(pid, name) => self.dialogs.confirm_kill = Some((pid, name)),
                Action::OpenSensor(s) => self.sensors.open = s,
            }
        }
    }

    pub fn rebuild(&mut self) {
        let t = &mut self.table;
        t.view = self.mon.view(&t.filter, t.sort, t.reverse);
    }

    pub fn msg(&mut self, m: impl Into<String>, ok: bool) {
        self.dialogs.flash = Some((m.into(), ok, Instant::now()));
    }

    pub fn select(&mut self, pid: Option<u32>) {
        if pid != self.sel.pid {
            self.sel.cpu.clear();
            self.sel.mem.clear();
        }
        self.sel.pid = pid;
        self.sel.details = pid.map(ProcDetails::read);
    }

    pub fn go(&mut self, p: Page) {
        if p == Page::Net && self.page != Page::Net {
            self.scan_conns();
        }
        self.page = p;
    }

    fn scan_conns(&mut self) {
        self.net.conns = rtop_core::netproc::scan();
        self.net.at = Some(Instant::now());
    }

    fn refresh(&mut self) {
        self.mon.refresh();
        self.rebuild();
        if self.page == Page::Net && self.net.at.is_none_or(|t| t.elapsed().as_secs_f32() >= 2.5) {
            self.scan_conns();
        }
        let Some(pid) = self.sel.pid else { return };
        match self.mon.proc(pid) {
            Some(p) => {
                let (cpu, mem) = (p.cpu, p.mem_pct);
                for (q, v) in [(&mut self.sel.cpu, cpu), (&mut self.sel.mem, mem)] {
                    if q.len() == WINDOW {
                        q.pop_front();
                    }
                    q.push_back(v);
                }
                self.sel.details = Some(ProcDetails::read(pid));
            }
            // el proceso terminó: se deja de mostrar
            None => self.select(None),
        }
    }

    pub fn save_cfg(&mut self) {
        self.th = self.cfg.theme();
        if let Err(e) = self.cfg.save() {
            self.msg(format!("No pude guardar la configuración: {e}"), false);
        }
    }

    pub fn set_sort(&mut self, s: SortBy) {
        if self.table.sort == s {
            self.table.reverse = !self.table.reverse;
        } else {
            self.table.sort = s;
            self.table.reverse = false;
        }
        self.rebuild();
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        devtools::fake_clicks(&mut self.dev, raw);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.visuals_for.as_ref() != Some(&self.th) {
            kit::apply_visuals(&ctx, &self.th);
            self.visuals_for = Some(self.th.clone());
        }

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
            .frame(egui::Frame::new().fill(kit::c(self.th.bg)).inner_margin(egui::Margin::symmetric(16, 14)))
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
        self.apply_actions();

        self.toast(&ctx);
        self.keys_window(&ctx);
        self.settings(&ctx);
        self.confirm(&ctx);
        devtools::screenshot(&mut self.dev, &ctx);
    }
}
