//! Inicio: contesta "¿qué me está consumiendo la máquina?".
//! Arriba una franja de estado (CPU, RAM, GPUs); la tabla de procesos es lo
//! principal; el proceso elegido se detalla a la derecha.
//!
//! - `table`: barra de filtro, tabla ordenable y teclado.
//! - `detail`: panel del proceso elegido.

mod detail;
mod table;

use prelude::*;

/// Lo que usan las tres partes de Inicio.
mod prelude {
    pub(crate) use eframe::egui::{self, Align, Color32, CornerRadius, Key, Layout, RichText, Sense, Shape, Stroke, Ui, Vec2};
    pub(crate) use egui_extras::{Column, TableBuilder};
    pub(crate) use rtop_core::{
        config::DEFAULT_COLUMNS,
        fmt::{hms, human},
        Proc, SortBy, State,
    };

    pub(crate) use super::state_name;
    pub(crate) use crate::{
        app::{App, Page},
        kit::{self, body, c, chart, kv, meta, muted, num, paint_meter, section_title, stat_row, strong, w, Grad, Series, Stat, T_BODY},
        pages::{pct_fmt, rate_fmt},
    };
}

pub fn state_name(s: State) -> &'static str {
    match s {
        State::Run => "corriendo",
        State::Sleep => "en espera",
        State::Stop => "detenido",
        State::Zombie => "zombie",
        State::Disk => "esperando al disco",
        State::Other => "desconocido",
    }
}

impl App {
    /// CPU de un proceso según el criterio elegido (total de la máquina o un núcleo).
    pub(crate) fn cpu_of(&self, p: &Proc) -> f32 {
        if self.cfg.cpu_per_core {
            p.cpu
        } else {
            p.cpu / self.mon.stats.cores.len().max(1) as f32
        }
    }

    pub(crate) fn cpu_hint(&self) -> String {
        if self.cfg.cpu_per_core {
            format!("100% = un núcleo entero (tenés {})", self.mon.stats.cores.len())
        } else {
            "100% = todo el procesador".into()
        }
    }

    pub(crate) fn page_home(&mut self, ui: &mut Ui) {
        self.home_strip(ui);
        ui.add_space(4.0);
        self.keyboard(ui);

        let th = self.th.clone();
        let total_w = ui.available_width();
        let h = ui.available_height();
        let gap = 12.0;
        let detail_w = if self.sel.pid.is_some() { (total_w * 0.3).clamp(280.0, 380.0) } else { 0.0 };
        let table_w = total_w - detail_w - if detail_w > 0.0 { gap } else { 0.0 };
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(Vec2::new(table_w, h), Layout::top_down(Align::Min), |ui| {
                egui::Frame::new()
                    .fill(c(th.panel))
                    .corner_radius(CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(16, 12))
                    .show(ui, |ui| {
                        ui.set_width(table_w - 32.0);
                        ui.set_height(h - 24.0);
                        self.toolbar(ui);
                        ui.add_space(6.0);
                        self.table(ui);
                    });
            });
            if detail_w > 0.0 {
                ui.allocate_ui_with_layout(Vec2::new(detail_w, h), Layout::top_down(Align::Min), |ui| {
                    egui::Frame::new()
                        .fill(kit::layer2(&th))
                        .stroke(Stroke::new(1.0, c(th.sel)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(16, 14))
                        .show(ui, |ui| {
                            ui.set_width(detail_w - 32.0);
                            ui.set_height(h - 28.0);
                            egui::ScrollArea::vertical().id_salt("detail").auto_shrink([false, false]).show(ui, |ui| {
                                self.detail(ui);
                            });
                        });
                });
            }
        });
    }

    /// Franja de estado: un valor por recurso, sin gráficos. Click = detalle.
    fn home_strip(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let s = &self.mon.stats;
        let heat = |v: f32| Grad::Heat.at(&th, v / 100.0);
        let mut stats = vec![];
        let mut pages = vec![];

        let mut cpu_d = format!("{}   {:.1} GHz", s.cpu_model, s.cpu_freq as f32 / 1000.0);
        if let Some(t) = s.cpu_temp {
            cpu_d.push_str(&format!("   {t:.0} °C"));
        }
        stats.push(
            Stat::new("CPU", format!("{:.0}%", s.cpu_total), heat(s.cpu_total))
                .detail(cpu_d)
                .meter(s.cpu_total, Grad::Heat)
                .link("Ver detalle del procesador (tecla 2)"),
        );
        pages.push(Page::Cpu);

        stats.push(
            Stat::new("Memoria", format!("{:.0}%", s.mem_pct()), heat(s.mem_pct()))
                .detail(format!("{} de {} en uso", human(s.mem_used), human(s.mem_total)))
                .meter(s.mem_pct(), Grad::Heat)
                .link(format!("Ver detalle de la memoria (tecla {})", s.gpus.len() + 3)),
        );
        pages.push(Page::Mem);

        for (i, g) in s.gpus.iter().enumerate() {
            let label = if g.integrated { format!("{} (integrada)", g.short_name()) } else { g.short_name() };
            let stat = if g.asleep {
                Stat::new(label, "Dormida", c(th.ok)).detail("ahorrando energía, sin uso")
            } else {
                let v = g.busy.unwrap_or(0.0);
                let mut d = format!("VRAM {} de {}", human(g.vram_used), human(g.vram_total));
                if let Some(t) = g.temp {
                    d.push_str(&format!("   {t:.0} °C"));
                }
                if !g.integrated {
                    d.push_str("   despierta");
                }
                Stat::new(label, format!("{v:.0}%"), heat(v)).detail(d).meter(v, Grad::Heat)
            };
            stats.push(stat.link(format!("Ver detalle de {} (tecla {})", g.short_name(), i + 3)));
            pages.push(Page::Gpu(i));
        }

        if let Some(i) = stat_row(ui, &th, "home_strip", &stats) {
            self.go(pages[i]);
        }
    }

}
