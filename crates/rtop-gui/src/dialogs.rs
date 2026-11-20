//! Ventanas: aviso abajo, atajos, ajustes y confirmación para terminar.

use std::time::Duration;

use eframe::egui::{self, Align, Align2, CornerRadius, Layout, RichText, Stroke, Vec2};
use rtop_core::{config::THEMES, Config, Rgb};

use crate::{
    app::App,
    kit::{self, body, c, meta},
};

impl App {
    pub(crate) fn toast(&mut self, ctx: &egui::Context) {
        let Some((m, ok, t)) = self.dialogs.flash.clone() else { return };
        if t.elapsed() > Duration::from_secs(5) {
            self.dialogs.flash = None;
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

    pub(crate) fn keys_window(&mut self, ctx: &egui::Context) {
        let mut open = self.dialogs.keys;
        let th = self.th.clone();
        egui::Window::new("Atajos de teclado").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            let rows = [
                ("1 a 9", "ir a cada sección (en el orden de la barra)"),
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
        self.dialogs.keys = open;
    }

    pub(crate) fn settings(&mut self, ctx: &egui::Context) {
        let mut open = self.dialogs.settings;
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
        self.dialogs.settings = open;
        if changed {
            self.save_cfg();
        }
    }

    pub(crate) fn confirm(&mut self, ctx: &egui::Context) {
        let Some((pid, name)) = self.dialogs.confirm_kill.clone() else { return };
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
            self.dialogs.confirm_kill = None;
            if let Some(force) = a {
                if self.mon.kill(pid, force) {
                    self.msg(format!("Le pedí a {name} que termine."), true);
                } else {
                    self.msg(format!("No pude terminar {name}: es de otro usuario o del sistema. Probá con sudo."), false);
                }
            }
        }
    }
}
