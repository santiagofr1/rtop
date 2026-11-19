use std::{
    io,
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Sparkline, Table, TableState},
};
use rtop_core::{
    fmt::{hms, human},
    Config, Monitor, Rgb, SortBy, State, Theme,
};

fn c(rgb: Rgb) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

#[derive(PartialEq)]
enum Mode {
    Normal,
    Filter,
    ConfirmKill(u32, String),
    Help,
}

struct App {
    mon: Monitor,
    cfg: Config,
    th: Theme,
    view: Vec<usize>,
    table: TableState,
    sort: SortBy,
    reverse: bool,
    filter: String,
    mode: Mode,
    paused: bool,
    status_msg: Option<(String, Instant)>,
}

impl App {
    fn new() -> Self {
        let cfg = Config::load();
        let mut app = App {
            mon: Monitor::new(),
            th: cfg.theme(),
            cfg,
            view: Vec::new(),
            table: TableState::default().with_selected(0),
            sort: SortBy::Cpu,
            reverse: false,
            filter: String::new(),
            mode: Mode::Normal,
            paused: false,
            status_msg: None,
        };
        app.rebuild();
        app
    }

    fn tick(&self) -> Duration {
        Duration::from_millis(self.cfg.refresh_ms())
    }

    fn rebuild(&mut self) {
        let sel_pid = self.selected().map(|p| p.pid);
        self.view = self.mon.view(&self.filter, self.sort, self.reverse);
        let idx = sel_pid
            .and_then(|pid| self.view.iter().position(|&i| self.mon.procs[i].pid == pid))
            .unwrap_or_else(|| self.table.selected().unwrap_or(0));
        self.table.select(Some(idx.min(self.view.len().saturating_sub(1))));
    }

    fn refresh(&mut self) {
        let sel_pid = self.selected().map(|p| p.pid);
        self.mon.refresh();
        self.view = self.mon.view(&self.filter, self.sort, self.reverse);
        let idx = sel_pid
            .and_then(|pid| self.view.iter().position(|&i| self.mon.procs[i].pid == pid))
            .unwrap_or_else(|| self.table.selected().unwrap_or(0));
        self.table.select(Some(idx.min(self.view.len().saturating_sub(1))));
    }

    fn selected(&self) -> Option<&rtop_core::Proc> {
        self.table
            .selected()
            .and_then(|i| self.view.get(i))
            .map(|&i| &self.mon.procs[i])
    }

    fn move_sel(&mut self, delta: isize) {
        if self.view.is_empty() {
            return;
        }
        let cur = self.table.selected().unwrap_or(0) as isize;
        let max = self.view.len() as isize - 1;
        self.table.select(Some((cur + delta).clamp(0, max) as usize));
    }

    fn flash(&mut self, msg: impl Into<String>) {
        self.status_msg = Some((msg.into(), Instant::now()));
    }

    fn save_cfg(&mut self) {
        self.th = self.cfg.theme();
        if let Err(e) = self.cfg.save() {
            self.flash(format!("no pude guardar la config: {e}"));
        }
    }

    fn set_sort(&mut self, s: SortBy) {
        if self.sort == s {
            self.reverse = !self.reverse;
        } else {
            self.sort = s;
            self.reverse = false;
        }
        self.rebuild();
    }

    fn kill(&mut self, pid: u32, name: &str, force: bool) {
        let s = if force { "SIGKILL" } else { "SIGTERM" };
        if self.mon.kill(pid, force) {
            self.flash(format!("{s} -> {name} ({pid})"));
        } else {
            self.flash(format!("no pude matar {name} ({pid}) - permisos?"));
        }
    }

    /// devuelve true si hay que salir
    fn on_key(&mut self, k: KeyEvent) -> bool {
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            return true;
        }
        match std::mem::replace(&mut self.mode, Mode::Normal) {
            Mode::Help => {}
            Mode::Filter => match k.code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.rebuild();
                }
                KeyCode::Enter => {}
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.rebuild();
                    self.mode = Mode::Filter;
                }
                KeyCode::Char(ch) => {
                    self.filter.push(ch);
                    self.rebuild();
                    self.mode = Mode::Filter;
                }
                _ => self.mode = Mode::Filter,
            },
            Mode::ConfirmKill(pid, name) => match k.code {
                KeyCode::Char('y') | KeyCode::Char('s') | KeyCode::Enter => self.kill(pid, &name, false),
                KeyCode::Char('9') => self.kill(pid, &name, true),
                _ => self.flash("cancelado"),
            },
            Mode::Normal => match k.code {
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::F(10) => return true,
                KeyCode::Down | KeyCode::Char('j') => self.move_sel(1),
                KeyCode::Up | KeyCode::Char('k') => self.move_sel(-1),
                KeyCode::PageDown => self.move_sel(20),
                KeyCode::PageUp => self.move_sel(-20),
                KeyCode::Home | KeyCode::Char('g') => self.table.select(Some(0)),
                KeyCode::End | KeyCode::Char('G') => {
                    self.table.select(Some(self.view.len().saturating_sub(1)))
                }
                KeyCode::Char('/') | KeyCode::F(3) => self.mode = Mode::Filter,
                KeyCode::Char('s') | KeyCode::F(6) | KeyCode::Tab => {
                    self.sort = self.sort.next();
                    self.rebuild();
                }
                KeyCode::Char('c') => self.set_sort(SortBy::Cpu),
                KeyCode::Char('m') => self.set_sort(SortBy::Mem),
                KeyCode::Char('n') => self.set_sort(SortBy::Name),
                KeyCode::Char('i') => self.set_sort(SortBy::Pid),
                KeyCode::Char('t') => self.set_sort(SortBy::Time),
                KeyCode::Char('r') => {
                    self.reverse = !self.reverse;
                    self.rebuild();
                }
                KeyCode::Char('p') | KeyCode::Char(' ') => {
                    self.paused = !self.paused;
                    self.flash(if self.paused { "pausado" } else { "corriendo" });
                }
                KeyCode::Char('l') => {
                    self.cfg.show_cmd = !self.cfg.show_cmd;
                    self.save_cfg();
                }
                KeyCode::Char('v') => {
                    self.cfg.show_graphs = !self.cfg.show_graphs;
                    self.save_cfg();
                }
                KeyCode::Char('T') => {
                    self.cfg.next_theme();
                    self.save_cfg();
                    self.flash(format!("tema: {}", self.cfg.theme));
                }
                KeyCode::Char('+') => {
                    self.cfg.refresh_ms = (self.cfg.refresh_ms() + 250).min(10_000);
                    self.save_cfg();
                    self.flash(format!("refresco: {} ms", self.cfg.refresh_ms));
                }
                KeyCode::Char('-') => {
                    self.cfg.refresh_ms = self.cfg.refresh_ms().saturating_sub(250).max(250);
                    self.save_cfg();
                    self.flash(format!("refresco: {} ms", self.cfg.refresh_ms));
                }
                KeyCode::Char('x') | KeyCode::F(9) | KeyCode::Delete => {
                    if let Some(p) = self.selected() {
                        self.mode = Mode::ConfirmKill(p.pid, p.name.clone());
                    }
                }
                KeyCode::Char('?') | KeyCode::F(1) | KeyCode::Char('h') => self.mode = Mode::Help,
                _ => {}
            },
        }
        false
    }
}

/// barra estilo htop: `[|||||||     42.0%]`
fn bar_line(th: &Theme, label: &str, pct: f32, width: u16, text: &str) -> Line<'static> {
    let label_w = label.chars().count() as u16 + 1;
    let inner = width.saturating_sub(label_w + 2) as usize;
    let filled = (((pct.clamp(0.0, 100.0) / 100.0) * inner as f32).round() as usize).min(inner);
    let txt_len = text.chars().count();
    let mut body: Vec<char> = (0..inner).map(|i| if i < filled { '|' } else { ' ' }).collect();
    if inner > txt_len {
        for (i, ch) in text.chars().enumerate() {
            body[inner - txt_len + i] = ch;
        }
    }
    Line::from(vec![
        Span::styled(format!("{label} "), Style::default().fg(c(th.accent)).bold()),
        Span::styled("[", Style::default().fg(c(th.dim))),
        Span::styled(body[..filled].iter().collect::<String>(), Style::default().fg(c(th.heat(pct)))),
        Span::styled(body[filled..].iter().collect::<String>(), Style::default().fg(c(th.dim))),
        Span::styled("]", Style::default().fg(c(th.dim))),
    ])
}

fn block(th: &Theme, title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(c(th.dim)))
        .title(Span::styled(format!(" {title} "), Style::default().fg(c(th.accent)).bold()))
}

fn ui(f: &mut Frame, app: &mut App) {
    f.render_widget(Block::default().style(Style::default().bg(c(app.th.bg)).fg(c(app.th.text))), f.area());

    let ncpu = app.mon.stats.cores.len();
    let cols = if ncpu > 16 { 4 } else { 2 };
    let info_lines = 8 + app.mon.stats.gpus.len() as u16;
    let header_h = (ncpu.div_ceil(cols) as u16).max(info_lines) + 2;
    let graphs_h = if app.cfg.show_graphs { 10 } else { 0 };

    let [header, graphs, body, footer] = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Length(graphs_h),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(f.area());

    draw_header(f, app, header, cols);
    if app.cfg.show_graphs {
        draw_graphs(f, app, graphs);
    }
    draw_procs(f, app, body);
    draw_footer(f, app, footer);

    match &app.mode {
        Mode::Help => draw_help(f, &app.th),
        Mode::ConfirmKill(pid, name) => draw_confirm(f, &app.th, *pid, name),
        _ => {}
    }
}

fn draw_header(f: &mut Frame, app: &App, area: Rect, cols: usize) {
    let th = &app.th;
    let s = &app.mon.stats;
    let [cpu_a, info_a] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);

    let b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(c(th.dim)))
        .title(Line::from(vec![
            Span::styled(" CPU ", Style::default().fg(c(th.accent)).bold()),
            Span::styled(format!("{:.0}% ", s.cpu_total), Style::default().fg(c(th.heat(s.cpu_total))).bold()),
        ]))
        .title(
            Line::from(Span::styled(
                format!(" {} · {} hilos · {:.1} GHz ", s.cpu_model, s.cores.len(), s.cpu_freq as f32 / 1000.0),
                Style::default().fg(c(th.dim)),
            ))
            .right_aligned(),
        );
    let inner = b.inner(cpu_a);
    f.render_widget(b, cpu_a);
    let col_areas = Layout::horizontal(vec![Constraint::Ratio(1, cols as u32); cols]).split(inner);
    let per_col = s.cores.len().div_ceil(cols).max(1);
    for (ci, ca) in col_areas.iter().enumerate() {
        let lines: Vec<Line> = s
            .cores
            .iter()
            .enumerate()
            .skip(ci * per_col)
            .take(per_col)
            .map(|(i, &u)| bar_line(th, &format!("{i:>2}"), u, ca.width, &format!("{u:.1}%")))
            .collect();
        f.render_widget(Paragraph::new(lines), *ca);
    }
    // si sobra lugar abajo, barra de uso total a todo lo ancho
    if inner.height as usize > per_col + 1 {
        let r = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
        f.render_widget(
            Paragraph::new(bar_line(th, "Tot", s.cpu_total, r.width, &format!("{:.1}%", s.cpu_total))),
            r,
        );
    }

    let b = block(th, &s.host);
    let inner = b.inner(info_a);
    f.render_widget(b, info_a);
    let w = inner.width;
    let k = |t: &str| Span::styled(format!("{t:<7}"), Style::default().fg(c(th.accent)).bold());
    let v = |t: String| Span::styled(t, Style::default().fg(c(th.text)));
    let load_pct = s.load.0 as f32 * 100.0 / s.cores.len().max(1) as f32;
    let mut lines = vec![
        bar_line(th, "Mem", s.mem_pct(), w, &format!("{}/{}", human(s.mem_used), human(s.mem_total))),
        bar_line(th, "Swp", s.swap_pct(), w, &format!("{}/{}", human(s.swap_used), human(s.swap_total))),
        Line::from(vec![
            k("Carga"),
            Span::styled(format!("{:.2} ", s.load.0), Style::default().fg(c(th.heat(load_pct))).bold()),
            v(format!("{:.2} {:.2}", s.load.1, s.load.2)),
        ]),
        Line::from(vec![
            k("Tareas"),
            v(format!("{} ", s.tasks)),
            Span::styled(format!("{} corriendo", s.running), Style::default().fg(c(th.ok))),
        ]),
        Line::from(vec![k("Uptime"), v(hms(s.uptime))]),
        Line::from(vec![
            k("Red"),
            Span::styled(format!("v {}/s ", human(s.rx_rate as u64)), Style::default().fg(c(th.ok))),
            Span::styled(format!("^ {}/s", human(s.tx_rate as u64)), Style::default().fg(c(th.warn))),
        ]),
    ];
    lines.push(Line::from(vec![
        k("Disco"),
        Span::styled(format!("r {}/s ", human(s.disk_read_rate as u64)), Style::default().fg(c(th.accent))),
        Span::styled(format!("w {}/s ", human(s.disk_write_rate as u64)), Style::default().fg(c(th.mem))),
        Span::styled(format!("({:.0}% usado)", s.disk_pct()), Style::default().fg(c(th.dim))),
    ]));
    for g in &s.gpus {
        let mut sp = vec![k("GPU")];
        sp.push(Span::styled(format!("{} ", g.short_name()), Style::default().fg(c(th.text))));
        if g.asleep {
            sp.push(Span::styled("dormida", Style::default().fg(c(th.dim))));
        } else {
            let b = g.busy.unwrap_or(0.0);
            sp.push(Span::styled(format!("{b:.0}% "), Style::default().fg(c(th.heat(b))).bold()));
            sp.push(Span::styled(
                format!("vram {}/{}", human(g.vram_used), human(g.vram_total)),
                Style::default().fg(c(th.dim)),
            ));
            if let Some(t) = g.temp {
                sp.push(Span::styled(format!(" {t:.0}°C"), Style::default().fg(c(th.dim))));
            }
        }
        lines.push(Line::from(sp));
    }
    if let Some(t) = s.temp_max {
        lines.push(Line::from(vec![
            k("Temp"),
            Span::styled(format!("{t:.0}°C max"), Style::default().fg(c(th.heat(t))).bold()),
        ]));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_graphs(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.th;
    let m = &app.mon;
    let s = &m.stats;
    let rows = Layout::vertical([Constraint::Ratio(1, 2); 2]).split(area);
    let top = Layout::horizontal([Constraint::Ratio(1, 3); 3]).split(rows[0]);
    let bot = Layout::horizontal([Constraint::Ratio(1, 3); 3]).split(rows[1]);

    let pct = |q: &std::collections::VecDeque<f32>| q.iter().map(|&v| v as u64).collect::<Vec<u64>>();
    let sum = |a: &std::collections::VecDeque<f64>, b: &std::collections::VecDeque<f64>| {
        a.iter().zip(b.iter()).map(|(x, y)| (x + y) as u64).collect::<Vec<u64>>()
    };
    let gpu_i = s.gpus.iter().position(|g| !g.asleep);
    let empty = std::collections::VecDeque::new();
    let gh = gpu_i.map(|i| &m.hist.gpu[i]).unwrap_or(&empty);
    let vh = gpu_i.map(|i| &m.hist.vram[i]).unwrap_or(&empty);
    let g = gpu_i.map(|i| &s.gpus[i]);

    let items: [(Rect, String, Vec<u64>, Option<u64>, Rgb); 6] = [
        (top[0], format!("CPU {:.0}%", s.cpu_total), pct(&m.hist.cpu), Some(100), th.accent),
        (top[1], format!("Memoria {:.0}%  {}/{}", s.mem_pct(), human(s.mem_used), human(s.mem_total)), pct(&m.hist.mem), Some(100), th.mem),
        (top[2], format!("Red v{}/s ^{}/s", human(s.rx_rate as u64), human(s.tx_rate as u64)), sum(&m.hist.rx, &m.hist.tx), None, th.ok),
        (bot[0], format!("Disco r{}/s w{}/s", human(s.disk_read_rate as u64), human(s.disk_write_rate as u64)), sum(&m.hist.disk_r, &m.hist.disk_w), None, th.warn),
        (bot[1], match g {
            Some(g) => format!("GPU {} {:.0}%", g.short_name(), g.busy.unwrap_or(0.0)),
            None => "GPU -".into(),
        }, pct(gh), Some(100), th.accent),
        (bot[2], match g {
            Some(g) => format!("VRAM {}/{}", human(g.vram_used), human(g.vram_total)),
            None => "VRAM -".into(),
        }, pct(vh), Some(100), th.mem),
    ];
    for (rect, title, data, max, color) in items {
        let blk = block(th, &title);
        let w = blk.inner(rect).width as usize;
        let data: Vec<u64> = data.iter().rev().take(w).rev().copied().collect();
        let mut sp = Sparkline::default().block(blk).data(&data).style(Style::default().fg(c(color)));
        if let Some(mx) = max {
            sp = sp.max(mx);
        }
        f.render_widget(sp, rect);
    }
}

fn state_color(th: &Theme, s: State) -> Rgb {
    match s {
        State::Run => th.ok,
        State::Stop | State::Disk => th.warn,
        State::Zombie => th.hot,
        _ => th.dim,
    }
}

fn draw_procs(f: &mut Frame, app: &mut App, area: Rect) {
    let th = app.th.clone();
    let accent = Style::default().fg(c(th.accent)).bold();
    let arrow = if app.reverse { "^" } else { "v" };
    let hdr = |name: &'static str, s: SortBy| {
        if app.sort == s {
            Cell::from(format!("{name}{arrow}")).style(Style::default().fg(c(th.bg)).bg(c(th.accent)).bold())
        } else {
            Cell::from(name).style(accent)
        }
    };
    let header = Row::new(vec![
        hdr("PID", SortBy::Pid),
        Cell::from("USUARIO").style(accent),
        hdr("CPU%", SortBy::Cpu),
        hdr("MEM%", SortBy::Mem),
        Cell::from("RES").style(accent),
        Cell::from("S").style(accent),
        hdr("TIEMPO", SortBy::Time),
        hdr(if app.cfg.show_cmd { "COMANDO" } else { "NOMBRE" }, SortBy::Name),
    ]);

    let show_cmd = app.cfg.show_cmd;
    let rows = app.view.iter().map(|&i| {
        let p = &app.mon.procs[i];
        let label = if show_cmd && !p.cmd.is_empty() { &p.cmd } else { &p.name };
        let cpu_c = if p.cpu < 1.0 { th.dim } else { th.heat(p.cpu) };
        let mem_c = if p.mem_pct < 1.0 { th.dim } else { th.heat(p.mem_pct * 4.0) };
        let user_c = if p.user == "root" { th.hot } else { th.text };
        Row::new(vec![
            Cell::from(format!("{:>7}", p.pid)).style(Style::default().fg(c(th.dim))),
            Cell::from(p.user.clone()).style(Style::default().fg(c(user_c))),
            Cell::from(format!("{:>5.1}", p.cpu)).style(Style::default().fg(c(cpu_c))),
            Cell::from(format!("{:>5.1}", p.mem_pct)).style(Style::default().fg(c(mem_c))),
            Cell::from(format!("{:>6}", human(p.mem))).style(Style::default().fg(c(th.text))),
            Cell::from(p.state.letter()).style(Style::default().fg(c(state_color(&th, p.state)))),
            Cell::from(hms(p.time)).style(Style::default().fg(c(th.dim))),
            Cell::from(label.clone()).style(Style::default().fg(c(th.text))),
        ])
    });

    let mut title = format!("Procesos ({})  orden: {}", app.view.len(), app.sort.label());
    if !app.filter.is_empty() {
        title.push_str(&format!("  filtro: \"{}\"", app.filter));
    }
    if app.paused {
        title.push_str("  [PAUSA]");
    }
    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(6),
            Constraint::Length(7),
            Constraint::Length(1),
            Constraint::Length(9),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(block(&th, &title))
    .column_spacing(1)
    .row_highlight_style(Style::default().bg(c(th.sel)).bold())
    .highlight_symbol("> ");
    f.render_stateful_widget(table, area, &mut app.table);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let th = &app.th;
    if app.mode == Mode::Filter {
        let l = Line::from(vec![
            Span::styled(" filtro: ", Style::default().fg(c(th.bg)).bg(c(th.warn)).bold()),
            Span::styled(format!(" {}", app.filter), Style::default().fg(c(th.text))),
            Span::styled("_", Style::default().fg(c(th.warn)).add_modifier(Modifier::SLOW_BLINK)),
            Span::styled("   Enter confirma, Esc limpia", Style::default().fg(c(th.dim))),
        ]);
        f.render_widget(Paragraph::new(l), area);
        return;
    }
    if let Some((m, t)) = &app.status_msg {
        if t.elapsed() < Duration::from_secs(3) {
            f.render_widget(
                Paragraph::new(Span::styled(format!(" {m}"), Style::default().fg(c(th.warn)).bold())),
                area,
            );
            return;
        }
    }
    let keys = [
        ("?", "ayuda"),
        ("/", "filtrar"),
        ("s", "orden"),
        ("x", "matar"),
        ("T", "tema"),
        ("v", "gráficos"),
        ("p", "pausa"),
        ("q", "salir"),
    ];
    let mut spans = vec![];
    for (k, d) in keys {
        spans.push(Span::styled(format!(" {k} "), Style::default().fg(c(th.bg)).bg(c(th.accent)).bold()));
        spans.push(Span::styled(format!("{d} "), Style::default().fg(c(th.dim))));
    }
    spans.push(Span::styled(format!(" [{}]", app.cfg.theme), Style::default().fg(c(th.dim))));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

fn draw_help(f: &mut Frame, th: &Theme) {
    let a = centered(f.area(), 58, 22);
    f.render_widget(Clear, a);
    let items = [
        ("j/k  flechas", "moverse"),
        ("PgUp/PgDn g/G", "saltar / inicio / fin"),
        ("/  F3", "filtrar (nombre, cmd, user, pid)"),
        ("s  Tab  F6", "ciclar orden"),
        ("c m t i n", "ordenar cpu/mem/tiempo/pid/nombre"),
        ("r", "invertir orden"),
        ("l", "mostrar comando completo"),
        ("x  F9  Supr", "matar proceso (TERM, 9 = KILL)"),
        ("T", "cambiar tema"),
        ("v", "mostrar/ocultar gráficos"),
        ("+ / -", "refresco más lento / más rápido"),
        ("p  espacio", "pausar refresco"),
        ("q  Esc  F10", "salir"),
    ];
    let mut lines = vec![Line::raw("")];
    for (k, d) in items {
        lines.push(Line::from(vec![
            Span::styled(format!("  {k:<16}"), Style::default().fg(c(th.accent)).bold()),
            Span::styled(d, Style::default().fg(c(th.text))),
        ]));
    }
    lines.push(Line::raw(""));
    if let Some(p) = Config::path() {
        lines.push(Line::styled(format!("  config: {}", p.display()), Style::default().fg(c(th.dim))));
    }
    lines.push(Line::styled("  cualquier tecla para cerrar", Style::default().fg(c(th.dim))));
    f.render_widget(
        Paragraph::new(lines).block(block(th, "rtop - ayuda")).style(Style::default().bg(c(th.panel))),
        a,
    );
}

fn draw_confirm(f: &mut Frame, th: &Theme, pid: u32, name: &str) {
    let a = centered(f.area(), 50, 7);
    f.render_widget(Clear, a);
    let lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::raw("  Matar "),
            Span::styled(name.to_string(), Style::default().fg(c(th.hot)).bold()),
            Span::raw(format!(" ({pid})?")),
        ]),
        Line::raw(""),
        Line::styled("  y/Enter = TERM   9 = KILL   otra = cancelar", Style::default().fg(c(th.dim))),
    ];
    f.render_widget(
        Paragraph::new(lines)
            .block(block(th, "confirmar").border_style(Style::default().fg(c(th.hot))))
            .style(Style::default().bg(c(th.panel)).fg(c(th.text))),
        a,
    );
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("rtop {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if args.iter().any(|a| a == "--gui" || a == "-g") {
        // atajo: `rtop --gui` lanza la versión con ventana
        return std::process::Command::new("rtop-gui").spawn().map(|_| ());
    }

    let mut app = App::new();

    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(out))?;

    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        hook(info);
    }));

    let res = run(&mut term, &mut app);

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
    term.show_cursor()?;
    res
}

fn run(term: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    let mut last_tick = Instant::now();
    loop {
        term.draw(|f| ui(f, app))?;
        let timeout = app.tick().saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press && app.on_key(k) {
                    return Ok(());
                }
            }
        }
        if last_tick.elapsed() >= app.tick() {
            if !app.paused {
                app.refresh();
            }
            last_tick = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    /// `cargo test -p rtop -- --nocapture` dibuja un frame en texto.
    #[test]
    fn render_frame() {
        let mut app = App::new();
        std::thread::sleep(Duration::from_millis(1100));
        app.refresh();
        let mut term = Terminal::new(TestBackend::new(150, 48)).unwrap();
        term.draw(|f| ui(f, &mut app)).unwrap();
        let buf = term.backend().buffer();
        for y in 0..buf.area.height {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            println!("{line}");
        }
    }
}
