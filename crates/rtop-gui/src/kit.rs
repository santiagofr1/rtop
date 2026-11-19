//! Vocabulario visual de rtop: tipografía, superficies, filas de estado,
//! medidores por segmentos y gráficos con degradé por altura.
//!
//! Reglas: una sola familia (Inter, la del sistema) en 4 tamaños; monoespaciada
//! solo para números; el color de carga (verde -> rojo) solo para carga; el
//! acento solo para selección y acciones.

use std::collections::VecDeque;

use eframe::egui::{
    self, epaint::Vertex, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Mesh, Pos2, Rect,
    RichText, Sense, Shape, Stroke, Ui, Vec2,
};
use rtop_core::{Rgb, Theme};

// ---------- tipografía ----------

/// metadatos, ejes, ayudas
pub const T_META: f32 = 12.0;
/// cuerpo, tablas, navegación
pub const T_BODY: f32 = 14.0;
/// títulos de sección
pub const T_SECTION: f32 = 17.0;
/// títulos de página y valores principales
pub const T_TITLE: f32 = 24.0;

fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Carga Inter (la fuente del sistema en Zorin) y JetBrains Mono para números.
/// Si no están, quedan las de egui.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let home = std::env::var("HOME").unwrap_or_default();
    let load = |paths: &[String]| paths.iter().find_map(|p| std::fs::read(p).ok());
    let inter = |w: &str| {
        load(&[
            format!("/usr/share/fonts/truetype/inter-zorin-os/Inter-{w}.ttf"),
            format!("/usr/share/fonts/truetype/inter/Inter-{w}.ttf"),
            format!("{home}/.local/share/fonts/Inter-{w}.ttf"),
        ])
    };
    let regular = inter("Regular");
    let semi = inter("SemiBold").or_else(|| inter("Medium"));
    let mono = load(&[
        format!("{home}/.local/share/fonts/JetBrainsMonoNerdFontMono-Regular.ttf"),
        "/usr/share/fonts/truetype/jetbrains-mono/JetBrainsMono-Regular.ttf".into(),
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf".into(),
    ]);

    if let Some(b) = regular {
        fonts.font_data.insert("inter".into(), FontData::from_owned(b).into());
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".into());
    }
    let mut semi_list = vec![];
    if let Some(b) = semi {
        fonts.font_data.insert("inter-semibold".into(), FontData::from_owned(b).into());
        semi_list.push("inter-semibold".to_string());
    }
    // la familia semibold cae a la proporcional para los glifos que falten
    semi_list.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(semibold(), semi_list);
    if let Some(b) = mono {
        fonts.font_data.insert("mono".into(), FontData::from_owned(b).into());
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "mono".into());
    }
    ctx.set_fonts(fonts);
}

pub fn c(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb.0, rgb.1, rgb.2)
}

fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

pub fn blend(a: Rgb, b: Rgb) -> Color32 {
    lerp(c(a), c(b), 0.5)
}

/// Texto secundario: se deriva del texto hacia el fondo, sin bajar de ~5:1.
pub fn muted(th: &Theme) -> Color32 {
    lerp(c(th.text), c(th.panel), 0.4)
}

/// Segunda capa neutra (barra lateral, panel de detalle).
pub fn layer2(th: &Theme) -> Color32 {
    lerp(c(th.panel), c(th.bg), 0.45)
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

// ---------- texto ----------

pub fn title(th: &Theme, t: impl Into<String>) -> RichText {
    RichText::new(t.into()).family(semibold()).size(T_TITLE).color(c(th.text))
}

pub fn section_title(th: &Theme, t: impl Into<String>) -> RichText {
    RichText::new(t.into()).family(semibold()).size(T_SECTION).color(c(th.text))
}

pub fn strong(t: impl Into<String>, col: Color32, size: f32) -> RichText {
    RichText::new(t.into()).family(semibold()).size(size).color(col)
}

pub fn body(th: &Theme, t: impl Into<String>) -> RichText {
    RichText::new(t.into()).size(T_BODY).color(c(th.text))
}

pub fn meta(th: &Theme, t: impl Into<String>) -> RichText {
    RichText::new(t.into()).size(T_META).color(muted(th))
}

pub fn num(t: impl Into<String>, col: Color32) -> RichText {
    RichText::new(t.into()).monospace().size(13.0).color(col)
}

// ---------- superficies ----------

/// Una superficie por grupo de contenido. No se anidan.
pub fn surface<R>(ui: &mut Ui, th: &Theme, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(c(th.panel))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(18, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            add(ui)
        })
        .inner
}

/// Título de sección con un dato a la derecha.
pub fn section(ui: &mut Ui, th: &Theme, t: &str, right: Option<RichText>) {
    ui.horizontal(|ui| {
        ui.label(section_title(th, t));
        if let Some(r) = right {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(r);
            });
        }
    });
}

/// Grilla clave/valor.
pub fn kv(ui: &mut Ui, th: &Theme, id: &str, rows: &[(&str, String)]) {
    egui::Grid::new(id).num_columns(2).spacing([20.0, 6.0]).show(ui, |ui| {
        for (k, v) in rows {
            ui.label(meta(th, *k).size(13.0));
            ui.add(egui::Label::new(body(th, v.clone()).size(13.5)).wrap());
            ui.end_row();
        }
    });
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

// ---------- fila de estado ----------

pub struct Stat {
    pub label: String,
    pub value: String,
    pub value_color: Color32,
    pub detail: String,
    /// medidor bajo el valor (0..100)
    pub meter: Option<(f32, Grad)>,
    /// texto del tooltip; si hay, la celda es clickeable
    pub link: Option<String>,
}

impl Stat {
    pub fn new(label: impl Into<String>, value: impl Into<String>, value_color: Color32) -> Stat {
        Stat { label: label.into(), value: value.into(), value_color, detail: String::new(), meter: None, link: None }
    }
    pub fn detail(mut self, d: impl Into<String>) -> Stat {
        self.detail = d.into();
        self
    }
    pub fn meter(mut self, v: f32, g: Grad) -> Stat {
        self.meter = Some((v, g));
        self
    }
    pub fn link(mut self, tip: impl Into<String>) -> Stat {
        self.link = Some(tip.into());
        self
    }
}

/// Una superficie con varios valores lado a lado, separados por líneas finas.
/// Devuelve el índice de la celda clickeada.
pub fn stat_row(ui: &mut Ui, th: &Theme, id: &str, stats: &[Stat]) -> Option<usize> {
    let mut clicked = None;
    let n = stats.len().max(1);
    egui::Frame::new()
        .fill(c(th.panel))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(6, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let w = ui.available_width() / n as f32;
            let h = 104.0;
            let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
            for (i, s) in stats.iter().enumerate() {
                let cell = Rect::from_min_size(row.min + Vec2::new(i as f32 * w, 0.0), Vec2::new(w, h));
                let resp = ui.interact(cell, egui::Id::new((id, i)), if s.link.is_some() { Sense::click() } else { Sense::hover() });
                let p = ui.painter();
                if s.link.is_some() && resp.hovered() {
                    p.rect_filled(cell.shrink(2.0), CornerRadius::same(8), c(th.sel).gamma_multiply(0.55));
                }
                if i > 0 {
                    p.line_segment(
                        [cell.left_top() + Vec2::new(0.0, 14.0), cell.left_bottom() - Vec2::new(0.0, 14.0)],
                        Stroke::new(1.0, c(th.sel)),
                    );
                }
                let x = cell.left() + 16.0;
                p.text(Pos2::new(x, cell.top() + 14.0), Align2::LEFT_TOP, &s.label, FontId::proportional(13.0), muted(th));
                p.text(
                    Pos2::new(x, cell.top() + 34.0),
                    Align2::LEFT_TOP,
                    &s.value,
                    FontId::new(T_TITLE, semibold()),
                    s.value_color,
                );
                p.text(Pos2::new(x, cell.top() + 68.0), Align2::LEFT_TOP, &s.detail, FontId::proportional(T_META), muted(th));
                if let Some((v, g)) = s.meter {
                    let r = Rect::from_min_size(Pos2::new(x, cell.bottom() - 13.0), Vec2::new(w - 32.0, 5.0));
                    paint_meter(p, th, r, v, g);
                }
                if let Some(tip) = &s.link {
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip);
                    if resp.clicked() {
                        clicked = Some(i);
                    }
                }
            }
        });
    clicked
}

// ---------- medidores ----------

pub fn meter(ui: &mut Ui, th: &Theme, width: f32, pct: f32, grad: Grad) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
    paint_meter(ui.painter(), th, rect, pct, grad);
}

/// Medidor por segmentos: cada bloque toma el color de su posición.
pub fn paint_meter(p: &egui::Painter, th: &Theme, rect: Rect, pct: f32, grad: Grad) {
    let seg_w = 4.0;
    let gap = 2.0;
    let n = ((rect.width() + gap) / (seg_w + gap)).floor().max(1.0) as usize;
    let filled = ((pct.clamp(0.0, 100.0) / 100.0) * n as f32).ceil() as usize;
    for j in 0..n {
        let x = rect.left() + j as f32 * (seg_w + gap);
        let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(seg_w, rect.height()));
        let col = if j < filled { grad.at(th, j as f32 / n as f32) } else { c(th.sel) };
        p.rect_filled(r, CornerRadius::same(1), col);
    }
}

/// Leyenda: muestra de color dibujada + texto.
pub fn legend(ui: &mut Ui, th: &Theme, items: &[(String, Color32)]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        for (t, col) in items {
            let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(2), *col);
            ui.label(meta(th, t.clone()).color(c(th.text)));
            ui.add_space(10.0);
        }
    });
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
    /// tope de la escala (100 para %). La escala se ajusta a los datos sin
    /// pasarse de este tope. None = sin tope.
    pub max: Option<f32>,
    pub fmt: fn(f64) -> String,
    /// muestras como máximo en pantalla
    pub window: usize,
    /// segundos por muestra (para el "hace Xs" del tooltip)
    pub step: f32,
    /// relleno bajo la línea (false = solo líneas, para varias series juntas)
    pub fill: bool,
}

/// Redondea hacia arriba a 1, 2, 2.5 o 5 por potencia de 10.
fn nice_ceil(v: f32) -> f32 {
    if v <= 0.0 {
        return 1.0;
    }
    let p = 10f32.powf(v.log10().floor());
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if m * p >= v {
            return m * p;
        }
    }
    10.0 * p
}

/// Gráfico de área con degradé por altura. Con `down`, la segunda serie se
/// dibuja espejada hacia abajo (como la red en btop).
pub fn chart(ui: &mut Ui, th: &Theme, o: &ChartOpts, up: &[Series], down: Option<&Series>) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), o.height), Sense::hover());
    let p = ui.painter_at(rect.expand(1.0));
    p.rect_filled(rect, CornerRadius::same(6), lerp(c(th.panel), c(th.bg), 0.5));

    let data_max = up
        .iter()
        .chain(down)
        .flat_map(|s| s.data.iter().copied())
        .fold(0.0f32, f32::max);
    let scale = match o.max {
        Some(cap) => nice_ceil((data_max * 1.2).max(cap * 0.1)).min(cap),
        None => nice_ceil((data_max * 1.2).max(1.0)),
    };
    let mid = if down.is_some() { rect.center().y } else { rect.bottom() };

    // grilla
    let grid = c(th.sel).gamma_multiply(0.8);
    for k in 1..4 {
        let y = rect.top() + rect.height() * k as f32 / 4.0;
        p.line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], Stroke::new(1.0, grid));
    }

    // la ventana crece con los datos: el gráfico se llena desde el arranque
    let n_max = up.iter().chain(down).map(|s| s.data.len()).max().unwrap_or(0);
    let window = n_max.clamp(30, o.window.max(30));
    let dx = rect.width() / (window - 1) as f32;
    let draw = |s: &Series, dir: f32, area_h: f32| {
        let n = s.data.len();
        if n < 2 {
            return;
        }
        let x = |i: usize| rect.right() - (n - 1 - i) as f32 * dx;
        let frac = |v: f32| (v / scale).clamp(0.0, 1.0);
        let y = |v: f32| mid - dir * frac(v) * area_h;
        if o.fill {
            let mut mesh = Mesh::default();
            let base_col = s.grad.at(th, 0.0).gamma_multiply(0.1);
            for i in 0..n {
                let t = frac(s.data[i]);
                let idx = mesh.vertices.len() as u32;
                mesh.vertices.push(Vertex { pos: Pos2::new(x(i), mid), uv: egui::epaint::WHITE_UV, color: base_col });
                mesh.vertices.push(Vertex {
                    pos: Pos2::new(x(i), y(s.data[i])),
                    uv: egui::epaint::WHITE_UV,
                    color: s.grad.at(th, t).gamma_multiply(0.7),
                });
                if i > 0 {
                    mesh.add_triangle(idx - 2, idx - 1, idx + 1);
                    mesh.add_triangle(idx - 2, idx + 1, idx);
                }
            }
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
        draw(s, 1.0, up_h);
    }
    if let Some(d) = down {
        draw(d, -1.0, rect.bottom() - mid - 2.0);
        p.line_segment([Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)], Stroke::new(1.0, grid));
    }

    // escala: solo el máximo, en el rincón
    if o.height >= 60.0 {
        let f = FontId::proportional(11.0);
        p.text(rect.left_top() + Vec2::new(8.0, 6.0), Align2::LEFT_TOP, (o.fmt)(scale as f64), f.clone(), muted(th));
        if down.is_some() {
            p.text(rect.left_bottom() + Vec2::new(8.0, -6.0), Align2::LEFT_BOTTOM, (o.fmt)(scale as f64), f, muted(th));
        }
    }

    // tooltip
    if let Some(h) = resp.hover_pos() {
        let back = ((rect.right() - h.x) / dx).round().max(0.0) as usize;
        let xh = rect.right() - back as f32 * dx;
        p.line_segment([Pos2::new(xh, rect.top()), Pos2::new(xh, rect.bottom())], Stroke::new(1.0, muted(th)));
        let mut lines = vec![];
        for s in up.iter().chain(down) {
            if let Some(v) = s.data.len().checked_sub(back + 1).and_then(|i| s.data.get(i)) {
                lines.push(format!("{}: {}", s.name, (o.fmt)(*v as f64)));
            }
        }
        if !lines.is_empty() {
            lines.push(format!("hace {:.0} s", back as f32 * o.step));
            resp.on_hover_text(lines.join("\n"));
        }
    }
}
