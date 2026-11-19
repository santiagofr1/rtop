//! Estado de la TUI: datos, tabla de procesos, modo (normal, filtro, ayuda,
//! confirmar) y config.

use std::time::{Duration, Instant};

use ratatui::widgets::TableState;
use rtop_core::{Config, Monitor, Proc, SortBy, Theme};

#[derive(PartialEq)]
pub enum Mode {
    Normal,
    Filter,
    ConfirmKill(u32, String),
    Help,
}

pub struct App {
    pub mon: Monitor,
    pub cfg: Config,
    pub th: Theme,
    pub view: Vec<usize>,
    pub table: TableState,
    pub sort: SortBy,
    pub reverse: bool,
    pub filter: String,
    pub mode: Mode,
    pub paused: bool,
    pub status_msg: Option<(String, Instant)>,
}

impl App {
    pub fn new() -> Self {
        let cfg = Config::load();
        let mut app = App {
            mon: Monitor::new(),
            th: cfg.theme(),
            cfg,
            view: Vec::new(),
            table: TableState::default().with_selected(0),
            sort: SortBy::Cpu,
            reverse: false,
            filter: String::new(),
            mode: Mode::Normal,
            paused: false,
            status_msg: None,
        };
        app.rebuild();
        app
    }

    pub fn tick(&self) -> Duration {
        Duration::from_millis(self.cfg.refresh_ms())
    }

    /// Vuelve a filtrar y ordenar, manteniendo elegido el mismo proceso.
    pub fn rebuild(&mut self) {
        let sel_pid = self.selected().map(|p| p.pid);
        self.view = self.mon.view(&self.filter, self.sort, self.reverse);
        let idx = sel_pid
            .and_then(|pid| self.view.iter().position(|&i| self.mon.procs[i].pid == pid))
            .unwrap_or_else(|| self.table.selected().unwrap_or(0));
        self.table.select(Some(idx.min(self.view.len().saturating_sub(1))));
    }

    pub fn refresh(&mut self) {
        self.mon.refresh();
        self.rebuild();
    }

    pub fn selected(&self) -> Option<&Proc> {
        self.table
            .selected()
            .and_then(|i| self.view.get(i))
            .map(|&i| &self.mon.procs[i])
    }

    pub fn move_sel(&mut self, delta: isize) {
        if self.view.is_empty() {
            return;
        }
        let cur = self.table.selected().unwrap_or(0) as isize;
        let max = self.view.len() as isize - 1;
        self.table.select(Some((cur + delta).clamp(0, max) as usize));
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.status_msg = Some((msg.into(), Instant::now()));
    }

    pub fn save_cfg(&mut self) {
        self.th = self.cfg.theme();
        if let Err(e) = self.cfg.save() {
            self.flash(format!("no pude guardar la config: {e}"));
        }
    }

    pub fn set_sort(&mut self, s: SortBy) {
        if self.sort == s {
            self.reverse = !self.reverse;
        } else {
            self.sort = s;
            self.reverse = false;
        }
        self.rebuild();
    }

    pub fn kill(&mut self, pid: u32, name: &str, force: bool) {
        let s = if force { "SIGKILL" } else { "SIGTERM" };
        if self.mon.kill(pid, force) {
            self.flash(format!("{s} -> {name} ({pid})"));
        } else {
            self.flash(format!("no pude matar {name} ({pid}) - permisos?"));
        }
    }
}
