//! Páginas de detalle. Estructura común: fila de estado, un gráfico principal
//! grande, y después los grupos de datos. Leen de `App` sin mutarlo; las
//! acciones (navegar, terminar un proceso) se piden con `App::act`.

mod battery;
mod cpu;
mod disk;
mod gpu;
mod mem;
mod net;
mod sensors;
mod table;

pub(crate) use battery::{level_color, BatState};
pub use table::Col;

use rtop_core::fmt::human;

use crate::{app::WINDOW, kit::ChartOpts, App};

/// Lo que usan todas las páginas.
mod prelude {
    pub(crate) use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Sense, Ui, Vec2};
    pub(crate) use rtop_core::{fmt::human, pct, Proc, SortBy, Vendor};

    pub(crate) use super::{mhz_fmt, pct_fmt, rate_fmt, temp_fmt, watt_fmt, Col};
    pub(crate) use crate::{
        app::{Action, WINDOW},
        kit::{
            self, body, c, chart, columns, kv, legend, meta, meter, muted, num, section, stat_row, strong, surface, tail_f32, tail_f64,
            w, ChartOpts, Grad, Series, Stat,
        },
        App,
    };
}


pub fn pct_fmt(v: f64) -> String {
    format!("{v:.0}%")
}
pub fn rate_fmt(v: f64) -> String {
    if v <= 0.0 { "0".into() } else { format!("{}/s", human(v as u64)) }
}
pub fn temp_fmt(v: f64) -> String {
    format!("{v:.0} °C")
}
pub fn watt_fmt(v: f64) -> String {
    format!("{v:.1} W")
}
pub fn mhz_fmt(v: f64) -> String {
    if v >= 1000.0 { format!("{:.2} GHz", v / 1000.0) } else { format!("{v:.0} MHz") }
}

impl App {
    pub(crate) fn opts(&self, height: f32, max: Option<f32>, fmt: fn(f64) -> String) -> ChartOpts {
        ChartOpts { height, max, fmt, window: WINDOW, step: self.cfg.refresh_ms() as f32 / 1000.0, fill: true, floor: 0.0, zero: true, binary: false }
    }
}
