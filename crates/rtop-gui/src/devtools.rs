//! Ganchos para probar la interfaz sin tocarla (solo con `--features devtools`):
//!
//! - `RTOP_PAGE=cpu|mem|gpu0|gpu1|net|disk|sensors|battery`: página inicial
//! - `RTOP_SELECT=<pid>`: proceso elegido
//! - `RTOP_SENSOR=<nombre>`: sensor ampliado en Temperaturas
//! - `RTOP_SHOT=/ruta.ppm` (+ `RTOP_SHOT_DELAY` segundos): captura y cierra
//! - `RTOP_CLICKS="6:1320,64;9:1300,110"`: clicks simulados (segundo:x,y)
//!
//! Sin la feature, todo esto no existe en el binario.

use eframe::egui;

use crate::app::App;

#[derive(Default)]
pub struct DevState {
    #[cfg(feature = "devtools")]
    started: Option<std::time::Instant>,
    #[cfg(feature = "devtools")]
    shot_requested: bool,
    #[cfg(feature = "devtools")]
    clicks_done: u64,
}

#[cfg(not(feature = "devtools"))]
pub fn setup(_: &mut App) {}
#[cfg(not(feature = "devtools"))]
pub fn fake_clicks(_: &mut DevState, _: &mut egui::RawInput) {}
#[cfg(not(feature = "devtools"))]
pub fn screenshot(_: &mut DevState, _: &egui::Context) {}

#[cfg(feature = "devtools")]
pub fn setup(a: &mut App) {
    use crate::app::Page;
    let env = |k: &str| std::env::var(k).ok();
    a.dev.started = Some(std::time::Instant::now());
    a.page = match env("RTOP_PAGE").as_deref() {
        Some("cpu") => Page::Cpu,
        Some("mem") => Page::Mem,
        Some("gpu0") => Page::Gpu(0),
        Some("gpu1") => Page::Gpu(1),
        Some("net") => Page::Net,
        Some("disk") => Page::Disk,
        Some("sensors") => Page::Sensors,
        Some("battery") => Page::Battery,
        _ => Page::Home,
    };
    a.sensors.open = env("RTOP_SENSOR");
    if let Some(pid) = env("RTOP_SELECT").and_then(|s| s.parse().ok()) {
        a.select(Some(pid));
    }
}

#[cfg(feature = "devtools")]
fn elapsed(d: &DevState) -> f32 {
    d.started.map(|t| t.elapsed().as_secs_f32()).unwrap_or(0.0)
}

#[cfg(feature = "devtools")]
pub fn fake_clicks(d: &mut DevState, raw: &mut egui::RawInput) {
    let Ok(spec) = std::env::var("RTOP_CLICKS") else { return };
    let t = elapsed(d);
    for (i, c) in spec.split(';').enumerate() {
        let Some((ts, xy)) = c.split_once(':') else { continue };
        let Some((x, y)) = xy.split_once(',') else { continue };
        let (Ok(ts), Ok(x), Ok(y)) = (ts.parse::<f32>(), x.parse::<f32>(), y.parse::<f32>()) else { continue };
        let bit = 1u64 << i;
        if t >= ts && d.clicks_done & bit == 0 {
            d.clicks_done |= bit;
            let pos = egui::pos2(x, y);
            raw.events.push(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                raw.events.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
            }
        }
    }
}

#[cfg(feature = "devtools")]
pub fn screenshot(d: &mut DevState, ctx: &egui::Context) {
    let Some(path) = std::env::var_os("RTOP_SHOT") else { return };
    let delay: f32 = std::env::var("RTOP_SHOT_DELAY").ok().and_then(|s| s.parse().ok()).unwrap_or(4.0);
    if !d.shot_requested && elapsed(d) > delay {
        d.shot_requested = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(200));
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
