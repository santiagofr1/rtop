//! Páginas de detalle. Estructura común: fila de estado, un gráfico principal
//! grande, y después los grupos de datos. Leen de `App` sin mutarlo; las
//! acciones (matar, navegar) se piden por `App::kill_req` / `App::goto`.

use eframe::egui::{self, Align, Align2, Color32, CornerRadius, FontId, Layout, Pos2, Rect, Sense, Ui, Vec2};
use rtop_core::{fmt::human, pct, Proc, SortBy, Vendor};

use crate::{
    kit::{
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

/// Columnas extra de las tablas compactas de procesos.
pub enum Col {
    Text(String, Color32),
    Meter(f32, Grad, String),
}

impl App {
    pub(crate) fn opts(&self, height: f32, max: Option<f32>, fmt: fn(f64) -> String) -> ChartOpts {
        ChartOpts { height, max, fmt, window: crate::WINDOW, step: self.cfg.refresh_ms() as f32 / 1000.0, fill: true }
    }

    /// Tabla compacta de procesos (click = verlo en Inicio, click derecho = terminar).
    pub(crate) fn proc_table(&self, ui: &mut Ui, id: &str, heads: &[&str], rows: Vec<(u32, String, Vec<Col>)>) {
        let th = &self.th;
        if rows.is_empty() {
            ui.label(meta(th, "Ninguno por ahora."));
            return;
        }
        egui::Grid::new(id).num_columns(heads.len() + 1).spacing([22.0, 7.0]).show(ui, |ui| {
            ui.label(meta(th, "Proceso"));
            for h in heads {
                ui.label(meta(th, *h));
            }
            ui.end_row();
            for (pid, name, cols) in rows {
                let r = ui
                    .add(egui::Label::new(body(th, &name)).sense(Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text(format!("PID {pid}. Click para verlo en Inicio"));
                if r.clicked() {
                    self.goto.set(Some(crate::Page::Home));
                    self.select_req.set(Some(pid));
                }
                r.context_menu(|ui| {
                    if ui.button("Terminar proceso…").clicked() {
                        *self.kill_req.borrow_mut() = Some((pid, name.clone()));
                        ui.close();
                    }
                });
                for col in cols {
                    match col {
                        Col::Text(t, color) => {
                            ui.label(num(t, color));
                        }
                        Col::Meter(v, g, t) => {
                            ui.horizontal(|ui| {
                                meter(ui, th, 64.0, v, g);
                                ui.label(num(t, c(th.text)));
                            });
                        }
                    }
                }
                ui.end_row();
            }
        });
    }

    fn cpu_rows(&self, procs: &[&Proc]) -> Vec<(u32, String, Vec<Col>)> {
        let th = &self.th;
        let scale = if self.cfg.cpu_per_core { 1.0 } else { 4.0 };
        procs
            .iter()
            .map(|p| {
                let v = self.cpu_of(p);
                (
                    p.pid,
                    p.name.clone(),
                    vec![Col::Meter((v * scale).min(100.0), Grad::Heat, format!("{v:>5.1}%")), Col::Text(human(p.mem), muted(th))],
                )
            })
            .collect()
    }

    fn mem_rows(&self, procs: &[&Proc]) -> Vec<(u32, String, Vec<Col>)> {
        let th = &self.th;
        procs
            .iter()
            .map(|p| {
                (
                    p.pid,
                    p.name.clone(),
                    vec![Col::Meter(p.mem_pct * 4.0, Grad::Mono(th.mem), human(p.mem)), Col::Text(format!("{:.1}%", p.mem_pct), muted(th))],
                )
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
            chart(ui, th, &self.opts(170.0, Some(100.0), pct_fmt), &[Series::new("CPU", tail_f32(&m.cpu_hist, crate::WINDOW), Grad::Heat)], None);
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
                        &[Series::new("frecuencia", tail_f32(&m.freq_hist, crate::WINDOW), Grad::Mono(th.accent))],
                        None,
                    );
                } else {
                    let t = s.cpu_temp.unwrap_or(0.0);
                    section(ui, th, "Temperatura", Some(strong(format!("{t:.0} °C"), heat(t), 15.0)));
                    let data = s
                        .temps
                        .iter()
                        .find(|x| Some(x.value) == s.cpu_temp)
                        .and_then(|x| m.temp_hist.get(&x.label))
                        .map(|h| tail_f32(h, crate::WINDOW))
                        .unwrap_or_default();
                    chart(ui, th, &self.opts(110.0, Some(100.0), temp_fmt), &[Series::new("temperatura", data, Grad::Heat)], None);
                }
            });
        });

        surface(ui, th, |ui| {
            section(ui, th, "Los que más CPU usan", Some(meta(th, self.cpu_hint())));
            let top = m.top(SortBy::Cpu, 10);
            self.proc_table(ui, "cpu_top", &["CPU", "Memoria"], self.cpu_rows(&top));
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
                        m.mem_hist.iter().zip(&m.cache_hist).rev().take(crate::WINDOW).rev().map(|(a, b)| (a + b).min(100.0)).collect(),
                        Grad::Mono(th.accent),
                    ),
                    Series::new("en uso", tail_f32(&m.mem_hist, crate::WINDOW), Grad::Mono(th.mem)),
                ],
                None,
            );
        });

        columns(ui, 2, 2, 12.0, |ui, i| {
            surface(ui, th, |ui| {
                if i == 0 {
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
                } else {
                    section(ui, th, "Los que más memoria usan", None);
                    let top = m.top(SortBy::Mem, 10);
                    self.proc_table(ui, "mem_top", &["Memoria", "% del total"], self.mem_rows(&top));
                }
            });
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
        let h = self.mon.gpu_detail.get(&g.id);
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
            section(ui, th, "Uso y VRAM", None);
            let mut o = self.opts(160.0, Some(100.0), pct_fmt);
            o.fill = false;
            chart(
                ui,
                th,
                &o,
                &[Series::new("VRAM", series(h.map(|h| &h.vram)), Grad::Mono(th.mem)), Series::new("uso", series(h.map(|h| &h.busy)), Grad::Heat)],
                None,
            );
            legend(ui, th, &[("uso".into(), c(th.ok)), ("VRAM".into(), c(th.mem))]);
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

        columns(ui, 2, 2, 12.0, |ui, i| {
            surface(ui, th, |ui| {
                if i == 0 {
                    section(ui, th, "Quién la está usando", Some(meta(th, match g.procs.len() { 1 => "1 proceso".to_string(), n => format!("{n} procesos") })));
                    let rows = g
                        .procs
                        .iter()
                        .take(12)
                        .map(|p| {
                            let u = p.usage.unwrap_or(0.0);
                            (
                                p.pid,
                                p.name.clone(),
                                vec![
                                    Col::Meter(u, Grad::Heat, p.usage.map(|u| format!("{u:>5.1}%")).unwrap_or("sin dato".into())),
                                    Col::Text(human(p.vram), c(th.mem)),
                                ],
                            )
                        })
                        .collect();
                    self.proc_table(ui, "gpu_procs", &["Uso", "VRAM"], rows);
                } else {
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
                }
            });
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

        surface(ui, th, |ui| {
            section(ui, th, "Tráfico", None);
            chart(
                ui,
                th,
                &self.opts(190.0, None, rate_fmt),
                &[Series::new("bajada", tail_f64(&m.rx_hist, crate::WINDOW), Grad::Mono(th.ok))],
                Some(&Series::new("subida", tail_f64(&m.tx_hist, crate::WINDOW), Grad::Mono(th.warn))),
            );
            legend(ui, th, &[("bajada (arriba)".into(), c(th.ok)), ("subida (abajo)".into(), c(th.warn))]);
        });

        surface(ui, th, |ui| {
            section(ui, th, "Interfaces", None);
            for (i, n) in real.iter().enumerate() {
                if i > 0 {
                    ui.separator();
                }
                ui.horizontal(|ui| {
                    ui.label(strong(&n.name, c(th.text), 15.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(num(format!("subida {}", rate_fmt(n.tx_rate)), c(th.warn)));
                        ui.label(num(format!("bajada {}", rate_fmt(n.rx_rate)), c(th.ok)));
                    });
                });
                ui.horizontal_top(|ui| {
                    let w = ui.available_width();
                    ui.allocate_ui_with_layout(Vec2::new(w * 0.55, 0.0), Layout::top_down(Align::Min), |ui| {
                        if let Some((rx, tx)) = m.net_hist.get(&n.name) {
                            chart(
                                ui,
                                th,
                                &self.opts(70.0, None, rate_fmt),
                                &[Series::new("bajada", tail_f64(rx, crate::WINDOW), Grad::Mono(th.ok))],
                                Some(&Series::new("subida", tail_f64(tx, crate::WINDOW), Grad::Mono(th.warn))),
                            );
                        }
                    });
                    ui.add_space(16.0);
                    ui.vertical(|ui| {
                        kv(
                            ui,
                            th,
                            &format!("net_{}", n.name),
                            &[
                                ("IP", if n.ips.is_empty() { "sin IP".into() } else { n.ips.join("\n") }),
                                ("MAC", n.mac.clone()),
                                ("Bajado desde el arranque", human(n.total_rx)),
                                ("Subido desde el arranque", human(n.total_tx)),
                            ],
                        );
                    });
                });
            }
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
                &self.opts(170.0, None, rate_fmt),
                &[Series::new("lectura", tail_f64(&m.disk_r_hist, crate::WINDOW), Grad::Mono(th.accent))],
                Some(&Series::new("escritura", tail_f64(&m.disk_w_hist, crate::WINDOW), Grad::Mono(th.mem))),
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
                        (p.pid, p.name.clone(), vec![Col::Text(rate_fmt(p.disk_read), c(th.accent)), Col::Text(rate_fmt(p.disk_write), c(th.mem))])
                    })
                    .collect();
                self.proc_table(ui, "disk_top", &["Lectura", "Escritura"], rows);
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
                data: m.temp_hist.get(&t.label).map(|h| tail_f32(h, crate::WINDOW)).unwrap_or_default(),
            })
            .collect();
        for g in &s.gpus {
            if g.vendor == Vendor::Nvidia && !g.asleep {
                items.push(T {
                    label: g.short_name(),
                    value: g.temp.unwrap_or(0.0),
                    max: None,
                    crit: None,
                    data: m.gpu_detail.get(&g.id).map(|h| tail_f32(&h.temp, crate::WINDOW)).unwrap_or_default(),
                });
            }
        }
        if items.is_empty() {
            surface(ui, th, |ui| {
                ui.label(meta(th, "No se encontraron sensores de temperatura."));
            });
            return;
        }
        items.sort_by(|a, b| b.value.total_cmp(&a.value));

        let hottest = &items[0];
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

        // un color por sensor, repartiendo el círculo de tonos
        let n = items.len();
        let palette: Vec<Color32> = (0..n).map(|i| egui::ecolor::Hsva::new(0.02 + 0.8 * i as f32 / n.max(2) as f32, 0.55, 0.95, 1.0).into()).collect();
        let rgb = |col: Color32| rtop_core::Rgb(col.r(), col.g(), col.b());

        surface(ui, th, |ui| {
            section(ui, th, "Todas juntas", None);
            let mut o = self.opts(220.0, Some(100.0), temp_fmt);
            o.fill = false;
            let series: Vec<Series> =
                items.iter().enumerate().map(|(i, t)| Series::new(t.label.clone(), t.data.clone(), Grad::Mono(rgb(palette[i])))).collect();
            chart(ui, th, &o, &series, None);
            legend(ui, th, &items.iter().enumerate().map(|(i, t)| (t.label.clone(), palette[i])).collect::<Vec<_>>());
        });

        surface(ui, th, |ui| {
            section(ui, th, "Por sensor", None);
            egui::Grid::new("temps").num_columns(5).spacing([22.0, 9.0]).show(ui, |ui| {
                for h in ["Sensor", "Ahora", "", "Máxima vista", "Crítica"] {
                    ui.label(meta(th, h));
                }
                ui.end_row();
                for (i, t) in items.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
                        ui.painter().rect_filled(r, CornerRadius::same(2), palette[i]);
                        ui.label(body(th, &t.label));
                    });
                    ui.label(strong(format!("{:.0} °C", t.value), Grad::Heat.at(th, t.value / 100.0), 15.0));
                    meter(ui, th, 200.0, t.value, Grad::Heat);
                    ui.label(num(t.max.map(|x| format!("{x:.0} °C")).unwrap_or("sin dato".into()), muted(th)));
                    ui.label(num(t.crit.map(|x| format!("{x:.0} °C")).unwrap_or("sin dato".into()), muted(th)));
                    ui.end_row();
                }
            });
        });
    }
}
