//! rtop-gui: monitor del sistema con ventana propia.

mod app;
mod devtools;
mod dialogs;
mod home;
mod input;
mod kit;
mod nav;
mod pages;

use eframe::egui;

pub use app::WINDOW;
use app::App;

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
