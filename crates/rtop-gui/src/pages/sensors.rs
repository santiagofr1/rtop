//! Temperaturas: una tarjeta por sensor; click amplía.

use super::prelude::*;

impl App {
    pub(crate) fn page_sensors(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;
        struct T {
            label: String,
            value: f32,
            max: Option<f32>,
            crit: Option<f32>,
            data: Vec<f32>,
        }
        let mut items: Vec<T> = s
            .temps
            .iter()
            .map(|t| T {
                label: t.label.clone(),
                value: t.value,
                max: t.max,
                crit: t.crit,
                data: m.hist.temps.get(&t.label).map(|h| tail_f32(h, WINDOW)).unwrap_or_default(),
            })
            .collect();
        for g in &s.gpus {
            if g.vendor == Vendor::Nvidia && !g.asleep {
                items.push(T {
                    label: g.short_name(),
                    value: g.temp.unwrap_or(0.0),
                    max: None,
                    crit: None,
                    data: m.hist.gpu_detail.get(&g.id).map(|h| tail_f32(&h.temp, WINDOW)).unwrap_or_default(),
                });
            }
        }
        if items.is_empty() {
            surface(ui, th, |ui| {
                ui.label(meta(th, "No se encontraron sensores de temperatura."));
            });
            return;
        }
        // orden fijo (el del sistema): si se reordenaran por valor, las
        // tarjetas saltarían de lugar a cada rato
        let hottest = items.iter().max_by(|a, b| a.value.total_cmp(&b.value)).unwrap();
        stat_row(
            ui,
            th,
            "temp_stats",
            &[
                Stat::new("La más alta", format!("{:.0} °C", hottest.value), Grad::Heat.at(th, hottest.value / 100.0)).detail(hottest.label.clone()),
                Stat::new(
                    "Procesador",
                    s.cpu_temp.map(|t| format!("{t:.0} °C")).unwrap_or("sin dato".into()),
                    s.cpu_temp.map(|t| Grad::Heat.at(th, t / 100.0)).unwrap_or(muted(th)),
                ),
                Stat::new("Sensores", items.len().to_string(), c(th.text)).detail("CPU, GPU, disco, memoria, placa"),
            ],
        );

        let step = self.cfg.refresh_ms() as f32 / 1000.0;
        let span_of = |n: usize| {
            let secs = n as f32 * step;
            if secs < 90.0 { format!("{secs:.0} s") } else { format!("{:.0} min", secs / 60.0) }
        };
        let rgb = |col: Color32| rtop_core::Rgb(col.r(), col.g(), col.b());

        // sensor ampliado: gráfico grande con tooltip y los números del período
        let open = self.sensors.open.clone();
        if let Some(t) = open.as_ref().and_then(|l| items.iter().find(|t| &t.label == l)) {
            let col = Grad::Heat.at(th, t.value / 100.0);
            let peak = t.data.iter().copied().fold(t.value, f32::max);
            let low = t.data.iter().copied().fold(t.value, f32::min);
            let avg = if t.data.is_empty() { t.value } else { t.data.iter().sum::<f32>() / t.data.len() as f32 };
            surface(ui, th, |ui| {
                ui.horizontal(|ui| {
                    ui.label(kit::section_title(th, &t.label));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.button("Cerrar").clicked() {
                            self.act(Action::OpenSensor(None));
                        }
                        ui.add_space(12.0);
                        ui.label(strong(format!("{:.0} °C", t.value), col, 24.0));
                    });
                });
                ui.add_space(4.0);
                let mut o = self.opts(280.0, None, temp_fmt);
                o.zero = false;
                chart(ui, th, &o, &[Series::new(t.label.clone(), t.data.clone(), Grad::Mono(rgb(col)))], None);
                ui.add_space(6.0);
                let mut cells = vec![
                    Stat::new("Mínima", format!("{low:.0} °C"), Grad::Heat.at(th, low / 100.0)),
                    Stat::new("Promedio", format!("{avg:.1} °C"), c(th.text)),
                    Stat::new("Máxima", format!("{peak:.0} °C"), Grad::Heat.at(th, peak / 100.0)).detail(format!("en los últimos {}", span_of(t.data.len()))),
                ];
                if let Some(cr) = t.crit {
                    cells.push(Stat::new("Crítica", format!("{cr:.0} °C"), c(th.hot)).detail(format!("faltan {:.0} °C", (cr - t.value).max(0.0))));
                } else if let Some(mx) = t.max {
                    cells.push(Stat::new("Alta según el sensor", format!("{mx:.0} °C"), c(th.warn)));
                }
                kit::stat_row(ui, th, "sensor_big", &cells);
            });
        }

        // una tarjeta por sensor, cada una con su propia escala: así se ven
        // las subidas de pocos grados y los picos. Click = ampliar.
        let cols = if ui.available_width() > 1100.0 { 3 } else { 2 };
        columns(ui, items.len(), cols, 12.0, |ui, i| {
            let t = &items[i];
            let col = Grad::Heat.at(th, t.value / 100.0);
            let peak = t.data.iter().copied().fold(t.value, f32::max);
            let low = t.data.iter().copied().fold(t.value, f32::min);
            let is_open = open.as_deref() == Some(t.label.as_str());
            let card = ui
                .scope(|ui| {
                    surface(ui, th, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(strong(&t.label, c(th.text), 15.0));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(strong(format!("{:.0} °C", t.value), col, 22.0));
                            });
                        });
                        ui.add_space(6.0);
                        let mut o = self.opts(90.0, None, temp_fmt);
                        o.zero = false;
                        chart(ui, th, &o, &[Series::new(t.label.clone(), t.data.clone(), Grad::Mono(rgb(col)))], None);
                        ui.add_space(6.0);
                        let mut line = format!("entre {low:.0} y {peak:.0} °C en los últimos {}", span_of(t.data.len()));
                        if let Some(cr) = t.crit {
                            line.push_str(&format!(", crítica {cr:.0} °C"));
                        } else if let Some(mx) = t.max {
                            line.push_str(&format!(", alta {mx:.0} °C"));
                        }
                        ui.label(meta(th, line));
                    })
                })
                .response;
            let r = ui.interact(card.rect, egui::Id::new(("sensor_card", i)), Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if is_open || r.hovered() {
                let stroke = if is_open { c(th.accent) } else { c(th.sel) };
                ui.painter().rect_stroke(card.rect, CornerRadius::same(10), egui::Stroke::new(1.5, stroke), egui::StrokeKind::Inside);
            }
            if r.clicked() {
                self.act(Action::OpenSensor(if is_open { None } else { Some(t.label.clone()) }));
            }
        });
    }
}
