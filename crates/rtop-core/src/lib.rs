//! Recolección de datos del sistema + config/temas, compartido por la TUI y la GUI.
//!
//! - `monitor`: junta todo en una foto por intervalo (`Monitor::refresh`).
//! - `model`: los tipos que leen las interfaces (`Stats`, `Proc`...).
//! - `history`: las series para los gráficos.
//! - fuentes: `gpu`, `disk`, `meminfo`, `details`, `netproc`.

pub mod config;
pub mod details;
pub mod disk;
pub mod fmt;
pub mod gpu;
pub mod history;
pub mod meminfo;
pub mod model;
pub mod monitor;
pub mod names;
pub mod netproc;

pub use config::{Config, Rgb, Theme};
pub use details::ProcDetails;
pub use gpu::{Gpu, GpuProc, Vendor};
pub use history::{GpuHist, Hist, History, HIST};
pub use meminfo::MemInfo;
pub use model::{pct, DiskDev, Mount, NetIf, Proc, SortBy, State, Stats, Temp};
pub use monitor::Monitor;
pub use names::short_cpu_name;
