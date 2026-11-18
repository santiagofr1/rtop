//! Páginas de detalle. Todas leen de `App` sin mutarlo; las acciones (matar,
//! navegar) se piden por `App::kill_req` / `App::goto`.

use eframe::egui::{self, Color32, RichText, Ui, Vec2};
use rtop_core::{
    fmt::{hms, human},
    pct, Proc, SortBy,
};

use crate::{
    kit::{self, big, c, caption, card, chart, columns, dim, header, kv, meter, ring, tail_f32, tail_f64, ChartOpts, Grad, Series},
    App, Page,
};

pub fn pct_fmt(v: f64) -> String {
    format!("{v:.0}%")
}
pub fn rate_fmt(v: f64) -> String {
    if v <= 0.0 { "0".into() } else { format!("{}/s", human(v as u64)) }
}
pub fn temp_fmt(v: f64) -> String {
    format!("{v:.0}°C")
}
pub fn watt_fmt(v: f64) -> String {
    format!("{v:.1} W")
}
pub fn mhz_fmt(v: f64) -> String {
    if v >= 1000.0 { format!("{:.2} GHz", v / 1000.0) } else { format!("{v:.0} MHz") }
}
pub fn num_fmt(v: f64) -> String {
    format!("{v:.2}")
}

pub enum Col {
    Text(String, Color32),
    Meter(f32, Grad, String),
}

impl App {
    pub(crate) fn opts(&self, height: f32, max: Option<f32>, fmt: fn(f64) -> String) -> ChartOpts {
        ChartOpts { height, max, fmt, window: crate::WINDOW, step: self.cfg.refresh_ms() as f32 / 1000.0 }
    }

    /// Tabla compacta de procesos con click derecho para matar.
    pub(crate) fn proc_table(&self, ui: &mut Ui, id: &str, heads: &[&str], rows: Vec<(u32, String, Vec<Col>)>) {
        let th = &self.th;
        if rows.is_empty() {
            ui.label(dim(th, "sin procesos"));
            return;
        }
        egui::Grid::new(id).num_columns(heads.len()).striped(true).spacing([16.0, 5.0]).show(ui, |ui| {
            for h in heads {
                ui.label(caption(th, h));
            }
            ui.end_row();
            for (pid, name, cols) in rows {
                ui.label(RichText::new(pid.to_string()).monospace().color(c(th.dim)));
                let r = ui.add(egui::Label::new(RichText::new(&name).color(c(th.text))).sense(egui::Sense::click()));
                r.context_menu(|ui| {
                    if ui.button("Terminar proceso").clicked() {
                        *self.kill_req.borrow_mut() = Some((pid, name.clone()));
                        ui.close();
                    }
                });
                for col in cols {
                    match col {
                        Col::Text(t, color) => {
                            ui.label(RichText::new(t).monospace().color(color));
                        }
                        Col::Meter(v, g, t) => {
                            ui.horizontal(|ui| {
                                meter(ui, th, 70.0, v, g);
                                ui.label(RichText::new(t).monospace().color(c(th.text)));
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
        procs
            .iter()
            .map(|p| {
                (
                    p.pid,
                    p.name.clone(),
                    vec![
                        Col::Meter(p.cpu.min(100.0), Grad::Heat, format!("{:>5.1}%", p.cpu)),
                        Col::Text(human(p.mem), c(th.text)),
                        Col::Text(p.user.clone(), c(if p.user == "root" { th.hot } else { th.dim })),
                    ],
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
                    vec![
                        Col::Meter(p.mem_pct * 4.0, Grad::Mono(th.mem), format!("{:>5.1}%", p.mem_pct)),
                        Col::Text(human(p.mem), c(th.text)),
                        Col::Text(p.user.clone(), c(if p.user == "root" { th.hot } else { th.dim })),
                    ],
                )
            })
            .collect()
    }

    // ================= RESUMEN =================

    pub(crate) fn page_overview(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;

        ui.horizontal(|ui| {
            ui.label(big(&s.host, c(th.text), 26.0));
            ui.add_space(10.0);
            ui.label(dim(
                th,
                format!("{} · kernel {} · encendida hace {} · {} procesos", s.os, s.kernel, hms(s.uptime), s.tasks),
            ));
        });

        // anillos
        struct R {
            page: Page,
            title: String,
            pct: Option<f32>,
            center: String,
            sub: String,
            grad: Grad,
        }
        let mut rings = vec![
            R {
                page: Page::Cpu,
                title: "CPU".into(),
                pct: Some(s.cpu_total),
                center: format!("{:.0}%", s.cpu_total),
                sub: format!("{} · {:.1} GHz", s.cpu_model, s.cpu_freq as f32 / 1000.0),
                grad: Grad::Heat,
            },
            R {
                page: Page::Mem,
                title: "Memoria".into(),
                pct: Some(s.mem_pct()),
                center: format!("{:.0}%", s.mem_pct()),
                sub: format!("{} / {}", human(s.mem_used), human(s.mem_total)),
                grad: Grad::Heat,
            },
        ];
        for (i, g) in s.gpus.iter().enumerate() {
            rings.push(R {
                page: Page::Gpu(i),
                title: g.short_name(),
                pct: g.busy.filter(|_| !g.asleep),
                center: if g.asleep { "zzz".into() } else { format!("{:.0}%", g.busy.unwrap_or(0.0)) },
                sub: if g.asleep {
                    "dormida".into()
                } else {
                    format!("VRAM {} / {}", human(g.vram_used), human(g.vram_total))
                },
                grad: Grad::Heat,
            });
        }
        rings.push(R {
            page: Page::Disk,
            title: "Disco /".into(),
            pct: Some(s.disk_pct()),
            center: format!("{:.0}%", s.disk_pct()),
            sub: format!("{} / {}", human(s.disk_used), human(s.disk_total)),
            grad: Grad::Heat,
        });
        if let Some(t) = s.cpu_temp {
            rings.push(R {
                page: Page::Sensors,
                title: "Temp CPU".into(),
                pct: Some(t),
                center: format!("{t:.0}°"),
                sub: format!("máx {:.0}°C", s.temp_max.unwrap_or(t)),
                grad: Grad::Heat,
            });
        }
        let n = rings.len();
        columns(ui, n, n, 10.0, |ui, i| {
            let r = &rings[i];
            card(ui, th, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(caption(th, &r.title));
                    let size = (ui.available_width() - 10.0).clamp(80.0, 130.0);
                    let resp = ring(ui, th, size, r.pct, &r.center, r.grad)
                        .on_hover_text("click para ver el detalle")
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if resp.clicked() {
                        self.goto.set(Some(r.page));
                    }
                    ui.label(dim(th, &r.sub).size(12.0));
                });
            });
        });

        // gráficos
        columns(ui, 3, 3, 10.0, |ui, i| {
            card(ui, th, |ui| match i {
                0 => {
                    header(ui, th, "CPU", big(format!("{:.0}%", s.cpu_total), kit::Grad::Heat.at(th, s.cpu_total / 100.0), 18.0));
                    chart(ui, th, &self.opts(110.0, Some(100.0), pct_fmt), &[Series::new("CPU", tail_f32(&m.cpu_hist, crate::WINDOW), Grad::Heat)], None);
                }
                1 => {
                    header(ui, th, "Memoria", big(format!("{:.0}%", s.mem_pct()), kit::Grad::Heat.at(th, s.mem_pct() / 100.0), 18.0));
                    chart(ui, th, &self.opts(110.0, Some(100.0), pct_fmt), &[Series::new("RAM", tail_f32(&m.mem_hist, crate::WINDOW), Grad::Mono(th.mem))], None);
                }
                _ => {
                    header(
                        ui,
                        th,
                        "Red",
                        RichText::new(format!("v {}/s   ^ {}/s", human(s.rx_rate as u64), human(s.tx_rate as u64))).color(c(th.text)),
                    );
                    chart(
                        ui,
                        th,
                        &self.opts(110.0, None, rate_fmt),
                        &[Series::new("bajada", tail_f64(&m.rx_hist, crate::WINDOW), Grad::Mono(th.ok))],
                        Some(&Series::new("subida", tail_f64(&m.tx_hist, crate::WINDOW), Grad::Mono(th.warn))),
                    );
                }
            });
        });

        columns(ui, 3, 3, 10.0, |ui, i| {
            card(ui, th, |ui| match i {
                0 => {
                    let v = s.gpus.iter().find(|g| !g.asleep).and_then(|g| g.busy).unwrap_or(0.0);
                    header(ui, th, "GPU", big(format!("{v:.0}%"), Grad::Heat.at(th, v / 100.0), 18.0));
                    let series: Vec<Series> = s
                        .gpus
                        .iter()
                        .enumerate()
                        .filter(|(_, g)| !g.asleep)
                        .map(|(i, g)| {
                            let grad = if i == 0 { Grad::Heat } else { Grad::Mono(th.accent) };
                            Series::new(g.short_name(), tail_f32(&m.gpu_hist[i], crate::WINDOW), grad)
                        })
                        .collect();
                    chart(ui, th, &self.opts(110.0, Some(100.0), pct_fmt), &series, None);
                }
                1 => {
                    header(
                        ui,
                        th,
                        "Disco",
                        RichText::new(format!("r {}/s   w {}/s", human(s.disk_read_rate as u64), human(s.disk_write_rate as u64))).color(c(th.text)),
                    );
                    chart(
                        ui,
                        th,
                        &self.opts(110.0, None, rate_fmt),
                        &[Series::new("lectura", tail_f64(&m.disk_r_hist, crate::WINDOW), Grad::Mono(th.accent))],
                        Some(&Series::new("escritura", tail_f64(&m.disk_w_hist, crate::WINDOW), Grad::Mono(th.mem))),
                    );
                }
                _ => {
                    let g = s.gpus.iter().find(|g| !g.asleep && g.vram_total > 0);
                    header(
                        ui,
                        th,
                        "VRAM",
                        g.map(|g| RichText::new(format!("{} / {}", human(g.vram_used), human(g.vram_total))).color(c(th.mem)))
                            .unwrap_or_else(|| dim(th, "-")),
                    );
                    let series: Vec<Series> = s
                        .gpus
                        .iter()
                        .enumerate()
                        .filter(|(_, g)| !g.asleep)
                        .map(|(i, g)| {
                            let grad = if i == 0 { Grad::Mono(th.mem) } else { Grad::Mono(th.accent) };
                            Series::new(g.short_name(), tail_f32(&m.vram_hist[i], crate::WINDOW), grad)
                        })
                        .collect();
                    chart(ui, th, &self.opts(110.0, Some(100.0), pct_fmt), &series, None);
                }
            });
        });

        // top procesos + temperaturas
        columns(ui, 3, 3, 10.0, |ui, i| {
            card(ui, th, |ui| match i {
                0 => {
                    ui.label(caption(th, "Más CPU"));
                    let top = m.top(SortBy::Cpu, 6);
                    self.proc_table(ui, "ov_cpu", &["PID", "Nombre", "CPU", "RAM", "Usuario"], self.cpu_rows(&top));
                }
                1 => {
                    ui.label(caption(th, "Más memoria"));
                    let top = m.top(SortBy::Mem, 6);
                    self.proc_table(ui, "ov_mem", &["PID", "Nombre", "RAM", "Tamaño", "Usuario"], self.mem_rows(&top));
                }
                _ => {
                    ui.label(caption(th, "Temperaturas"));
                    self.temp_list(ui, 7);
                }
            });
        });
    }

    fn temp_list(&self, ui: &mut Ui, max: usize) {
        let th = &self.th;
        let s = &self.mon.stats;
        let mut temps: Vec<(String, f32)> = s.temps.iter().map(|t| (t.label.clone(), t.value)).collect();
        for g in &s.gpus {
            if g.vendor == rtop_core::Vendor::Nvidia {
                if let Some(t) = g.temp {
                    temps.push((g.short_name(), t));
                }
            }
        }
        egui::Grid::new("temps").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
            for (label, v) in temps.into_iter().take(max) {
                ui.label(RichText::new(label).color(c(th.text)));
                meter(ui, th, 110.0, v, Grad::Heat);
                ui.label(RichText::new(format!("{v:.0}°C")).strong().color(Grad::Heat.at(th, v / 100.0)));
                ui.end_row();
            }
        });
    }

    // ================= CPU =================

    pub(crate) fn page_cpu(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;
        let heat = |v: f32| Grad::Heat.at(th, v / 100.0);

        card(ui, th, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(caption(th, "Procesador"));
                    ui.label(big(format!("{:.0}%", s.cpu_total), heat(s.cpu_total), 44.0));
                    ui.label(RichText::new(&s.cpu_model).color(c(th.text)).size(15.0));
                    ui.label(dim(th, &s.cpu_brand).size(11.0));
                });
                ui.add_space(30.0);
                kv(
                    ui,
                    th,
                    "cpu_kv",
                    &[
                        ("Hilos", s.cores.len().to_string()),
                        ("Frecuencia", mhz_fmt(s.cpu_freq as f64)),
                        ("Carga 1/5/15", format!("{:.2}  {:.2}  {:.2}", s.load.0, s.load.1, s.load.2)),
                        ("Temperatura", s.cpu_temp.map(|t| format!("{t:.0}°C")).unwrap_or("-".into())),
                        ("Procesos", format!("{} ({} corriendo)", s.tasks, s.running)),
                        ("Encendida", hms(s.uptime)),
                    ],
                );
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    chart(ui, th, &self.opts(150.0, Some(100.0), pct_fmt), &[Series::new("CPU", tail_f32(&m.cpu_hist, crate::WINDOW), Grad::Heat)], None);
                });
            });
        });

        // núcleos
        let n = s.cores.len();
        let cols = kit::fit_cols(ui.available_width(), 170.0, n);
        columns(ui, n, cols, 8.0, |ui, i| {
            let u = s.cores[i];
            card(ui, th, |ui| {
                ui.horizontal(|ui| {
                    ui.label(caption(th, &format!("Núcleo {i}")));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(big(format!("{u:.0}%"), heat(u), 15.0));
                        ui.label(dim(th, mhz_fmt(*s.core_freq.get(i).unwrap_or(&0) as f64)).size(11.0));
                    });
                });
                let data = m.core_hist.get(i).map(|h| tail_f32(h, crate::WINDOW)).unwrap_or_default();
                chart(ui, th, &self.opts(46.0, Some(100.0), pct_fmt), &[Series::new(&format!("núcleo {i}"), data, Grad::Heat)], None);
            });
        });

        columns(ui, 3, 3, 10.0, |ui, i| {
            card(ui, th, |ui| match i {
                0 => {
                    header(ui, th, "Frecuencia", big(mhz_fmt(s.cpu_freq as f64), c(th.accent), 16.0));
                    chart(ui, th, &self.opts(90.0, None, mhz_fmt), &[Series::new("frecuencia", tail_f32(&m.freq_hist, crate::WINDOW), Grad::Mono(th.accent))], None);
                }
                1 => {
                    header(ui, th, "Carga", big(format!("{:.2}", s.load.0), c(th.text), 16.0));
                    chart(ui, th, &self.opts(90.0, None, num_fmt), &[Series::new("carga 1m", tail_f32(&m.load_hist, crate::WINDOW), Grad::Mono(th.mem))], None);
                }
                _ => {
                    let t = s.cpu_temp.unwrap_or(0.0);
                    header(ui, th, "Temperatura", big(format!("{t:.0}°C"), heat(t), 16.0));
                    let key = s.temps.iter().find(|x| Some(x.value) == s.cpu_temp).map(|x| x.label.clone());
                    let data = key.and_then(|k| m.temp_hist.get(&k)).map(|h| tail_f32(h, crate::WINDOW)).unwrap_or_default();
                    chart(ui, th, &self.opts(90.0, Some(100.0), temp_fmt), &[Series::new("temp", data, Grad::Heat)], None);
                }
            });
        });

        card(ui, th, |ui| {
            ui.label(caption(th, "Procesos que más CPU usan"));
            let top = m.top(SortBy::Cpu, 12);
            self.proc_table(ui, "cpu_top", &["PID", "Nombre", "CPU", "RAM", "Usuario"], self.cpu_rows(&top));
        });
    }

    // ================= MEMORIA =================

    pub(crate) fn page_mem(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let mi = &s.meminfo;
        let m = &self.mon;

        card(ui, th, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label(caption(th, "RAM"));
                    ring(ui, th, 130.0, Some(pct(mi.used(), mi.total)), &format!("{:.0}%", pct(mi.used(), mi.total)), Grad::Heat);
                });
                ui.vertical(|ui| {
                    ui.label(caption(th, "Swap"));
                    ring(ui, th, 130.0, Some(s.swap_pct()), &format!("{:.0}%", s.swap_pct()), Grad::Heat);
                });
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(caption(th, "Cómo se reparte"));
                    // barra apilada usada / caché / libre
                    let w = ui.available_width();
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 22.0), egui::Sense::hover());
                    let p = ui.painter();
                    let total = mi.total.max(1) as f32;
                    let parts = [
                        (mi.used(), c(th.mem), "Usada"),
                        (mi.cache(), c(th.accent), "Caché/buffers"),
                        (mi.free, c(th.sel), "Libre"),
                    ];
                    let mut x = rect.left();
                    for (v, col, _) in parts {
                        let ww = rect.width() * v as f32 / total;
                        p.rect_filled(
                            egui::Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(ww, rect.height())),
                            egui::CornerRadius::same(3),
                            col,
                        );
                        x += ww;
                    }
                    ui.horizontal(|ui| {
                        for (v, col, name) in parts {
                            ui.label(RichText::new("■").color(col));
                            ui.label(RichText::new(format!("{name} {}", human(v))).color(c(th.text)));
                            ui.add_space(10.0);
                        }
                    });
                    ui.add_space(6.0);
                    kv(
                        ui,
                        th,
                        "mem_kv",
                        &[
                            ("Total", human(mi.total)),
                            ("En uso", format!("{} ({:.0}%)", human(mi.used()), pct(mi.used(), mi.total))),
                            ("Disponible", human(mi.available)),
                            ("Caché", human(mi.cached)),
                            ("Buffers", human(mi.buffers)),
                            ("Compartida", human(mi.shmem)),
                            ("Pendiente de escribir", human(mi.dirty)),
                            ("Swap", format!("{} / {}", human(s.swap_used), human(s.swap_total))),
                        ],
                    );
                });
            });
        });

        columns(ui, 2, 2, 10.0, |ui, i| {
            card(ui, th, |ui| match i {
                0 => {
                    header(ui, th, "RAM en uso", big(format!("{:.0}%", s.mem_pct()), Grad::Heat.at(th, s.mem_pct() / 100.0), 16.0));
                    chart(
                        ui,
                        th,
                        &self.opts(130.0, Some(100.0), pct_fmt),
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
                }
                _ => {
                    header(ui, th, "Swap", big(format!("{:.0}%", s.swap_pct()), Grad::Heat.at(th, s.swap_pct() / 100.0), 16.0));
                    chart(ui, th, &self.opts(130.0, Some(100.0), pct_fmt), &[Series::new("swap", tail_f32(&m.swap_hist, crate::WINDOW), Grad::Heat)], None);
                }
            });
        });

        card(ui, th, |ui| {
            ui.label(caption(th, "Procesos que más memoria usan"));
            let top = m.top(SortBy::Mem, 12);
            self.proc_table(ui, "mem_top", &["PID", "Nombre", "RAM", "Tamaño", "Usuario"], self.mem_rows(&top));
        });
    }

    // ================= GPU =================

    pub(crate) fn page_gpu(&self, ui: &mut Ui, idx: usize) {
        let th = &self.th;
        let s = &self.mon.stats;
        let Some(g) = s.gpus.get(idx) else {
            ui.label(dim(th, "GPU no encontrada"));
            return;
        };
        let h = self.mon.gpu_detail.get(&g.id);
        let heat = |v: f32| Grad::Heat.at(th, v / 100.0);

        card(ui, th, |ui| {
            ui.label(caption(th, if g.integrated { "GPU integrada" } else { "GPU dedicada" }));
            ui.label(big(&g.name, c(th.text), 22.0));
            let mut info = vec![g.vendor.label().to_string(), format!("PCI {}", g.id)];
            if let Some(p) = &g.pstate {
                info.push(format!("estado {p}"));
            }
            ui.label(dim(th, info.join(" · ")));
        });

        if g.asleep {
            card(ui, th, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    ui.label(big("La GPU está dormida", c(th.dim), 20.0));
                    ui.label(dim(th, "Está en ahorro de energía porque nadie la usa. rtop no la despierta para no gastar batería."));
                    ui.add_space(20.0);
                });
            });
            return;
        }

        let power_max = g
            .power_limit_w
            .or_else(|| h.map(|h| h.power.iter().copied().fold(0.0f32, f32::max) * 1.25))
            .unwrap_or(1.0)
            .max(1.0);
        let rings: Vec<(&str, Option<f32>, String, String)> = vec![
            ("Uso", g.busy, format!("{:.0}%", g.busy.unwrap_or(0.0)), g.clock_mhz.map(|c| format!("reloj {}", mhz_fmt(c as f64))).unwrap_or_default()),
            ("VRAM", Some(g.vram_pct()), format!("{:.0}%", g.vram_pct()), format!("{} / {}", human(g.vram_used), human(g.vram_total))),
            ("Temperatura", g.temp, g.temp.map(|t| format!("{t:.0}°")).unwrap_or("-".into()), String::new()),
            (
                "Potencia",
                g.power_w.map(|p| p * 100.0 / power_max),
                g.power_w.map(|p| format!("{p:.1}W")).unwrap_or("-".into()),
                g.power_limit_w.map(|l| format!("límite {l:.0} W")).unwrap_or_default(),
            ),
        ];
        columns(ui, rings.len(), rings.len(), 10.0, |ui, i| {
            let (t, p, ctr, sub) = &rings[i];
            card(ui, th, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(caption(th, t));
                    ring(ui, th, 120.0, *p, ctr, Grad::Heat);
                    ui.label(dim(th, sub).size(12.0));
                });
            });
        });

        let series = |q: Option<&rtop_core::Hist<f32>>| q.map(|q| tail_f32(q, crate::WINDOW)).unwrap_or_default();
        columns(ui, 4, 2, 10.0, |ui, i| {
            card(ui, th, |ui| match i {
                0 => {
                    header(ui, th, "Uso", big(format!("{:.0}%", g.busy.unwrap_or(0.0)), heat(g.busy.unwrap_or(0.0)), 16.0));
                    chart(ui, th, &self.opts(110.0, Some(100.0), pct_fmt), &[Series::new("uso", series(h.map(|h| &h.busy)), Grad::Heat)], None);
                }
                1 => {
                    header(ui, th, "VRAM", big(format!("{} / {}", human(g.vram_used), human(g.vram_total)), c(th.mem), 16.0));
                    chart(ui, th, &self.opts(110.0, Some(100.0), pct_fmt), &[Series::new("VRAM", series(h.map(|h| &h.vram)), Grad::Mono(th.mem))], None);
                }
                2 => {
                    let t = g.temp.unwrap_or(0.0);
                    header(ui, th, "Temperatura", big(format!("{t:.0}°C"), heat(t), 16.0));
                    chart(ui, th, &self.opts(110.0, Some(100.0), temp_fmt), &[Series::new("temp", series(h.map(|h| &h.temp)), Grad::Heat)], None);
                }
                _ => {
                    header(ui, th, "Potencia", big(g.power_w.map(|p| format!("{p:.1} W")).unwrap_or("-".into()), c(th.warn), 16.0));
                    chart(ui, th, &self.opts(110.0, Some(power_max), watt_fmt), &[Series::new("potencia", series(h.map(|h| &h.power)), Grad::Mono(th.warn))], None);
                }
            });
        });

        columns(ui, 2, 2, 10.0, |ui, i| {
            card(ui, th, |ui| {
                if i == 0 {
                    ui.label(caption(th, "Detalles"));
                    let opt = |v: Option<f32>, f: fn(f64) -> String| v.map(|x| f(x as f64)).unwrap_or("-".into());
                    let mut rows = vec![
                        ("Reloj núcleo", opt(g.clock_mhz, mhz_fmt)),
                        ("Reloj memoria", opt(g.mem_clock_mhz, mhz_fmt)),
                        ("Ventilador", g.fan_pct.map(|f| format!("{f:.0}%")).unwrap_or("-".into())),
                        ("Potencia / límite", format!("{} / {}", opt(g.power_w, watt_fmt), opt(g.power_limit_w, watt_fmt))),
                        ("VRAM", format!("{} / {}", human(g.vram_used), human(g.vram_total))),
                    ];
                    if g.shared_total > 0 {
                        rows.push(("Memoria compartida", format!("{} / {}", human(g.shared_used), human(g.shared_total))));
                    }
                    kv(ui, th, "gpu_kv", &rows);
                    if g.integrated {
                        ui.add_space(4.0);
                        ui.label(dim(th, "Es integrada: tiene poca VRAM propia y usa la RAM como memoria compartida.").size(11.0));
                    }
                } else {
                    ui.label(caption(th, "Procesos usando esta GPU"));
                    let rows = g
                        .procs
                        .iter()
                        .take(14)
                        .map(|p| {
                            let u = p.usage.unwrap_or(0.0);
                            (
                                p.pid,
                                p.name.clone(),
                                vec![
                                    Col::Meter(u, Grad::Heat, p.usage.map(|u| format!("{u:>5.1}%")).unwrap_or("  -".into())),
                                    Col::Text(human(p.vram), c(th.mem)),
                                ],
                            )
                        })
                        .collect();
                    self.proc_table(ui, "gpu_procs", &["PID", "Nombre", "Uso", "VRAM"], rows);
                }
            });
        });
    }

    // ================= RED =================

    pub(crate) fn page_net(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;

        card(ui, th, |ui| {
            ui.horizontal(|ui| {
                ui.label(caption(th, "Red"));
                ui.add_space(20.0);
                ui.label(big(format!("v {}/s", human(s.rx_rate as u64)), c(th.ok), 22.0));
                ui.add_space(14.0);
                ui.label(big(format!("^ {}/s", human(s.tx_rate as u64)), c(th.warn), 22.0));
            });
            chart(
                ui,
                th,
                &self.opts(180.0, None, rate_fmt),
                &[Series::new("bajada", tail_f64(&m.rx_hist, crate::WINDOW), Grad::Mono(th.ok))],
                Some(&Series::new("subida", tail_f64(&m.tx_hist, crate::WINDOW), Grad::Mono(th.warn))),
            );
            ui.label(dim(th, "bajada hacia arriba, subida hacia abajo").size(11.0));
        });

        let (real, virt): (Vec<_>, Vec<_>) = s.nets.iter().partition(|n| !n.is_virtual());
        let real: Vec<_> = real.into_iter().filter(|n| n.total_rx + n.total_tx > 0 || !n.ips.is_empty()).collect();
        columns(ui, real.len(), real.len().min(2), 10.0, |ui, i| {
            let n = real[i];
            card(ui, th, |ui| {
                header(
                    ui,
                    th,
                    &n.name,
                    RichText::new(format!("v {}/s   ^ {}/s", human(n.rx_rate as u64), human(n.tx_rate as u64))).color(c(th.text)),
                );
                if let Some((rx, tx)) = m.net_hist.get(&n.name) {
                    chart(
                        ui,
                        th,
                        &self.opts(80.0, None, rate_fmt),
                        &[Series::new("bajada", tail_f64(rx, crate::WINDOW), Grad::Mono(th.ok))],
                        Some(&Series::new("subida", tail_f64(tx, crate::WINDOW), Grad::Mono(th.warn))),
                    );
                }
                let ips = if n.ips.is_empty() { "-".to_string() } else { n.ips.join("\n") };
                kv(
                    ui,
                    th,
                    &format!("net_{}", n.name),
                    &[
                        ("IP", ips),
                        ("MAC", n.mac.clone()),
                        ("Total bajado", human(n.total_rx)),
                        ("Total subido", human(n.total_tx)),
                    ],
                );
            });
        });

        if !virt.is_empty() {
            card(ui, th, |ui| {
                egui::CollapsingHeader::new(caption(th, &format!("Interfaces virtuales ({})", virt.len())))
                    .id_salt("virt")
                    .show(ui, |ui| {
                        egui::Grid::new("virt_g").num_columns(4).spacing([18.0, 4.0]).striped(true).show(ui, |ui| {
                            for n in virt {
                                ui.label(RichText::new(&n.name).color(c(th.text)));
                                ui.label(dim(th, n.ips.first().cloned().unwrap_or("-".into())));
                                ui.label(dim(th, format!("v {}", human(n.total_rx))));
                                ui.label(dim(th, format!("^ {}", human(n.total_tx))));
                                ui.end_row();
                            }
                        });
                    });
            });
        }
    }

    // ================= DISCO =================

    pub(crate) fn page_disk(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;

        card(ui, th, |ui| {
            ui.horizontal(|ui| {
                ui.label(caption(th, "Disco"));
                ui.add_space(20.0);
                ui.label(big(format!("lectura {}/s", human(s.disk_read_rate as u64)), c(th.accent), 20.0));
                ui.add_space(14.0);
                ui.label(big(format!("escritura {}/s", human(s.disk_write_rate as u64)), c(th.mem), 20.0));
            });
            chart(
                ui,
                th,
                &self.opts(160.0, None, rate_fmt),
                &[Series::new("lectura", tail_f64(&m.disk_r_hist, crate::WINDOW), Grad::Mono(th.accent))],
                Some(&Series::new("escritura", tail_f64(&m.disk_w_hist, crate::WINDOW), Grad::Mono(th.mem))),
            );
            ui.label(dim(th, "lectura hacia arriba, escritura hacia abajo").size(11.0));
        });

        columns(ui, s.devs.len(), s.devs.len().min(2), 10.0, |ui, i| {
            let d = &s.devs[i];
            card(ui, th, |ui| {
                header(ui, th, &d.name, RichText::new(d.model.clone().unwrap_or_default()).color(c(th.text)));
                if let Some((r, w)) = m.dev_hist.get(&d.name) {
                    chart(
                        ui,
                        th,
                        &self.opts(80.0, None, rate_fmt),
                        &[Series::new("lectura", tail_f64(r, crate::WINDOW), Grad::Mono(th.accent))],
                        Some(&Series::new("escritura", tail_f64(w, crate::WINDOW), Grad::Mono(th.mem))),
                    );
                }
                kv(
                    ui,
                    th,
                    &format!("dev_{}", d.name),
                    &[
                        ("Lectura", format!("{}/s", human(d.read_rate as u64))),
                        ("Escritura", format!("{}/s", human(d.write_rate as u64))),
                        ("Leído desde el arranque", human(d.read_total)),
                        ("Escrito desde el arranque", human(d.write_total)),
                    ],
                );
            });
        });

        card(ui, th, |ui| {
            ui.label(caption(th, "Particiones"));
            egui::Grid::new("mounts").num_columns(5).spacing([16.0, 6.0]).striped(true).show(ui, |ui| {
                for h in ["Montaje", "Dispositivo", "Uso", "", "Tipo"] {
                    ui.label(caption(th, h));
                }
                ui.end_row();
                for mnt in &s.mounts {
                    let p = pct(mnt.used, mnt.total);
                    ui.label(RichText::new(&mnt.mount).color(c(th.text)).strong());
                    ui.label(dim(th, &mnt.device));
                    meter(ui, th, 220.0, p, Grad::Heat);
                    ui.label(RichText::new(format!("{} / {} ({p:.0}%)", human(mnt.used), human(mnt.total))).color(c(th.text)));
                    let mut kind = mnt.fs.clone();
                    if mnt.ssd {
                        kind.push_str(" · SSD");
                    }
                    if mnt.removable {
                        kind.push_str(" · extraíble");
                    }
                    ui.label(dim(th, kind));
                    ui.end_row();
                }
            });
        });

        card(ui, th, |ui| {
            ui.label(caption(th, "Procesos que más leen/escriben"));
            let top: Vec<&Proc> = m.top(SortBy::Disk, 12).into_iter().filter(|p| p.disk_read + p.disk_write > 0.0).collect();
            let rows = top
                .iter()
                .map(|p| {
                    (
                        p.pid,
                        p.name.clone(),
                        vec![
                            Col::Text(format!("{}/s", human(p.disk_read as u64)), c(th.accent)),
                            Col::Text(format!("{}/s", human(p.disk_write as u64)), c(th.mem)),
                            Col::Text(p.user.clone(), c(th.dim)),
                        ],
                    )
                })
                .collect();
            if top.is_empty() {
                ui.label(dim(th, "Ninguno de tus procesos está leyendo o escribiendo ahora. Los de root (journald, kernel) no se ven sin permisos."));
            } else {
                self.proc_table(ui, "disk_top", &["PID", "Nombre", "Lectura", "Escritura", "Usuario"], rows);
            }
        });
    }

    // ================= SENSORES =================

    pub(crate) fn page_sensors(&self, ui: &mut Ui) {
        let th = &self.th;
        let s = &self.mon.stats;
        let m = &self.mon;
        let mut items: Vec<(String, f32, Option<f32>, Option<f32>, Vec<f32>)> = s
            .temps
            .iter()
            .map(|t| {
                let h = m.temp_hist.get(&t.label).map(|h| tail_f32(h, crate::WINDOW)).unwrap_or_default();
                (t.label.clone(), t.value, t.max, t.crit, h)
            })
            .collect();
        for g in &s.gpus {
            if g.vendor == rtop_core::Vendor::Nvidia && !g.asleep {
                let h = m.gpu_detail.get(&g.id).map(|h| tail_f32(&h.temp, crate::WINDOW)).unwrap_or_default();
                items.push((g.short_name(), g.temp.unwrap_or(0.0), None, None, h));
            }
        }
        if items.is_empty() {
            card(ui, th, |ui| {
                ui.label(dim(th, "No se encontraron sensores de temperatura."));
            });
            return;
        }
        columns(ui, items.len(), 3, 10.0, |ui, i| {
            let (label, v, max, crit, data) = &items[i];
            card(ui, th, |ui| {
                header(ui, th, label, big(format!("{v:.0}°C"), Grad::Heat.at(th, v / 100.0), 18.0));
                let mut sub = vec![];
                if let Some(mx) = max {
                    sub.push(format!("máx {mx:.0}°C"));
                }
                if let Some(cr) = crit {
                    sub.push(format!("crítica {cr:.0}°C"));
                }
                ui.label(dim(th, if sub.is_empty() { " ".into() } else { sub.join(" · ") }).size(11.0));
                chart(ui, th, &self.opts(70.0, Some(crit.unwrap_or(100.0).max(100.0)), temp_fmt), &[Series::new(label, data.clone(), Grad::Heat)], None);
            });
        });
    }
}
