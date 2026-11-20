//! Barra lateral, encabezado de las páginas y aviso de pausa.

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Layout, Sense, Stroke, Ui, Vec2};
use rtop_core::fmt::human;

use crate::{
    app::{App, Page},
    kit::{self, body, c, meta, muted, Grad},
};

impl App {
    /// Orden de la barra lateral; también define los atajos 1..9.
    pub(crate) fn pages(&self) -> Vec<Page> {
        let mut v = vec![Page::Home, Page::Cpu];
        v.extend((0..self.mon.stats.gpus.len()).map(Page::Gpu));
        v.extend([Page::Mem, Page::Disk, Page::Net, Page::Sensors]);
        if !self.mon.stats.power.batteries.is_empty() {
            v.push(Page::Battery);
        }
        v
    }

    pub(crate) fn page_title(&self, p: Page) -> String {
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
            Page::Battery => "Batería".into(),
        }
    }

    // ---------- barra lateral ----------

    pub(crate) fn nav(&mut self, ui: &mut Ui) {
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
                Page::Battery => match s.power.total() {
                    Some(b) => {
                        let arrow = match b.status {
                            rtop_core::BatStatus::Charging => "↑ ",
                            rtop_core::BatStatus::Discharging => "↓ ",
                            _ => "",
                        };
                        (format!("{arrow}{:.0}%", b.capacity), crate::pages::level_color(&th, b.capacity))
                    }
                    None => (String::new(), muted(&th)),
                },
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
                    self.dialogs.settings = !self.dialogs.settings;
                }
                if ui.button("Atajos").on_hover_text("F1").clicked() {
                    self.dialogs.keys = !self.dialogs.keys;
                }
            });
        });
    }

    /// Encabezado de las páginas de detalle, con el botón para volver.
    pub(crate) fn page_bar(&mut self, ui: &mut Ui) {
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

    pub(crate) fn pause_banner(&mut self, ui: &mut Ui) {
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
}
