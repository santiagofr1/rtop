//! Páginas de detalle. Estructura común: fila de estado, un gráfico principal
//! grande, y después los grupos de datos. Leen de `App` sin mutarlo; las
//! acciones (matar, navegar) se piden por `App::kill_req` / `App::goto`.

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Sense, Ui, Vec2};
use rtop_core::{fmt::human, pct, Proc, SortBy, Vendor};

use crate::{
    kit::{
        self,
        body, c, chart, columns, kv, legend, meta, meter, muted, num, section, stat_row, strong, surface, tail_f32, tail_f64, ChartOpts,
        Grad, Series, Stat,
    },
    App,
};

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

/// Columnas extra de las tablas de procesos de las páginas.
pub enum Col {
    /// número alineado a la derecha
    Text(String, Color32),
    /// número con medidor
    Meter(f32, Grad, String),
    /// texto libre, a la izquierda y apagado
    Note(String),
}

impl App {
    pub(crate) fn opts(&self, height: f32, max: Option<f32>, fmt: fn(f64) -> String) -> ChartOpts {
        ChartOpts { height, max, fmt, window: crate::WINDOW, step: self.cfg.refresh_ms() as f32 / 1000.0, fill: true, floor: 0.0, zero: true, binary: false }
    }

    /// Tabla de procesos de las páginas, con el mismo formato que la de Inicio:
    /// PID, nombre con sus argumentos, usuario y las columnas propias de cada
    /// página (`heads`: título y ancho). Click = verlo en Inicio, click
    /// derecho = terminar.
    pub(crate) fn proc_table(&self, ui: &mut Ui, id: &str, heads: &[(&str, f32)], rows: Vec<(u32, String, Vec<Col>)>) {
        let th = &self.th;
        let mut_c = muted(th);
        if rows.is_empty() {
            ui.label(meta(th, "Ninguno por ahora."));
            return;
        }
        // los títulos se alinean como su contenido
        let left: Vec<bool> = rows[0].2.iter().map(|c| matches!(c, Col::Note(_))).collect();
        // si no entra, primero se va el usuario y después el PID
        let extra: f32 = heads.iter().map(|h| h.1 + 10.0).sum();
        let avail = ui.available_width();
        let show_user = avail - extra > 160.0 + 64.0 + 96.0 + 30.0;
        let show_pid = avail - extra > 160.0 + 64.0 + 20.0;
        let mut tb = egui_extras::TableBuilder::new(ui)
            .id_salt(id)
            .striped(false)
            .vscroll(false)
            .sense(Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center));
        if show_pid {
            tb = tb.column(egui_extras::Column::exact(56.0));
        }
        tb = tb.column(egui_extras::Column::remainder().at_least(150.0).clip(true));
        if show_user {
            tb = tb.column(egui_extras::Column::exact(90.0).clip(true));
        }
        for h in heads {
            tb = tb.column(egui_extras::Column::exact(h.1).clip(true));
        }
        let head = |ui: &mut Ui, t: &str, right: bool| {
            let l = if right { Layout::right_to_left(Align::Center) } else { Layout::left_to_right(Align::Center) };
            ui.with_layout(l, |ui| ui.label(meta(th, t)));
        };
        let right = |ui: &mut Ui, t: egui::RichText| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(t));
        };
        tb.header(24.0, |mut h| {
            if show_pid {
                h.col(|ui| head(ui, "PID", true));
            }
            h.col(|ui| head(ui, "Proceso", false));
            if show_user {
                h.col(|ui| head(ui, "Usuario", false));
            }
            for (k, (t, _)) in heads.iter().enumerate() {
                h.col(|ui| head(ui, t, !left.get(k).copied().unwrap_or(false)));
            }
        })
        .body(|mut tbody| {
            for (pid, name, cols) in rows {
                let proc = self.mon.proc(pid);
                tbody.row(26.0, |mut row| {
                    if show_pid {
                        row.col(|ui| right(ui, num(pid.to_string(), mut_c)));
                    }
                    row.col(|ui| {
                        ui.label(body(th, &name));
                        let args = proc.and_then(|p| p.cmd.split_once(' ')).map(|(_, a)| a.trim()).unwrap_or("");
                        if !args.is_empty() {
                            ui.add(egui::Label::new(egui::RichText::new(args).size(12.5).color(mut_c)).truncate());
                        }
                    });
                    if show_user {
                        row.col(|ui| {
                            let u = proc.map(|p| p.user.as_str()).unwrap_or("");
                            ui.label(egui::RichText::new(u).size(13.0).color(if u == "root" { c(th.warn) } else { mut_c }));
                        });
                    }
                    for col in cols {
                        row.col(|ui| match col {
                            Col::Text(t, color) => right(ui, num(t, color)),
                            Col::Note(t) => {
                                ui.add(egui::Label::new(egui::RichText::new(t).size(13.0).color(mut_c)).truncate());
                            }
                            Col::Meter(v, g, t) => {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(num(t, if v < 0.05 { mut_c } else { c(th.text) }));
                                    let (r, _) = ui.allocate_exact_size(Vec2::new(36.0, 6.0), Sense::hover());
                                    if v >= 0.5 {
                                        crate::kit::paint_meter(ui.painter(), th, r, v.min(100.0), g);
                                    }
                                });
                            }
                        });
                    }
                    let r = row.response().on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Click para verlo en Inicio");
                    if r.clicked() {
                        self.goto.set(Some(crate::Page::Home));
                        self.select_req.set(Some(pid));
                    }
                    r.context_menu(|ui| {
                        if ui.button("Ver en Inicio").clicked() {
                            self.goto.set(Some(crate::Page::Home));
                            self.select_req.set(Some(pid));
                            ui.close();
                        }
                        if ui.button("Terminar proceso…").clicked() {
                            *self.kill_req.borrow_mut() = Some((pid, name.clone()));
                            ui.close();
                        }
                    });
                });
            }
        });
    }

    /// Columnas comunes a todas las tablas: CPU, memoria y tiempo activo.
    fn base_cols(&self, p: &Proc) -> (Col, Col, Col) {
        let th = &self.th;
        let v = self.cpu_of(p);
        let scale = if self.cfg.cpu_per_core { 1.0 } else { 4.0 };
        (
            Col::Meter((v * scale).min(100.0), Grad::Heat, format!("{v:.1}%")),
            Col::Meter(p.mem_pct * 4.0, Grad::Mono(th.mem), human(p.mem)),
            Col::Text(rtop_core::fmt::hms(p.time), muted(th)),
        )
    }

    fn cpu_rows(&self, procs: &[&Proc]) -> Vec<(u32, String, Vec<Col>)> {
        let th = &self.th;
        procs
            .iter()
            .map(|p| {
                let (cpu, mem, time) = self.base_cols(p);
                let gpu = match p.gpu {
                    Some(g) if g >= 0.05 => Col::Text(format!("{g:.1}%"), Grad::Heat.at(th, g / 100.0)),
                    _ => Col::Text(String::new(), muted(th)),
                };
                let state = Col::Note(crate::home::state_name(p.state).to_string());
                (p.pid, p.name.clone(), vec![cpu, mem, gpu, state, time])
            })
            .collect()
    }

    fn mem_rows(&self, procs: &[&Proc]) -> Vec<(u32, String, Vec<Col>)> {
        let th = &self.th;
        procs
            .iter()
            .map(|p| {
                let (cpu, mem, time) = self.base_cols(p);
                let vram = Col::Text(if p.vram > 0 { human(p.vram) } else { String::new() }, c(th.mem));
                (p.pid, p.name.clone(), vec![mem, Col::Text(format!("{:.1}%", p.mem_pct), muted(th)), vram, cpu, time])
            })
            .collect()
    }

    // ================= CPU =================

    pub(crate) fn page_cpu(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;
        let heat = |v: f32| Grad::Heat.at(th, v / 100.0);
        ui.label(meta(th, &s.cpu_brand));

        stat_row(
            ui,
            th,
            "cpu_stats",
            &[
                Stat::new("Uso", format!("{:.0}%", s.cpu_total), heat(s.cpu_total)).meter(s.cpu_total, Grad::Heat),
                Stat::new("Frecuencia", mhz_fmt(s.cpu_freq as f64), c(th.text)).detail("promedio de los núcleos"),
                Stat::new(
                    "Temperatura",
                    s.cpu_temp.map(|t| format!("{t:.0} °C")).unwrap_or("sin dato".into()),
                    s.cpu_temp.map(heat).unwrap_or(muted(th)),
                ),
                Stat::new("Carga", format!("{:.2}", s.load.0), c(th.text))
                    .detail(format!("en cola; 5 min {:.2}, 15 min {:.2}", s.load.1, s.load.2)),
                Stat::new("Procesos", s.tasks.to_string(), c(th.text)).detail(format!("{} corriendo ahora", s.running)),
            ],
        );

        surface(ui, th, |ui| {
            section(ui, th, "Uso total", None);
            chart(ui, th, &self.opts(170.0, Some(100.0), pct_fmt), &[Series::new("CPU", tail_f32(&m.hist.cpu, crate::WINDOW), Grad::Heat)], None);
        });

        surface(ui, th, |ui| {
            section(ui, th, "Núcleos", Some(meta(th, format!("{} hilos", s.cores.len()))));
            self.core_bars(ui);
        });

        columns(ui, 2, 2, 12.0, |ui, i| {
            surface(ui, th, |ui| {
                if i == 0 {
                    section(ui, th, "Frecuencia", Some(strong(mhz_fmt(s.cpu_freq as f64), c(th.accent), 15.0)));
                    chart(
                        ui,
                        th,
                        &self.opts(110.0, None, mhz_fmt),
                        &[Series::new("frecuencia", tail_f32(&m.hist.freq, crate::WINDOW), Grad::Mono(th.accent))],
                        None,
                    );
                } else {
                    let t = s.cpu_temp.unwrap_or(0.0);
                    section(ui, th, "Temperatura", Some(strong(format!("{t:.0} °C"), heat(t), 15.0)));
                    let data = s
                        .temps
                        .iter()
                        .find(|x| Some(x.value) == s.cpu_temp)
                        .and_then(|x| m.hist.temps.get(&x.label))
                        .map(|h| tail_f32(h, crate::WINDOW))
                        .unwrap_or_default();
                    chart(ui, th, &self.opts(110.0, Some(100.0), temp_fmt), &[Series::new("temperatura", data, Grad::Heat)], None);
                }
            });
        });

        surface(ui, th, |ui| {
            section(ui, th, "Los que más CPU usan", Some(meta(th, self.cpu_hint())));
            let top = m.top(SortBy::Cpu, 10);
            self.proc_table(
                ui,
                "cpu_top",
                &[("CPU", 116.0), ("Memoria", 124.0), ("GPU", 70.0), ("Estado", 120.0), ("Tiempo activo", 104.0)],
                self.cpu_rows(&top),
            );
        });
    }

    /// Una barra vertical por núcleo, con % adentro y frecuencia abajo.
    fn core_bars(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let n = s.cores.len().max(1);
        let per_row = n.min(16);
        let rows = n.div_ceil(per_row);
        let w = ui.available_width();
        let gap = 8.0;
        let bar_w = (w - gap * (per_row as f32 - 1.0)) / per_row as f32;
        let bar_h = 96.0;
        let row_h = bar_h + 40.0;
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, rows as f32 * row_h), Sense::hover());
        let p = ui.painter_at(rect);
        let mut hovered = None;
        for (i, &u) in s.cores.iter().enumerate() {
            let (r, col) = (i / per_row, i % per_row);
            let x = rect.left() + col as f32 * (bar_w + gap);
            let y = rect.top() + r as f32 * row_h;
            let bar = Rect::from_min_size(Pos2::new(x, y), Vec2::new(bar_w, bar_h));
            p.rect_filled(bar, CornerRadius::same(6), c(th.sel).gamma_multiply(0.6));
            let frac = (u / 100.0).clamp(0.0, 1.0);
            if frac > 0.005 {
                let mut f = bar;
                f.set_top(bar.bottom() - (bar_h * frac).max(3.0));
                p.rect_filled(f, CornerRadius::same(6), Grad::Heat.at(th, frac).gamma_multiply(0.85));
            }
            p.text(bar.center_top() + Vec2::new(0.0, 8.0), Align2::CENTER_TOP, format!("{u:.0}%"), FontId::proportional(14.0), c(th.text));
            p.text(
                Pos2::new(bar.center().x, bar.bottom() + 6.0),
                Align2::CENTER_TOP,
                format!("Núcleo {i}"),
                FontId::proportional(12.0),
                c(th.text),
            );
            p.text(
                Pos2::new(bar.center().x, bar.bottom() + 22.0),
                Align2::CENTER_TOP,
                mhz_fmt(*s.core_freq.get(i).unwrap_or(&0) as f64),
                FontId::proportional(11.0),
                muted(th),
            );
            if resp.hover_pos().is_some_and(|h| bar.contains(h)) {
                hovered = Some((i, u));
            }
        }
        if let Some((i, u)) = hovered {
            resp.on_hover_text(format!("Núcleo {i}: {u:.1}%"));
        }
    }

    // ================= MEMORIA =================

    pub(crate) fn page_mem(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let mi = &s.meminfo;
        let m = &self.mon;
        let used_pct = pct(mi.used(), mi.total);

        stat_row(
            ui,
            th,
            "mem_stats",
            &[
                Stat::new("En uso", human(mi.used()), Grad::Heat.at(th, used_pct / 100.0))
                    .detail(format!("{used_pct:.0}% de {}", human(mi.total)))
                    .meter(used_pct, Grad::Heat),
                Stat::new("Disponible", human(mi.available), c(th.text)).detail("lo que pueden pedir los programas"),
                Stat::new("Caché", human(mi.cache()), c(th.accent)).detail("archivos recientes; se libera solo"),
                Stat::new("Swap", human(s.swap_used), Grad::Heat.at(th, s.swap_pct() / 100.0))
                    .detail(format!("de {}", human(s.swap_total)))
                    .meter(s.swap_pct(), Grad::Heat),
            ],
        );

        surface(ui, th, |ui| {
            section(ui, th, "Cómo se reparte", None);
            let w = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 20.0), Sense::hover());
            let p = ui.painter();
            let parts = [
                (mi.used(), c(th.mem), "En uso por programas"),
                (mi.cache(), c(th.accent), "Caché y buffers"),
                (mi.free, c(th.sel), "Libre"),
            ];
            // las partes se solapan un poco en /proc/meminfo: se reparten sobre su suma
            let total = parts.iter().map(|p| p.0).sum::<u64>().max(1) as f32;
            let mut x = rect.left();
            for (v, col, _) in parts {
                let ww = rect.width() * v as f32 / total;
                p.rect_filled(Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(ww, rect.height())), CornerRadius::same(4), col);
                x += ww;
            }
            legend(ui, th, &parts.iter().map(|(v, col, n)| (format!("{n}: {}", human(*v)), *col)).collect::<Vec<_>>());
            ui.add_space(6.0);
            chart(
                ui,
                th,
                &self.opts(150.0, Some(100.0), pct_fmt),
                &[
                    Series::new(
                        "en uso + caché",
                        m.hist.mem.iter().zip(&m.hist.cache).rev().take(crate::WINDOW).rev().map(|(a, b)| (a + b).min(100.0)).collect(),
                        Grad::Mono(th.accent),
                    ),
                    Series::new("en uso", tail_f32(&m.hist.mem, crate::WINDOW), Grad::Mono(th.mem)),
                ],
                None,
            );
        });

        surface(ui, th, |ui| {
            section(ui, th, "Los que más memoria usan", None);
            let top = m.top(SortBy::Mem, 12);
            self.proc_table(
                ui,
                "mem_top",
                &[("Memoria", 124.0), ("% del total", 90.0), ("VRAM", 80.0), ("CPU", 116.0), ("Tiempo activo", 104.0)],
                self.mem_rows(&top),
            );
        });

        surface(ui, th, |ui| {
            section(ui, th, "Detalle", None);
            kv(
                ui,
                th,
                "mem_kv",
                &[
                    ("Total instalada", human(mi.total)),
                    ("En uso", format!("{} ({used_pct:.0}%)", human(mi.used()))),
                    ("Disponible", human(mi.available)),
                    ("Caché de archivos", human(mi.cached)),
                    ("Buffers", human(mi.buffers)),
                    ("Compartida entre procesos", human(mi.shmem)),
                    ("Pendiente de escribir a disco", human(mi.dirty)),
                    ("Swap", format!("{} de {}", human(s.swap_used), human(s.swap_total))),
                ],
            );
        });
    }

    // ================= GPU =================

    pub(crate) fn page_gpu(&self, ui: &mut Ui, idx: usize) {
        let th = &self.th;
        let s = &self.mon.stats;
        let Some(g) = s.gpus.get(idx) else {
            ui.label(meta(th, "No se encontró esta GPU."));
            return;
        };
        let h = self.mon.hist.gpu_detail.get(&g.id);
        let heat = |v: f32| Grad::Heat.at(th, v / 100.0);
        let kind = if g.integrated { "integrada en el procesador" } else { "dedicada" };
        ui.label(meta(th, format!("{}, {kind}, PCI {}", g.name, g.id)));

        if g.asleep {
            surface(ui, th, |ui| {
                ui.add_space(20.0);
                ui.vertical_centered(|ui| {
                    ui.label(strong("Dormida", c(th.ok), 22.0));
                    ui.label(body(th, "Está en ahorro de energía porque ningún programa la usa."));
                    ui.label(meta(th, "rtop no la despierta para no gastar batería. Si se despierta, acá vas a ver quién la usa."));
                });
                ui.add_space(20.0);
            });
            return;
        }

        let power_max = g
            .power_limit_w
            .or_else(|| h.map(|h| h.power.iter().copied().fold(0.0f32, f32::max) * 1.25))
            .unwrap_or(1.0)
            .max(1.0);
        let busy = g.busy.unwrap_or(0.0);
        let mut stats = vec![
            Stat::new("Uso", format!("{busy:.0}%"), heat(busy)).meter(busy, Grad::Heat),
            Stat::new("VRAM", human(g.vram_used), c(th.mem))
                .detail(format!("de {} ({:.0}%)", human(g.vram_total), g.vram_pct()))
                .meter(g.vram_pct(), Grad::Mono(th.mem)),
            Stat::new("Temperatura", g.temp.map(|t| format!("{t:.0} °C")).unwrap_or("sin dato".into()), g.temp.map(heat).unwrap_or(muted(th))),
            Stat::new("Potencia", g.power_w.map(|p| format!("{p:.1} W")).unwrap_or("sin dato".into()), c(th.text))
                .detail(g.power_limit_w.map(|l| format!("límite {l:.0} W")).unwrap_or_default()),
        ];
        if !g.integrated {
            stats.push(
                Stat::new("Estado", "Despierta", c(th.warn))
                    .detail(g.pstate.as_ref().map(|p| format!("modo {p} (P0 rápido, P8 reposo)")).unwrap_or_default()),
            );
        }
        stat_row(ui, th, "gpu_stats", &stats);

        let series = |q: Option<&rtop_core::Hist<f32>>| q.map(|q| tail_f32(q, crate::WINDOW)).unwrap_or_default();
        surface(ui, th, |ui| {
            section(ui, th, "Uso", Some(strong(format!("{busy:.0}%"), heat(busy), 15.0)));
            chart(ui, th, &self.opts(150.0, Some(100.0), pct_fmt), &[Series::new("uso", series(h.map(|h| &h.busy)), Grad::Heat)], None);
        });
        surface(ui, th, |ui| {
            section(
                ui,
                th,
                "VRAM",
                Some(strong(format!("{} de {}", human(g.vram_used), human(g.vram_total)), c(th.mem), 15.0)),
            );
            // en % del total, así la escala no salta
            let mut o = self.opts(110.0, Some(100.0), pct_fmt);
            o.floor = 100.0;
            chart(ui, th, &o, &[Series::new("VRAM", series(h.map(|h| &h.vram)), Grad::Mono(th.mem))], None);
        });

        columns(ui, 2, 2, 12.0, |ui, i| {
            surface(ui, th, |ui| {
                if i == 0 {
                    let t = g.temp.unwrap_or(0.0);
                    section(ui, th, "Temperatura", Some(strong(format!("{t:.0} °C"), heat(t), 15.0)));
                    chart(ui, th, &self.opts(100.0, Some(100.0), temp_fmt), &[Series::new("temperatura", series(h.map(|h| &h.temp)), Grad::Heat)], None);
                } else {
                    section(ui, th, "Potencia", Some(strong(g.power_w.map(|p| format!("{p:.1} W")).unwrap_or("-".into()), c(th.warn), 15.0)));
                    chart(
                        ui,
                        th,
                        &self.opts(100.0, Some(power_max), watt_fmt),
                        &[Series::new("potencia", series(h.map(|h| &h.power)), Grad::Mono(th.warn))],
                        None,
                    );
                }
            });
        });

        surface(ui, th, |ui| {
            section(ui, th, "Procesos que la usan", Some(meta(th, match g.procs.len() { 1 => "1 proceso".to_string(), n => format!("{n} procesos") })));
            let rows = g
                .procs
                .iter()
                .take(12)
                .map(|gp| {
                    let u = gp.usage.unwrap_or(0.0);
                    let mut cols = vec![
                        Col::Meter(u, Grad::Heat, gp.usage.map(|u| format!("{u:.1}%")).unwrap_or("sin dato".into())),
                        Col::Text(human(gp.vram), c(th.mem)),
                    ];
                    match self.mon.proc(gp.pid) {
                        Some(p) => {
                            let (cpu, mem, time) = self.base_cols(p);
                            cols.extend([cpu, mem, time]);
                        }
                        None => cols.extend((0..3).map(|_| Col::Text(String::new(), muted(th)))),
                    }
                    (gp.pid, gp.name.clone(), cols)
                })
                .collect();
            self.proc_table(
                ui,
                "gpu_procs",
                &[("Uso de GPU", 116.0), ("VRAM", 80.0), ("CPU", 116.0), ("Memoria", 124.0), ("Tiempo activo", 104.0)],
                rows,
            );
        });

        surface(ui, th, |ui| {
            section(ui, th, "Detalle", None);
            let opt = |v: Option<f32>, f: fn(f64) -> String| v.map(|x| f(x as f64)).unwrap_or("sin dato".into());
            let mut rows = vec![
                ("Fabricante", g.vendor.label().to_string()),
                ("Reloj del núcleo", opt(g.clock_mhz, mhz_fmt)),
                ("Reloj de la memoria", opt(g.mem_clock_mhz, mhz_fmt)),
                ("Ventilador", g.fan_pct.map(|f| format!("{f:.0}%")).unwrap_or("sin dato (lo maneja la laptop)".into())),
                ("Potencia", match g.power_limit_w { Some(l) => format!("{} de {}", opt(g.power_w, watt_fmt), watt_fmt(l as f64)), None => opt(g.power_w, watt_fmt) }),
                ("VRAM", format!("{} de {}", human(g.vram_used), human(g.vram_total))),
            ];
            if g.shared_total > 0 {
                rows.push(("Memoria compartida", format!("{} de {} (toma RAM)", human(g.shared_used), human(g.shared_total))));
            }
            kv(ui, th, "gpu_kv", &rows);
        });
    }

    // ================= RED =================

    pub(crate) fn page_net(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;
        let (real, virt): (Vec<_>, Vec<_>) = s.nets.iter().partition(|n| !n.is_virtual());
        let real: Vec<_> = real.into_iter().filter(|n| n.total_rx + n.total_tx > 0 || !n.ips.is_empty()).collect();

        stat_row(
            ui,
            th,
            "net_stats",
            &[
                Stat::new("Bajada", rate_fmt(s.rx_rate), c(th.ok)),
                Stat::new("Subida", rate_fmt(s.tx_rate), c(th.warn)),
                Stat::new("Conexiones", real.len().to_string(), c(th.text))
                    .detail(real.iter().map(|n| n.name.as_str()).collect::<Vec<_>>().join(", ")),
            ],
        );

        let rate = |h: f32| {
            let mut o = self.opts(h, None, rate_fmt);
            o.floor = 4096.0;
            o.binary = true;
            o
        };
        surface(ui, th, |ui| {
            section(ui, th, "Tráfico", None);
            chart(
                ui,
                th,
                &rate(190.0),
                &[Series::new("bajada", tail_f64(&m.hist.rx, crate::WINDOW), Grad::Mono(th.ok))],
                Some(&Series::new("subida", tail_f64(&m.hist.tx, crate::WINDOW), Grad::Mono(th.warn))),
            );
            legend(ui, th, &[("bajada (arriba)".into(), c(th.ok)), ("subida (abajo)".into(), c(th.warn))]);
        });

        surface(ui, th, |ui| {
            section(ui, th, "Interfaces", None);
            for (i, n) in real.iter().enumerate() {
                if i > 0 {
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                }
                let kind = if n.name.starts_with("wl") {
                    "Wi-Fi"
                } else if n.name.starts_with("en") || n.name.starts_with("eth") {
                    "cable"
                } else if n.name.starts_with("tailscale") {
                    "VPN de Tailscale"
                } else if n.name.starts_with("tun") || n.name.starts_with("wg") {
                    "VPN"
                } else {
                    ""
                };
                ui.horizontal(|ui| {
                    ui.label(strong(&n.name, c(th.text), 15.0));
                    ui.label(meta(th, kind));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(num(format!("subida {}", rate_fmt(n.tx_rate)), c(th.warn)));
                        ui.label(num(format!("bajada {}", rate_fmt(n.rx_rate)), c(th.ok)));
                    });
                });
                ui.add_space(4.0);
                if let Some((rx, tx)) = m.hist.nets.get(&n.name) {
                    chart(
                        ui,
                        th,
                        &rate(130.0),
                        &[Series::new("bajada", tail_f64(rx, crate::WINDOW), Grad::Mono(th.ok))],
                        Some(&Series::new("subida", tail_f64(tx, crate::WINDOW), Grad::Mono(th.warn))),
                    );
                }
                ui.add_space(8.0);
                let ips: Vec<std::net::IpAddr> = n.ips.iter().filter_map(|s| s.split('/').next()?.parse().ok()).collect();
                let users = rtop_core::netproc::by_process(&self.conns, &ips);
                let mut rows = vec![("IP", if n.ips.is_empty() { "sin IP".into() } else { n.ips.join("\n") })];
                if n.mac != "00:00:00:00:00:00" && !n.mac.is_empty() {
                    rows.push(("MAC", n.mac.clone()));
                }
                rows.push(("Bajado desde el arranque", human(n.total_rx)));
                rows.push(("Subido desde el arranque", human(n.total_tx)));
                kv(ui, th, &format!("net_{}", n.name), &rows);
                ui.add_space(8.0);
                ui.label(body(th, "Procesos con conexiones activas"));
                if users.is_empty() {
                    ui.label(meta(th, "Sin conexiones activas en esta interfaz."));
                } else {
                    let rows = users
                        .iter()
                        .take(8)
                        .map(|(pid, name, k, dst)| (*pid, name.to_string(), vec![Col::Text(k.to_string(), c(th.text)), Col::Note(dst.join(", "))]))
                        .collect();
                    self.proc_table(ui, &format!("netp_{}", n.name), &[("Conexiones", 90.0), ("Destinos", 380.0)], rows);
                }
            }
            ui.add_space(8.0);
            ui.label(meta(
                th,
                "El tráfico por proceso requiere permisos de administrador, por eso se listan las conexiones activas de cada uno. No incluye procesos del sistema (root).",
            ));
            if !virt.is_empty() {
                ui.separator();
                egui::CollapsingHeader::new(meta(th, format!("Interfaces virtuales (docker, puentes): {}", virt.len())))
                    .id_salt("virt")
                    .show(ui, |ui| {
                        egui::Grid::new("virt_g").num_columns(4).spacing([18.0, 4.0]).show(ui, |ui| {
                            for n in virt {
                                ui.label(body(th, &n.name));
                                ui.label(meta(th, n.ips.first().cloned().unwrap_or("sin IP".into())));
                                ui.label(meta(th, format!("bajado {}", human(n.total_rx))));
                                ui.label(meta(th, format!("subido {}", human(n.total_tx))));
                                ui.end_row();
                            }
                        });
                    });
            }
        });
    }

    // ================= DISCO =================

    pub(crate) fn page_disk(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;

        stat_row(
            ui,
            th,
            "disk_stats",
            &[
                Stat::new("Lectura", rate_fmt(s.disk_read_rate), c(th.accent)),
                Stat::new("Escritura", rate_fmt(s.disk_write_rate), c(th.mem)),
                Stat::new("Espacio usado en /", format!("{:.0}%", s.disk_pct()), Grad::Heat.at(th, s.disk_pct() / 100.0))
                    .detail(format!("{} de {}", human(s.disk_used), human(s.disk_total)))
                    .meter(s.disk_pct(), Grad::Heat),
            ],
        );

        surface(ui, th, |ui| {
            section(ui, th, "Actividad", None);
            chart(
                ui,
                th,
                &ChartOpts { floor: 64.0 * 1024.0, binary: true, ..self.opts(170.0, None, rate_fmt) },
                &[Series::new("lectura", tail_f64(&m.hist.disk_r, crate::WINDOW), Grad::Mono(th.accent))],
                Some(&Series::new("escritura", tail_f64(&m.hist.disk_w, crate::WINDOW), Grad::Mono(th.mem))),
            );
            legend(ui, th, &[("lectura (arriba)".into(), c(th.accent)), ("escritura (abajo)".into(), c(th.mem))]);
        });

        surface(ui, th, |ui| {
            section(ui, th, "Discos y particiones", None);
            for d in &s.devs {
                ui.horizontal(|ui| {
                    ui.label(strong(d.model.clone().unwrap_or(d.name.clone()), c(th.text), 15.0));
                    ui.label(meta(th, &d.name));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(meta(th, format!("leído {} y escrito {} desde el arranque", human(d.read_total), human(d.write_total))));
                    });
                });
            }
            ui.add_space(4.0);
            egui::Grid::new("mounts").num_columns(4).spacing([20.0, 8.0]).show(ui, |ui| {
                for h in ["Montada en", "Partición", "Ocupado", ""] {
                    ui.label(meta(th, h));
                }
                ui.end_row();
                for mnt in &s.mounts {
                    let p = pct(mnt.used, mnt.total);
                    ui.label(body(th, &mnt.mount));
                    ui.label(meta(th, format!("{} ({})", mnt.device, mnt.fs)));
                    meter(ui, th, 240.0, p, Grad::Heat);
                    ui.label(num(format!("{} de {} ({p:.0}%)", human(mnt.used), human(mnt.total)), c(th.text)));
                    ui.end_row();
                }
            });
        });

        surface(ui, th, |ui| {
            section(ui, th, "Los que más leen y escriben", None);
            let top: Vec<&Proc> = m.top(SortBy::Disk, 10).into_iter().filter(|p| p.disk_read + p.disk_write > 0.0).collect();
            if top.is_empty() {
                ui.label(meta(th, "Ninguno de tus procesos está usando el disco ahora. Los del sistema (root) no se pueden ver sin permisos."));
            } else {
                let rows = top
                    .iter()
                    .map(|p| {
                        {
                            let (cpu, mem, time) = self.base_cols(p);
                            (p.pid, p.name.clone(), vec![Col::Text(rate_fmt(p.disk_read), c(th.accent)), Col::Text(rate_fmt(p.disk_write), c(th.mem)), cpu, mem, time])
                        }
                    })
                    .collect();
                self.proc_table(ui, "disk_top", &[("Lectura", 90.0), ("Escritura", 90.0), ("CPU", 116.0), ("Memoria", 124.0), ("Tiempo activo", 104.0)], rows);
            }
        });
    }

    // ================= TEMPERATURAS =================

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
                data: m.hist.temps.get(&t.label).map(|h| tail_f32(h, crate::WINDOW)).unwrap_or_default(),
            })
            .collect();
        for g in &s.gpus {
            if g.vendor == Vendor::Nvidia && !g.asleep {
                items.push(T {
                    label: g.short_name(),
                    value: g.temp.unwrap_or(0.0),
                    max: None,
                    crit: None,
                    data: m.hist.gpu_detail.get(&g.id).map(|h| tail_f32(&h.temp, crate::WINDOW)).unwrap_or_default(),
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
        let open = self.sensor_open.borrow().clone();
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
                            *self.sensor_open.borrow_mut() = None;
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
                *self.sensor_open.borrow_mut() = if is_open { None } else { Some(t.label.clone()) };
            }
        });
    }
}
