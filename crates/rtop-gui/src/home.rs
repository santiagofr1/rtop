//! Pantalla principal: consumo de CPU / RAM / GPUs arriba y la tabla de
//! procesos (ordenable por cualquier columna) con el detalle del elegido.

use eframe::egui::{self, Align, Color32, CornerRadius, Layout, RichText, Sense, Ui, Vec2};
use egui_extras::{Column, TableBuilder};
use rtop_core::{
    fmt::{hms, human},
    SortBy, State,
};

use crate::{
    kit::{self, big, c, caption, chart, dim, header, kv, paint_meter, tail_f32, Grad, Series},
    pages::{pct_fmt, rate_fmt},
    App, Page, WINDOW,
};

fn state_name(s: State) -> &'static str {
    match s {
        State::Run => "corriendo",
        State::Sleep => "durmiendo",
        State::Stop => "detenido",
        State::Zombie => "zombie",
        State::Disk => "esperando disco",
        State::Other => "desconocido",
    }
}

impl App {
    pub(crate) fn page_home(&mut self, ui: &mut Ui) {
        self.home_cards(ui);
        let th = self.th.clone();
        egui::Frame::new()
            .fill(c(th.panel))
            .corner_radius(CornerRadius::same(10))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                self.toolbar(ui);
                ui.add_space(4.0);
                let h = ui.available_height();
                let gap = 12.0;
                let total = ui.available_width();
                let detail_w = if self.selected.is_some() { (total * 0.32).clamp(250.0, 360.0) } else { 0.0 };
                let table_w = ui.available_width() - detail_w - if detail_w > 0.0 { gap } else { 0.0 };
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.allocate_ui_with_layout(Vec2::new(table_w, h), Layout::top_down(Align::Min), |ui| {
                        ui.set_width(table_w);
                        ui.set_height(h);
                        self.table(ui);
                    });
                    if detail_w > 0.0 {
                        ui.allocate_ui_with_layout(Vec2::new(detail_w, h), Layout::top_down(Align::Min), |ui| {
                            ui.set_width(detail_w);
                            egui::ScrollArea::vertical().id_salt("detail").auto_shrink([false, false]).show(ui, |ui| {
                                self.detail(ui);
                            });
                        });
                    }
                });
            });
    }

    /// Tarjetas de consumo (CPU, RAM, una por GPU). Click = ir al detalle.
    fn home_cards(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let th = &th;
        let s = &self.mon.stats;
        let m = &self.mon;
        let heat = |v: f32| Grad::Heat.at(th, v / 100.0);

        struct CardData {
            page: Page,
            title: String,
            value: RichText,
            sub: String,
            series: Vec<Series>,
        }
        let mut cards = vec![
            CardData {
                page: Page::Cpu,
                title: "CPU".into(),
                value: big(format!("{:.0}%", s.cpu_total), heat(s.cpu_total), 22.0),
                sub: {
                    let mut t = format!("{} · {:.1} GHz", s.cpu_model, s.cpu_freq as f32 / 1000.0);
                    if let Some(x) = s.cpu_temp {
                        t.push_str(&format!(" · {x:.0}°C"));
                    }
                    t
                },
                series: vec![Series::new("CPU", tail_f32(&m.cpu_hist, WINDOW), Grad::Heat)],
            },
            CardData {
                page: Page::Mem,
                title: "RAM".into(),
                value: big(format!("{:.0}%", s.mem_pct()), heat(s.mem_pct()), 22.0),
                sub: format!("{} de {} · swap {}", human(s.mem_used), human(s.mem_total), human(s.swap_used)),
                series: vec![Series::new("RAM", tail_f32(&m.mem_hist, WINDOW), Grad::Mono(th.mem))],
            },
        ];
        for (i, g) in s.gpus.iter().enumerate() {
            let v = g.busy.unwrap_or(0.0);
            cards.push(CardData {
                page: Page::Gpu(i),
                title: format!("GPU {} · {}", i + 1, g.short_name()),
                value: if g.asleep { big("dormida", c(th.dim), 18.0) } else { big(format!("{v:.0}%"), heat(v), 22.0) },
                sub: if g.asleep {
                    "en ahorro de energía".into()
                } else {
                    let mut t = format!("VRAM {} de {}", human(g.vram_used), human(g.vram_total));
                    if let Some(x) = g.temp {
                        t.push_str(&format!(" · {x:.0}°C"));
                    }
                    t
                },
                series: if g.asleep {
                    vec![]
                } else {
                    vec![Series::new("uso", m.gpu_hist.get(i).map(|h| tail_f32(h, WINDOW)).unwrap_or_default(), Grad::Heat)]
                },
            });
        }

        let n = cards.len();
        let mut go = None;
        let opts = self.opts(64.0, Some(100.0), pct_fmt);
        kit::columns(ui, n, n, 10.0, |ui, i| {
            let cd = &cards[i];
            let r = egui::Frame::new()
                .fill(c(th.panel))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = 4.0;
                    header(ui, th, &cd.title, cd.value.clone());
                    ui.label(dim(th, &cd.sub).size(11.5));
                    if cd.series.is_empty() {
                        ui.add_space(opts.height);
                    } else {
                        chart(ui, th, &opts, &cd.series, None);
                    }
                });
            let resp = ui
                .interact(r.response.rect, egui::Id::new(("home_card", i)), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("click para ver el detalle");
            if resp.hovered() {
                ui.painter().rect_stroke(
                    r.response.rect,
                    CornerRadius::same(10),
                    egui::Stroke::new(1.0, c(th.accent)),
                    egui::StrokeKind::Inside,
                );
            }
            if resp.clicked() {
                go = Some(cd.page);
            }
        });
        if let Some(p) = go {
            self.go(p);
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label(caption(&self.th, "Procesos"));
            ui.add_space(6.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.filter)
                    .hint_text("filtrar por nombre, comando, usuario o pid")
                    .desired_width(260.0),
            );
            if r.changed() {
                self.rebuild();
            }
            if !self.filter.is_empty() && ui.button("x").clicked() {
                self.filter.clear();
                self.rebuild();
            }
            ui.separator();
            ui.label(dim(&self.th, format!("{} procesos", self.view.len())))
                .on_hover_text("click en el título de una columna para ordenar; otro click invierte");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut cmd = self.cfg.show_cmd;
                if ui.checkbox(&mut cmd, "comando completo").changed() {
                    self.cfg.show_cmd = cmd;
                    self.save_cfg();
                }
            });
        });
    }

    fn table(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let arrow = if self.reverse { " ^" } else { " v" };
        let mut clicked_sort: Option<SortBy> = None;
        let mut clicked_row: Option<u32> = None;
        let mut kill_req: Option<(u32, String)> = None;
        let sort = self.sort;
        let show_cmd = self.cfg.show_cmd;

        let hdr = |ui: &mut Ui, name: &str, s: SortBy, out: &mut Option<SortBy>| {
            let active = s == sort;
            let txt = if active { format!("{name}{arrow}") } else { name.to_string() };
            let rt = RichText::new(txt).strong().size(12.0).color(if active { c(th.bg) } else { c(th.accent) });
            let b = egui::Button::new(rt).fill(if active { c(th.accent) } else { Color32::TRANSPARENT });
            if ui.add(b).on_hover_text("ordenar (otro click invierte)").clicked() {
                *out = Some(s);
            }
        };

        // columnas: las opcionales se agregan mientras entren (por prioridad)
        #[derive(Clone, Copy, PartialEq)]
        enum K {
            Pid,
            Name,
            User,
            Cpu,
            Ram,
            Res,
            Gpu,
            Vram,
            Disk,
            St,
            Time,
        }
        let spacing = 8.0;
        let width = |k: K| match k {
            K::Pid => 60.0,
            K::Name => 140.0,
            K::User => 88.0,
            K::Cpu => 104.0,
            K::Ram => 96.0,
            K::Res => 62.0,
            K::Gpu => 56.0,
            K::Vram => 62.0,
            K::Disk => 72.0,
            K::St => 20.0,
            K::Time => 74.0,
        };
        let avail = ui.available_width() - 16.0;
        let mut on = vec![K::Pid, K::Name, K::Cpu, K::Ram];
        let mut used: f32 = on.iter().map(|k| width(*k) + spacing).sum();
        for k in [K::Gpu, K::Res, K::Time, K::User, K::Vram, K::Disk, K::St] {
            if used + width(k) + spacing <= avail {
                used += width(k) + spacing;
                on.push(k);
            }
        }
        let order = [K::Pid, K::Name, K::User, K::Cpu, K::Ram, K::Res, K::Gpu, K::Vram, K::Disk, K::St, K::Time];
        let cols: Vec<K> = order.into_iter().filter(|k| on.contains(k)).collect();

        let mut tb = TableBuilder::new(ui)
            .striped(true)
            .sense(Sense::click())
            .auto_shrink([false, false])
            .cell_layout(Layout::left_to_right(Align::Center));
        for k in &cols {
            tb = tb.column(if *k == K::Name {
                Column::remainder().at_least(width(K::Name)).clip(true)
            } else {
                Column::exact(width(*k)).clip(true)
            });
        }
        tb.header(26.0, |mut h| {
            for k in &cols {
                let (name, s) = match k {
                    K::Pid => ("PID", SortBy::Pid),
                    K::Name => (if show_cmd { "COMANDO" } else { "NOMBRE" }, SortBy::Name),
                    K::User => ("USUARIO", SortBy::User),
                    K::Cpu => ("CPU%", SortBy::Cpu),
                    K::Ram => ("RAM%", SortBy::Mem),
                    K::Res => ("RES", SortBy::Mem),
                    K::Gpu => ("GPU%", SortBy::Gpu),
                    K::Vram => ("VRAM", SortBy::Vram),
                    K::Disk => ("DISCO", SortBy::Disk),
                    K::St => ("S", SortBy::State),
                    K::Time => ("TIEMPO", SortBy::Time),
                };
                h.col(|ui| hdr(ui, name, s, &mut clicked_sort));
            }
        })
        .body(|body| {
            body.rows(22.0, self.view.len(), |mut row| {
                let p = &self.mon.procs[self.view[row.index()]];
                row.set_selected(self.selected == Some(p.pid));
                let mono = |t: String, col: Color32| RichText::new(t).monospace().color(col);
                let user_c = if p.user == "root" { th.hot } else { th.text };
                let st_c = match p.state {
                    State::Run => th.ok,
                    State::Stop | State::Disk => th.warn,
                    State::Zombie => th.hot,
                    _ => th.dim,
                };
                let label = if show_cmd && !p.cmd.is_empty() { &p.cmd } else { &p.name };
                let meter_cell = |ui: &mut Ui, v: f32, g: Grad, t: String, faint: bool| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(40.0, 8.0), Sense::hover());
                    paint_meter(ui.painter(), &th, r, v, g);
                    ui.label(mono(t, if faint { c(th.dim) } else { c(th.text) }));
                };
                for k in &cols {
                    row.col(|ui| match k {
                        K::Pid => {
                            ui.label(mono(format!("{:>7}", p.pid), c(th.dim)));
                        }
                        K::Name => {
                            ui.label(mono(label.clone(), c(th.text)));
                        }
                        K::User => {
                            ui.label(mono(p.user.clone(), c(user_c)));
                        }
                        K::Cpu => meter_cell(ui, p.cpu.min(100.0), Grad::Heat, format!("{:>5.1}", p.cpu), p.cpu < 1.0),
                        K::Ram => meter_cell(ui, p.mem_pct * 4.0, Grad::Mono(th.mem), format!("{:>4.1}", p.mem_pct), p.mem_pct < 1.0),
                        K::Res => {
                            ui.label(mono(format!("{:>6}", human(p.mem)), c(th.text)));
                        }
                        K::Gpu => match p.gpu {
                            Some(g) => {
                                ui.label(mono(format!("{g:>5.1}"), if g < 1.0 { c(th.dim) } else { Grad::Heat.at(&th, g / 100.0) }));
                            }
                            None => {
                                ui.label(mono("    -".into(), c(th.dim)));
                            }
                        },
                        K::Vram => {
                            ui.label(mono(
                                if p.vram > 0 { human(p.vram) } else { "-".into() },
                                if p.vram > 0 { c(th.mem) } else { c(th.dim) },
                            ));
                        }
                        K::Disk => {
                            let io = p.disk_read + p.disk_write;
                            ui.label(mono(
                                if io > 0.0 { format!("{}/s", human(io as u64)) } else { "-".into() },
                                if io > 0.0 { c(th.accent) } else { c(th.dim) },
                            ));
                        }
                        K::St => {
                            ui.label(mono(p.state.letter().into(), c(st_c))).on_hover_text(state_name(p.state));
                        }
                        K::Time => {
                            ui.label(mono(hms(p.time), c(th.dim)));
                        }
                    });
                }
                let r = row.response();
                if r.clicked() {
                    clicked_row = Some(p.pid);
                }
                let (pid, name) = (p.pid, p.name.clone());
                r.context_menu(|ui| {
                    if ui.button("Ver detalle").clicked() {
                        clicked_row = Some(pid);
                        ui.close();
                    }
                    if ui.button("Terminar proceso").clicked() {
                        kill_req = Some((pid, name.clone()));
                        ui.close();
                    }
                });
            });
        });

        if let Some(s) = clicked_sort {
            self.set_sort(s);
        }
        if let Some(pid) = clicked_row {
            self.select(if self.selected == Some(pid) { None } else { Some(pid) });
        }
        if kill_req.is_some() {
            self.confirm_kill = kill_req;
        }
    }

    /// Panel con todo lo que se sabe del proceso elegido.
    fn detail(&mut self, ui: &mut Ui) {
        let th = self.th.clone();
        let th = &th;
        let Some(pid) = self.selected else { return };
        let Some(p) = self.mon.proc(pid).cloned() else {
            ui.label(dim(th, "El proceso terminó."));
            return;
        };
        let d = self.details.as_ref().filter(|(x, _)| *x == pid).map(|(_, d)| d.clone()).unwrap_or_default();
        let ncores = self.mon.stats.cores.len().max(1);

        ui.horizontal(|ui| {
            ui.label(big(&p.name, c(th.text), 20.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("x").on_hover_text("cerrar (Esc)").clicked() {
                    self.select(None);
                }
            });
        });
        ui.label(dim(th, format!("pid {} · {}", p.pid, state_name(p.state))));
        if ui.button(RichText::new("Terminar proceso").color(c(th.hot))).clicked() {
            self.confirm_kill = Some((p.pid, p.name.clone()));
        }
        ui.add_space(4.0);

        // CPU
        let cpu_data: Vec<f32> = self.sel_cpu.iter().copied().collect();
        let cpu_max = cpu_data.iter().copied().fold(100.0f32, f32::max);
        header(ui, th, "CPU", big(format!("{:.1}%", p.cpu), Grad::Heat.at(th, p.cpu / 100.0), 16.0));
        ui.label(dim(th, format!("100% = un núcleo entero · tenés {ncores} hilos")).size(10.5));
        chart(ui, th, &self.opts(70.0, Some(cpu_max), pct_fmt), &[Series::new("CPU", cpu_data, Grad::Heat)], None);

        // RAM
        header(ui, th, "RAM", big(human(p.mem), c(th.mem), 16.0));
        let mem_data: Vec<f32> = self.sel_mem.iter().copied().collect();
        let mem_max = mem_data.iter().copied().fold(1.0f32, f32::max) * 1.3;
        chart(ui, th, &self.opts(60.0, Some(mem_max), pct_fmt), &[Series::new("RAM %", mem_data, Grad::Mono(th.mem))], None);

        ui.add_space(4.0);
        let parent = p.parent.map(|pp| match self.mon.proc(pp) {
            Some(x) => format!("{} ({pp})", x.name),
            None => pp.to_string(),
        });
        let mut rows = vec![
            ("Usuario", p.user.clone()),
            ("Proceso padre", parent.unwrap_or("-".into())),
            ("Corriendo hace", hms(p.time)),
            ("Hilos", d.threads.map(|t| t.to_string()).unwrap_or("-".into())),
            ("Prioridad (nice)", d.nice.map(|n| n.to_string()).unwrap_or("-".into())),
            ("RAM", format!("{} ({:.1}%)", human(p.mem), p.mem_pct)),
            ("Swap", d.swap.map(human).unwrap_or("-".into())),
            ("Disco lectura", rate_fmt(p.disk_read)),
            ("Disco escritura", rate_fmt(p.disk_write)),
        ];
        for g in &self.mon.stats.gpus {
            if let Some(gp) = g.procs.iter().find(|x| x.pid == p.pid) {
                rows.push((
                    "GPU",
                    format!(
                        "{}: {} · VRAM {}",
                        g.short_name(),
                        gp.usage.map(|u| format!("{u:.1}%")).unwrap_or("-".into()),
                        human(gp.vram)
                    ),
                ));
            }
        }
        kv(ui, th, "proc_kv", &rows);

        ui.add_space(6.0);
        ui.label(caption(th, "Ejecutable"));
        ui.add(egui::Label::new(RichText::new(d.exe.unwrap_or("- (sin permisos)".into())).monospace().size(11.0)).wrap());
        ui.label(caption(th, "Carpeta de trabajo"));
        ui.add(egui::Label::new(RichText::new(d.cwd.unwrap_or("-".into())).monospace().size(11.0)).wrap());
        ui.label(caption(th, "Comando"));
        let cmd = if p.cmd.is_empty() { p.name.clone() } else { p.cmd.clone() };
        ui.add(egui::Label::new(RichText::new(cmd).monospace().size(11.0).color(c(th.text))).wrap().selectable(true));
    }
}
