//! Batería: carga, a qué ritmo se carga o se descarga, el promedio a batería,
//! quién se lleva la energía, y el desgaste respecto de la de fábrica.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use rtop_core::{BatStatus, Battery};

use super::prelude::*;

/// peso del uso de GPU frente al de CPU al repartir el consumo: un punto de
/// GPU gasta bastante más que un punto de un núcleo
const GPU_WEIGHT: f32 = 2.0;

/// color del nivel de carga: poca carga = caliente
pub(crate) fn level_color(th: &rtop_core::Theme, pct: f32) -> Color32 {
    Grad::Heat.at(th, 1.0 - pct / 100.0)
}

fn hours_fmt(h: f32) -> String {
    let m = (h * 60.0).round() as u32;
    if m < 60 { format!("{m} min") } else { format!("{} h {:02} min", m / 60, m % 60) }
}

fn wh(v: Option<f32>) -> String {
    v.map(|v| format!("{v:.1} Wh")).unwrap_or("sin dato".into())
}

/// los valores de la tabla son un promedio de más o menos estos segundos
const SMOOTH_SECS: f32 = 15.0;
/// y el orden se rehace cada tanto, para que las filas no salten
const RESORT: Duration = Duration::from_secs(5);

/// Un programa (procesos agrupados por nombre), con su uso suavizado.
#[derive(Clone)]
struct Eater {
    pid: u32,
    name: String,
    count: usize,
    cpu: f32,
    gpu: f32,
}

impl Eater {
    fn weight(&self) -> f32 {
        self.cpu + self.gpu * GPU_WEIGHT
    }
}

/// Tabla de quién consume: se actualiza en cada refresco (esté abierta la
/// página o no, así al entrar ya tiene promedio).
#[derive(Default)]
pub struct BatState {
    eaters: HashMap<String, Eater>,
    /// nombres en el orden en que se muestran
    order: Vec<String>,
    sorted_at: Option<Instant>,
    /// W de descarga suavizados (None = no está a batería)
    watts: Option<f32>,
}

impl App {
    pub(crate) fn page_battery(&self, ui: &mut Ui) {
        let th = &self.th;
        let pw = &self.mon.stats.power;
        let Some(b) = pw.total() else {
            surface(ui, th, |ui| {
                ui.label(meta(th, "Este equipo no tiene batería."));
            });
            return;
        };
        let level = level_color(th, b.capacity);
        // W -> % de la batería por hora
        let per_hour = |w: f32| b.energy_full.filter(|f| *f > 0.0).map(|f| w * 100.0 / f);

        ui.label(meta(th, bat_subtitle(&b)));

        // ---------- números ----------
        let status_col = match b.status {
            BatStatus::Charging => c(th.accent),
            BatStatus::Full => c(th.ok),
            BatStatus::Discharging if b.capacity < 15.0 => c(th.hot),
            _ => c(th.text),
        };
        let now = match (b.status, b.power) {
            (BatStatus::Discharging, Some(p)) => Stat::new("Descarga ahora", format!("{p:.1} W"), c(th.text))
                .detail(per_hour(p).map(|r| format!("−{r:.1} % por hora")).unwrap_or_default()),
            (BatStatus::Charging, Some(p)) => Stat::new("Carga ahora", format!("{p:.1} W"), c(th.accent))
                .detail(per_hour(p).map(|r| format!("+{r:.1} % por hora")).unwrap_or_default()),
            _ => Stat::new("Ahora", "—", muted(th)).detail("Sin flujo de energía"),
        };
        let mins_on_bat = self.mon.hist.drain_n as f32 * self.cfg.refresh_ms() as f32 / 60_000.0;
        let avg = match self.mon.hist.drain_avg() {
            Some(a) => Stat::new("Descarga promedio", format!("{a:.1} W"), c(th.text)).detail(format!(
                "{}{} con batería",
                per_hour(a).map(|r| format!("−{r:.1} % por hora · ")).unwrap_or_default(),
                hours_fmt(mins_on_bat / 60.0)
            )),
            None => Stat::new("Descarga promedio", "—", muted(th)).detail("Sin datos con batería"),
        };
        let time = match (b.status, b.hours_left()) {
            (BatStatus::Discharging, Some(t)) => Stat::new("Autonomía", hours_fmt(t), c(th.text)).detail("Al consumo actual"),
            (BatStatus::Charging, Some(t)) => Stat::new("Tiempo de carga", hours_fmt(t), c(th.text))
                .detail(b.end_threshold.map(|e| format!("Hasta {e} % (umbral)")).unwrap_or("A la potencia actual".into())),
            _ => Stat::new("Tiempo", "—", muted(th)).detail(match pw.on_ac() {
                Some(true) => "Conectada a la red",
                _ => "Calculando",
            }),
        };
        let mut cells = vec![
            Stat::new("Carga", format!("{:.0}%", b.capacity), level)
                .detail(format!("{} de {}", wh(b.energy_now), wh(b.energy_full)))
                .meter(b.capacity, Grad::Mono(rgb(level))),
            Stat::new("Estado", b.status.label(), status_col).detail(match pw.on_ac() {
                Some(true) => "Conectada a la red",
                Some(false) => "Con batería",
                None => "",
            }),
            now,
            avg,
            time,
        ];
        if let Some(hp) = b.health() {
            let col = Grad::Heat.at(th, ((100.0 - hp) / 40.0).clamp(0.0, 1.0));
            let det = b.cycles.map(|n| format!("{n} ciclos")).unwrap_or("de la capacidad de fábrica".into());
            cells.push(Stat::new("Salud", format!("{hp:.0}%"), col).detail(det));
        }
        stat_row(ui, th, "bat_stats", &cells);

        // ---------- quién consume ----------
        let draining = b.status == BatStatus::Discharging;
        let total_w = self.bat.watts.filter(|_| draining);
        surface(ui, th, |ui| {
            let hint = if draining {
                "Estimación según uso de CPU y GPU, promedio de 15 s"
            } else {
                "Con el cargador conectado se muestra solo la proporción"
            };
            section(ui, th, "Consumo por proceso", Some(meta(th, hint)));
            let eaters = self.eaters(12);
            let rows = eaters
                .iter()
                .map(|&(e, share)| {
                    let name = if e.count > 1 { format!("{}  ×{}", e.name, e.count) } else { e.name.clone() };
                    let watts = match total_w {
                        Some(t) => Col::Meter(share * 100.0, Grad::Heat, format!("{:.1} W", share * t)),
                        None => Col::Meter(share * 100.0, Grad::Heat, String::new()),
                    };
                    let gpu = if e.gpu >= 0.05 {
                        Col::Text(format!("{:.1}%", e.gpu), Grad::Heat.at(th, e.gpu / 100.0))
                    } else {
                        Col::Text(String::new(), muted(th))
                    };
                    (e.pid, name, vec![watts, Col::Text(format!("{:.0}%", share * 100.0), c(th.text)), Col::Text(format!("{:.1}%", e.cpu), muted(th)), gpu])
                })
                .collect();
            self.proc_table(ui, "bat_top", &[("Consumo", w::METER), ("Proporción", w::SMALL), ("CPU", w::GPU), ("GPU", w::GPU)], rows);
            ui.label(meta(
                th,
                "Los valores incluyen la proporción de pantalla y demás componentes, que no puede medirse por separado sin permisos de administrador.",
            ));
        });

        // ---------- desgaste ----------
        if let (Some(full), Some(design)) = (b.energy_full, b.energy_design) {
            surface(ui, th, |ui| {
                section(ui, th, "Capacidad", Some(meta(th, "Capacidad actual frente a la de diseño")));
                let w = ui.available_width();
                let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 20.0), Sense::hover());
                let p = ui.painter();
                let max = design.max(full).max(0.1);
                p.rect_filled(rect, CornerRadius::same(4), c(th.sel));
                let fw = rect.width() * full / max;
                p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(fw, rect.height())), CornerRadius::same(4), c(th.accent).gamma_multiply(0.45));
                if let Some(now) = b.energy_now {
                    let nw = rect.width() * now / max;
                    p.rect_filled(Rect::from_min_size(rect.min, Vec2::new(nw, rect.height())), CornerRadius::same(4), level);
                }
                legend(
                    ui,
                    th,
                    &[
                        (format!("Energía actual: {}", wh(b.energy_now)), level),
                        (format!("Capacidad actual: {full:.1} Wh"), c(th.accent).gamma_multiply(0.45)),
                        (format!("Capacidad de diseño: {design:.1} Wh"), c(th.sel)),
                    ],
                );
                let lost = (design - full).max(0.0);
                if lost > 0.05 {
                    ui.label(meta(th, format!("Desgaste: {lost:.1} Wh ({:.0} %) respecto de la capacidad de diseño.", lost * 100.0 / design)));
                } else {
                    ui.label(meta(th, "Sin desgaste medible respecto de la capacidad de diseño."));
                }
            });
        }

        // ---------- detalle ----------
        surface(ui, th, |ui| {
            section(ui, th, "Detalle", None);
            let mut rows: Vec<(&str, String)> = vec![];
            let txt = |s: &str| if s.is_empty() { "No disponible".to_string() } else { s.to_string() };
            rows.push(("Fabricante", txt(&b.manufacturer)));
            rows.push(("Modelo", txt(&b.model)));
            rows.push(("Tecnología", txt(&b.technology)));
            rows.push(("Nivel informado", txt(&b.level)));
            rows.push(("Energía actual", wh(b.energy_now)));
            rows.push(("Capacidad actual", wh(b.energy_full)));
            rows.push(("Capacidad de diseño", wh(b.energy_design)));
            if let Some(n) = b.cycles {
                rows.push(("Ciclos de carga", n.to_string()));
            }
            if let Some(v) = b.voltage {
                let d = b.voltage_design.map(|d| format!(" (nominal {d:.1} V)")).unwrap_or_default();
                rows.push(("Voltaje", format!("{v:.2} V{d}")));
            }
            if let (Some(p), Some(v)) = (b.power, b.voltage.filter(|v| *v > 0.0)) {
                rows.push(("Corriente", format!("{:.2} A", p / v)));
            }
            if let Some(a) = b.alarm {
                rows.push(("Aviso de batería baja", format!("{a:.1} Wh ({:.0}%)", b.energy_full.map_or(0.0, |f| a * 100.0 / f))));
            }
            if b.start_threshold.is_some() || b.end_threshold.is_some() {
                let s = b.start_threshold.map(|v| format!("{v}%")).unwrap_or("—".into());
                let e = b.end_threshold.map(|v| format!("{v}%")).unwrap_or("—".into());
                rows.push(("Umbral de carga", format!("Inicio {s}, corte {e}")));
            }
            for a in &pw.adapters {
                let kind = match a.kind.as_str() {
                    "Mains" => "Cargador",
                    "USB" | "USB_C" | "USB_PD" => "USB-C",
                    k => k,
                };
                rows.push((kind, format!("{} ({})", if a.online { "Conectado" } else { "Desconectado" }, a.name)));
            }
            if let Some(p) = &pw.profile {
                rows.push(("Perfil de energía", profile_name(p).to_string()));
            }
            if !pw.profiles.is_empty() {
                rows.push(("Perfiles disponibles", pw.profiles.iter().map(|x| profile_name(x)).collect::<Vec<_>>().join(", ")));
            }
            if pw.batteries.len() > 1 {
                for x in &pw.batteries {
                    rows.push(("Batería", format!("{}: {:.0}%, {}", x.name, x.capacity, x.status.label().to_lowercase())));
                }
            }
            kv(ui, th, "bat_kv", &rows);
        });
    }

    /// Junta el uso de CPU y GPU por programa y lo suaviza (promedio
    /// exponencial). Lo llama `refresh` en cada muestra.
    pub(crate) fn track_eaters(&mut self) {
        let a = 1.0 - (-(self.cfg.refresh_ms() as f32 / 1000.0) / SMOOTH_SECS).exp();
        let mut now: HashMap<&str, Eater> = HashMap::new();
        let mut best: HashMap<&str, f32> = HashMap::new();
        for p in &self.mon.procs {
            let gpu = p.gpu.unwrap_or(0.0);
            let e = now.entry(&p.name).or_insert(Eater { pid: p.pid, name: p.name.clone(), count: 0, cpu: 0.0, gpu: 0.0 });
            e.count += 1;
            e.cpu += p.cpu;
            e.gpu += gpu;
            // el PID que se muestra es el del proceso que más usa
            let b = best.entry(&p.name).or_insert(-1.0);
            if p.cpu + gpu > *b {
                *b = p.cpu + gpu;
                e.pid = p.pid;
            }
        }
        let st = &mut self.bat;
        // los que ya no están van bajando hasta desaparecer
        for (name, e) in st.eaters.iter_mut() {
            if !now.contains_key(name.as_str()) {
                e.cpu -= a * e.cpu;
                e.gpu -= a * e.gpu;
            }
        }
        for (name, cur) in now {
            match st.eaters.get_mut(name) {
                Some(e) => {
                    e.cpu += a * (cur.cpu - e.cpu);
                    e.gpu += a * (cur.gpu - e.gpu);
                    e.count = cur.count;
                    e.pid = cur.pid;
                }
                // los nuevos arrancan en su valor, no en cero
                None if cur.weight() >= 0.05 => {
                    st.eaters.insert(name.to_string(), cur);
                }
                None => {}
            }
        }
        st.eaters.retain(|_, e| e.weight() >= 0.02);

        let bat = self.mon.stats.power.total();
        st.watts = match bat.as_ref().filter(|b| b.status == BatStatus::Discharging).and_then(|b| b.power) {
            Some(w) => Some(st.watts.map_or(w, |o| o + a * (w - o))),
            None => None,
        };

        if st.sorted_at.is_none_or(|t| t.elapsed() >= RESORT) {
            let mut v: Vec<&Eater> = st.eaters.values().collect();
            v.sort_by(|x, y| y.weight().total_cmp(&x.weight()));
            st.order = v.into_iter().map(|e| e.name.clone()).collect();
            st.sorted_at = Some(Instant::now());
        } else {
            // entre reordenamientos: los que se fueron salen, los nuevos van al final
            st.order.retain(|n| st.eaters.contains_key(n));
            let mut new: Vec<&Eater> = st.eaters.values().filter(|e| !st.order.contains(&e.name)).collect();
            new.sort_by(|x, y| y.weight().total_cmp(&x.weight()));
            let new: Vec<String> = new.into_iter().map(|e| e.name.clone()).collect();
            st.order.extend(new);
        }
    }

    /// Los `n` primeros en el orden fijado, con su parte del total (0..1).
    fn eaters(&self, n: usize) -> Vec<(&Eater, f32)> {
        let st = &self.bat;
        let total: f32 = st.eaters.values().map(Eater::weight).sum::<f32>().max(0.001);
        st.order.iter().filter_map(|name| st.eaters.get(name)).take(n).map(|e| (e, e.weight() / total)).collect()
    }
}

fn bat_subtitle(b: &Battery) -> String {
    let mut v = vec![];
    if !b.manufacturer.is_empty() {
        v.push(b.manufacturer.clone());
    }
    if !b.technology.is_empty() {
        v.push(b.technology.clone());
    }
    if let Some(d) = b.energy_design {
        v.push(format!("{d:.0} Wh"));
    }
    v.push(b.name.clone());
    v.join("  ·  ")
}

fn rgb(col: Color32) -> rtop_core::Rgb {
    rtop_core::Rgb(col.r(), col.g(), col.b())
}

fn profile_name(p: &str) -> &str {
    match p {
        "low-power" => "Ahorro de energía",
        "quiet" => "Silencioso",
        "cool" => "Baja temperatura",
        "balanced" => "Equilibrado",
        "balanced-performance" => "Equilibrado (rendimiento)",
        "performance" => "Rendimiento",
        o => o,
    }
}
