//! Teclado de la TUI.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use rtop_core::SortBy;

use crate::app::{App, Mode};

impl App {
    /// devuelve true si hay que salir
    pub fn on_key(&mut self, k: KeyEvent) -> bool {
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            return true;
        }
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Help => {}
            Mode::Filter => match k.code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.rebuild();
                }
                KeyCode::Enter => {}
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.rebuild();
                    self.mode = Mode::Filter;
                }
                KeyCode::Char(ch) => {
                    self.filter.push(ch);
                    self.rebuild();
                    self.mode = Mode::Filter;
                }
                _ => self.mode = Mode::Filter,
            },
            Mode::ConfirmKill(pid, name) => match k.code {
                KeyCode::Char('y') | KeyCode::Char('s') | KeyCode::Enter => self.kill(pid, &name, false),
                KeyCode::Char('9') => self.kill(pid, &name, true),
                _ => self.flash("cancelado"),
            },
            Mode::Normal => match k.code {
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::F(10) => return true,
                KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
                KeyCode::PageDown => self.move_sel(20),
                KeyCode::PageUp => self.move_sel(-20),
                KeyCode::Home | KeyCode::Char('g') => self.table.select(Some(0)),
                KeyCode::End | KeyCode::Char('G') => {
                    self.table.select(Some(self.view.len().saturating_sub(1)))
                }
                KeyCode::Char('/') | KeyCode::F(3) => self.mode = Mode::Filter,
                KeyCode::Char('s') | KeyCode::F(6) | KeyCode::Tab => {
                    self.sort = self.sort.next();
                    self.rebuild();
                }
                KeyCode::Char('c') => self.set_sort(SortBy::Cpu),
                KeyCode::Char('m') => self.set_sort(SortBy::Mem),
                KeyCode::Char('n') => self.set_sort(SortBy::Name),
                KeyCode::Char('i') => self.set_sort(SortBy::Pid),
                KeyCode::Char('t') => self.set_sort(SortBy::Time),
                KeyCode::Char('r') => {
                    self.reverse = !self.reverse;
                    self.rebuild();
                }
                KeyCode::Char('p') | KeyCode::Char(' ') => {
                    self.paused = !self.paused;
                    self.flash(if self.paused { "pausado" } else { "corriendo" });
                }
                KeyCode::Char('l') => {
                    self.cfg.show_cmd = !self.cfg.show_cmd;
                    self.save_cfg();
                }
                KeyCode::Char('v') => {
                    self.cfg.show_graphs = !self.cfg.show_graphs;
                    self.save_cfg();
                }
                KeyCode::Char('T') => {
                    self.cfg.next_theme();
                    self.save_cfg();
                    self.flash(format!("tema: {}", self.cfg.theme));
                }
                KeyCode::Char('+') => {
                    self.cfg.refresh_ms = (self.cfg.refresh_ms() + 250).min(10_000);
                    self.save_cfg();
                    self.flash(format!("refresco: {} ms", self.cfg.refresh_ms));
                }
                KeyCode::Char('-') => {
                    self.cfg.refresh_ms = self.cfg.refresh_ms().saturating_sub(250).max(250);
                    self.save_cfg();
                    self.flash(format!("refresco: {} ms", self.cfg.refresh_ms));
                }
                KeyCode::Char('x') | KeyCode::F(9) | KeyCode::Delete => {
                    if let Some(p) = self.selected() {
                        self.mode = Mode::ConfirmKill(p.pid, p.name.clone());
                    }
                }
                KeyCode::Char('?') | KeyCode::F(1) | KeyCode::Char('h') => self.mode = Mode::Help,
                _ => {}
            },
        }
        false
    }
}
