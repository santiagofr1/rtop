//! Widgets propios estilo btop: gráficos con degradé por altura, medidores por
//! segmentos, anillos y tarjetas.

use std::collections::VecDeque;

use eframe::egui::{
    self, epaint::Vertex, Align2, Color32, CornerRadius, FontId, Mesh, Pos2, Rect, RichText, Sense,
    Shape, Stroke, Ui, Vec2,
};
use rtop_core::{Rgb, Theme};

pub fn c(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

pub fn blend(a: Rgb, b: Rgb) -> Color32 {
    lerp(c(a), c(b), 0.5)
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// Paleta de un gráfico/medidor: el color depende de la altura (0..1).
#[derive(Clone, Copy)]
pub enum Grad {
    /// verde -> amarillo -> rojo (uso, temperatura)
    Heat,
    /// de un color apagado al color pleno
    Mono(Rgb),
}

impl Grad {
    pub fn at(self, th: &Theme, t: f32) -> Color32 {
        match self {
            Grad::Heat => {
                if t < 0.5 {
                    lerp(c(th.ok), c(th.warn), t * 2.0)
                } else {
                    lerp(c(th.warn), c(th.hot), (t - 0.5) * 2.0)
                }
            }
            Grad::Mono(col) => lerp(lerp(c(th.panel), c(col), 0.45), c(col), t * 1.6),
        }
    }
}

// ---------- tarjetas y textos ----------

pub fn card<R>(ui: &mut Ui, th: &Theme, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(c(th.panel))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            add(ui)
        })
        .inner
}

pub fn caption(th: &Theme, t: &str) -> RichText {
    RichText::new(t.to_uppercase()).color(c(th.dim)).size(11.0).strong()
}

pub fn big(t: impl Into<String>, col: Color32, size: f32) -> RichText {
    RichText::new(t.into()).size(size).strong().color(col)
}

pub fn dim(th: &Theme, t: impl Into<String>) -> RichText {
    RichText::new(t.into()).color(c(th.dim))
}

/// Título de tarjeta a la izquierda y valor a la derecha.
pub fn header(ui: &mut Ui, th: &Theme, title: &str, value: RichText) {
    ui.horizontal(|ui| {
        ui.label(caption(th, title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(value);
        });
    });
}

/// Grilla clave/valor.
pub fn kv(ui: &mut Ui, th: &Theme, id: &str, rows: &[(&str, String)]) {
    egui::Grid::new(id).num_columns(2).spacing([18.0, 4.0]).show(ui, |ui| {
        for (k, v) in rows {
            ui.label(dim(th, *k));
            ui.label(RichText::new(v).color(c(th.text)));
            ui.end_row();
        }
    });
}

/// Cantidad de columnas que entra en `width` con tarjetas de ~`min_w`,
/// prefiriendo una que reparta `n` items parejo.
pub fn fit_cols(width: f32, min_w: f32, n: usize) -> usize {
    let max = ((width / min_w).floor() as usize).clamp(1, n.max(1));
    (1..=max).rev().find(|c| n % c == 0 && *c * 2 > max).unwrap_or(max)
}

/// Reparte el ancho en `cols` columnas y llama `f(ui, i)` en cada una.
pub fn columns(ui: &mut Ui, n: usize, cols: usize, gap: f32, mut f: impl FnMut(&mut Ui, usize)) {
    let cols = cols.max(1);
    let w = (ui.available_width() - gap * (cols as f32 - 1.0)) / cols as f32;
    let mut i = 0;
    while i < n {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for _ in 0..cols {
                if i >= n {
                    break;
                }
                ui.allocate_ui_with_layout(Vec2::new(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(w);
                    f(ui, i)
                });
                i += 1;
            }
        });
    }
}

// ---------- medidores ----------

/// Medidor por segmentos estilo btop: cada bloque toma el color de su posición.
pub fn meter(ui: &mut Ui, th: &Theme, width: f32, pct: f32, grad: Grad) {
    let h = 10.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
    paint_meter(ui.painter(), th, rect, pct, grad);
}

pub fn paint_meter(p: &egui::Painter, th: &Theme, rect: Rect, pct: f32, grad: Grad) {
    let seg_w = 5.0;
    let gap = 2.0;
    let n = ((rect.width() + gap) / (seg_w + gap)).floor().max(1.0) as usize;
    let filled = ((pct.clamp(0.0, 100.0) / 100.0) * n as f32).round() as usize;
    for j in 0..n {
        let x = rect.left() + j as f32 * (seg_w + gap);
        let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(seg_w, rect.height()));
        let col = if j < filled { grad.at(th, j as f32 / n as f32) } else { c(th.sel) };
        p.rect_filled(r, CornerRadius::same(1), col);
    }
}

/// Anillo con el porcentaje en el medio. Devuelve la respuesta (para clicks).
pub fn ring(ui: &mut Ui, th: &Theme, size: f32, pct: Option<f32>, center: &str, grad: Grad) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    let p = ui.painter();
    let ctr = rect.center();
    let r = size / 2.0 - 7.0;
    let w = 9.0;
    p.circle_stroke(ctr, r, Stroke::new(w, c(th.sel)));
    if let Some(pct) = pct {
        let frac = (pct / 100.0).clamp(0.0, 1.0);
        let steps = (frac * 120.0).ceil() as usize;
        let start = -std::f32::consts::FRAC_PI_2;
        let pt = |a: f32| ctr + Vec2::new(a.cos(), a.sin()) * r;
        for k in 0..steps {
            let a0 = start + std::f32::consts::TAU * frac * k as f32 / steps as f32;
            let a1 = start + std::f32::consts::TAU * frac * (k + 1) as f32 / steps as f32;
            let col = grad.at(th, frac * (k as f32 + 0.5) / steps as f32);
            p.line_segment([pt(a0), pt(a1)], Stroke::new(w, col));
        }
        // puntas redondeadas
        if steps > 0 {
            p.circle_filled(pt(start), w / 2.0, grad.at(th, 0.0));
            p.circle_filled(pt(start + std::f32::consts::TAU * frac), w / 2.0, grad.at(th, frac));
        }
    }
    p.text(ctr, Align2::CENTER_CENTER, center, FontId::proportional(size * 0.2), c(th.text));
    resp
}

// ---------- gráficos ----------

pub struct Series {
    pub name: String,
    pub data: Vec<f32>,
    pub grad: Grad,
}

impl Series {
    pub fn new(name: impl Into<String>, data: Vec<f32>, grad: Grad) -> Self {
        Series { name: name.into(), data, grad }
    }
}

pub fn tail_f32(q: &VecDeque<f32>, n: usize) -> Vec<f32> {
    q.iter().rev().take(n).rev().copied().collect()
}

pub fn tail_f64(q: &VecDeque<f64>, n: usize) -> Vec<f32> {
    q.iter().rev().take(n).rev().map(|&v| v as f32).collect()
}

pub struct ChartOpts {
    pub height: f32,
    /// máximo fijo (ej. 100 para %); None = automático
    pub max: Option<f32>,
    pub fmt: fn(f64) -> String,
    /// muestras visibles
    pub window: usize,
    /// segundos por muestra (para el "hace Xs" del tooltip)
    pub step: f32,
    /// relleno bajo la línea (false = solo líneas, para varias series juntas)
    pub fill: bool,
}

/// Gráfico de área con degradé por altura. Con `down`, la segunda serie se
/// dibuja espejada hacia abajo (como la red en btop).
pub fn chart(ui: &mut Ui, th: &Theme, o: &ChartOpts, up: &[Series], down: Option<&Series>) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), o.height), Sense::hover());
    let p = ui.painter_at(rect.expand(1.0));
    p.rect_filled(rect, CornerRadius::same(6), lerp(c(th.panel), c(th.bg), 0.55));

    let auto_max = |s: &[&Series]| {
        s.iter().flat_map(|s| s.data.iter().copied()).fold(0.0f32, f32::max).max(1.0) * 1.15
    };
    let up_refs: Vec<&Series> = up.iter().collect();
    let (mid, up_max, down_max) = match down {
        Some(d) => {
            let m = o.max.unwrap_or_else(|| auto_max(&up_refs).max(auto_max(&[d])));
            (rect.center().y, m, m)
        }
        None => (rect.bottom(), o.max.unwrap_or_else(|| auto_max(&up_refs)), 0.0),
    };

    // grilla
    let grid = c(th.dim).gamma_multiply(0.18);
    for k in 1..4 {
        let y = rect.top() + rect.height() * k as f32 / 4.0;
        p.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0, grid));
    }

    let dx = rect.width() / (o.window.max(2) - 1) as f32;
    let draw = |s: &Series, max: f32, dir: f32, area_h: f32| {
        let n = s.data.len();
        if n < 2 {
            return;
        }
        let x = |i: usize| rect.right() - (n - 1 - i) as f32 * dx;
        let frac = |v: f32| (v / max).clamp(0.0, 1.0);
        let y = |v: f32| mid - dir * frac(v) * area_h;
        let mut mesh = Mesh::default();
        let base_col = s.grad.at(th, 0.0).gamma_multiply(0.12);
        for i in 0..n {
            let t = frac(s.data[i]);
            let idx = mesh.vertices.len() as u32;
            mesh.vertices.push(Vertex { pos: Pos2::new(x(i), mid), uv: egui::epaint::WHITE_UV, color: base_col });
            mesh.vertices.push(Vertex {
                pos: Pos2::new(x(i), y(s.data[i])),
                uv: egui::epaint::WHITE_UV,
                color: s.grad.at(th, t).gamma_multiply(0.75),
            });
            if i > 0 {
                mesh.add_triangle(idx - 2, idx - 1, idx + 1);
                mesh.add_triangle(idx - 2, idx + 1, idx);
            }
        }
        if o.fill {
            p.add(Shape::mesh(mesh));
        }
        for i in 1..n {
            let a = Pos2::new(x(i - 1), y(s.data[i - 1]));
            let b = Pos2::new(x(i), y(s.data[i]));
            p.line_segment([a, b], Stroke::new(1.6, s.grad.at(th, frac(s.data[i]))));
        }
    };

    let up_h = mid - rect.top() - 2.0;
    for s in up {
        draw(s, up_max, 1.0, up_h);
    }
    if let Some(d) = down {
        draw(d, down_max, -1.0, rect.bottom() - mid - 2.0);
        p.line_segment([Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)], Stroke::new(1.0, grid));
    }

    // escala
    let lbl = |pos: Pos2, a: Align2, t: String| {
        p.text(pos, a, t, FontId::proportional(10.0), c(th.dim));
    };
    if o.height >= 60.0 {
        lbl(rect.left_top() + Vec2::new(6.0, 4.0), Align2::LEFT_TOP, (o.fmt)(up_max as f64));
    }
    if down.is_some() && o.height >= 60.0 {
        lbl(rect.left_bottom() + Vec2::new(6.0, -4.0), Align2::LEFT_BOTTOM, (o.fmt)(down_max as f64));
    }

    // tooltip
    if let Some(h) = resp.hover_pos() {
        let back = ((rect.right() - h.x) / dx).round() as usize;
        let xh = rect.right() - back as f32 * dx;
        p.line_segment([Pos2::new(xh, rect.top()), Pos2::new(xh, rect.bottom())], Stroke::new(1.0, c(th.dim)));
        let mut lines = vec![];
        for s in up.iter().chain(down) {
            if let Some(v) = s.data.len().checked_sub(back + 1).and_then(|i| s.data.get(i)) {
                lines.push(format!("{}: {}", s.name, (o.fmt)(*v as f64)));
            }
        }
        if !lines.is_empty() {
            lines.push(format!("hace {:.0}s", back as f32 * o.step));
            resp.on_hover_text(lines.join("\n"));
        }
    }
}

/// Mini gráfico sin escala ni tooltip (para la barra lateral y listas).
pub fn paint_spark(p: &egui::Painter, th: &Theme, rect: Rect, data: &[f32], max: Option<f32>, grad: Grad) {
    let n = data.len();
    if n < 2 {
        return;
    }
    let max = max.unwrap_or_else(|| data.iter().copied().fold(0.0, f32::max).max(1.0) * 1.1);
    let dx = rect.width() / (n - 1) as f32;
    let mut mesh = Mesh::default();
    for (i, &v) in data.iter().enumerate() {
        let t = (v / max).clamp(0.0, 1.0);
        let x = rect.left() + i as f32 * dx;
        let idx = mesh.vertices.len() as u32;
        mesh.vertices.push(Vertex {
            pos: Pos2::new(x, rect.bottom()),
            uv: egui::epaint::WHITE_UV,
            color: grad.at(th, 0.0).gamma_multiply(0.1),
        });
        mesh.vertices.push(Vertex {
            pos: Pos2::new(x, rect.bottom() - t * rect.height()),
            uv: egui::epaint::WHITE_UV,
            color: grad.at(th, t).gamma_multiply(0.8),
        });
        if i > 0 {
            mesh.add_triangle(idx - 2, idx - 1, idx + 1);
            mesh.add_triangle(idx - 2, idx + 1, idx);
        }
    }
    p.add(Shape::mesh(mesh));
}
