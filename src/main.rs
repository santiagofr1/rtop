use std::{
    collections::VecDeque,
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
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Sparkline, Table, TableState,
    },
};
use sysinfo::{
    Components, Networks, Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, Signal,
    System, Users,
};

// ---------- paleta ----------
const BG_SEL: Color = Color::Rgb(45, 55, 80);
const ACCENT: Color = Color::Rgb(130, 170, 255);
const DIM: Color = Color::Rgb(110, 115, 130);
const OK: Color = Color::Rgb(120, 220, 140);
const WARN: Color = Color::Rgb(240, 200, 90);
const HOT: Color = Color::Rgb(240, 100, 100);

fn heat(p: f32) -> Color {
    if p < 50.0 {
        OK
    } else if p < 80.0 {
        WARN
    } else {
        HOT
    }
}

const TICK: Duration = Duration::from_millis(1000);
const HIST: usize = 240;

#[derive(Clone, Copy, PartialEq)]
enum SortBy {
    Cpu,
    Mem,
    Pid,
    Name,
    Time,
}

impl SortBy {
    fn next(self) -> Self {
        match self {
            SortBy::Cpu => SortBy::Mem,
            SortBy::Mem => SortBy::Time,
            SortBy::Time => SortBy::Pid,
            SortBy::Pid => SortBy::Name,
            SortBy::Name => SortBy::Cpu,
        }
    }
    fn label(self) -> &'static str {
        match self {
            SortBy::Cpu => "CPU%",
            SortBy::Mem => "MEM%",
            SortBy::Pid => "PID",
            SortBy::Name => "NOMBRE",
            SortBy::Time => "TIEMPO",
        }
    }
}

#[derive(PartialEq)]
enum Mode {
    Normal,
    Filter,
    ConfirmKill(Pid, String),
    Help,
}

struct ProcRow {
    pid: Pid,
    user: String,
    cpu: f32,
    mem: u64,
    status: ProcessStatus,
    time: u64,
    name: String,
    cmd: String,
}

struct App {
    sys: System,
    users: Users,
    nets: Networks,
    comps: Components,
    rows: Vec<ProcRow>,
    table: TableState,
    sort: SortBy,
    reverse: bool,
    filter: String,
    mode: Mode,
    paused: bool,
    show_cmd: bool,
    cpu_hist: VecDeque<u64>,
    mem_hist: VecDeque<u64>,
    rx_rate: f64,
    tx_rate: f64,
    last: Instant,
    status_msg: Option<(String, Instant)>,
}

impl App {
    fn new() -> Self {
        let mut sys = System::new_all();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        sys.refresh_all();
        let mut app = App {
            sys,
            users: Users::new_with_refreshed_list(),
            nets: Networks::new_with_refreshed_list(),
            comps: Components::new_with_refreshed_list(),
            rows: Vec::new(),
            table: TableState::default().with_selected(0),
            sort: SortBy::Cpu,
            reverse: false,
            filter: String::new(),
            mode: Mode::Normal,
            paused: false,
            show_cmd: false,
            cpu_hist: VecDeque::with_capacity(HIST),
            mem_hist: VecDeque::with_capacity(HIST),
            rx_rate: 0.0,
            tx_rate: 0.0,
            last: Instant::now(),
            status_msg: None,
        };
        push(&mut app.cpu_hist, app.sys.global_cpu_usage() as u64);
        let mt = app.sys.total_memory().max(1);
        push(&mut app.mem_hist, app.sys.used_memory() * 100 / mt);
        app.rebuild_rows();
        app
    }

    fn refresh(&mut self) {
        let dt = self.last.elapsed().as_secs_f64().max(0.001);
        self.last = Instant::now();
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything(),
        );
        self.nets.refresh(true);
        self.comps.refresh(false);

        let (rx, tx) = self
            .nets
            .iter()
            .filter(|(n, _)| *n != "lo")
            .fold((0u64, 0u64), |(r, t), (_, d)| (r + d.received(), t + d.transmitted()));
        self.rx_rate = rx as f64 / dt;
        self.tx_rate = tx as f64 / dt;

        push(&mut self.cpu_hist, self.sys.global_cpu_usage() as u64);
        let mt = self.sys.total_memory().max(1);
        push(&mut self.mem_hist, self.sys.used_memory() * 100 / mt);

        self.rebuild_rows();
    }

    fn rebuild_rows(&mut self) {
        let sel_pid = self.selected_pid();
        let f = self.filter.to_lowercase();
        let mut rows: Vec<ProcRow> = self
            .sys
            .processes()
            .values()
            .filter(|p| p.thread_kind().is_none())
            .map(|p| {
                let name = p.name().to_string_lossy().into_owned();
                let cmd = p
                    .cmd()
                    .iter()
                    .map(|s| s.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" ");
                let user = p
                    .user_id()
                    .and_then(|u| self.users.get_user_by_id(u))
                    .map(|u| u.name().to_string())
                    .unwrap_or_else(|| "?".into());
                ProcRow {
                    pid: p.pid(),
                    user,
                    cpu: p.cpu_usage(),
                    mem: p.memory(),
                    status: p.status(),
                    time: p.run_time(),
                    name,
                    cmd,
                }
            })
            .filter(|r| {
                f.is_empty()
                    || r.name.to_lowercase().contains(&f)
                    || r.cmd.to_lowercase().contains(&f)
                    || r.user.to_lowercase().contains(&f)
                    || r.pid.to_string().contains(&f)
            })
            .collect();

        rows.sort_by(|a, b| {
            let o = match self.sort {
                SortBy::Cpu => b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal),
                SortBy::Mem => b.mem.cmp(&a.mem),
                SortBy::Time => b.time.cmp(&a.time),
                SortBy::Pid => a.pid.cmp(&b.pid),
                SortBy::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            };
            if self.reverse { o.reverse() } else { o }
        });
        self.rows = rows;

        // mantener seleccionado el mismo proceso si sigue vivo
        let idx = sel_pid
            .and_then(|pid| self.rows.iter().position(|r| r.pid == pid))
            .unwrap_or_else(|| self.table.selected().unwrap_or(0));
        let max = self.rows.len().saturating_sub(1);
        self.table.select(Some(idx.min(max)));
    }

    fn selected_pid(&self) -> Option<Pid> {
        self.table.selected().and_then(|i| self.rows.get(i)).map(|r| r.pid)
    }

    fn move_sel(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let cur = self.table.selected().unwrap_or(0) as isize;
        let max = self.rows.len() as isize - 1;
        self.table.select(Some((cur + delta).clamp(0, max) as usize));
    }

    fn flash(&mut self, msg: impl Into<String>) {
        self.status_msg = Some((msg.into(), Instant::now()));
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
                    self.rebuild_rows();
                }
                KeyCode::Enter => {}
                KeyCode::Backspace => {
                    self.filter.pop();
                    self.rebuild_rows();
                    self.mode = Mode::Filter;
                }
                KeyCode::Char(c) => {
                    self.filter.push(c);
                    self.rebuild_rows();
                    self.mode = Mode::Filter;
                }
                _ => self.mode = Mode::Filter,
            },
            Mode::ConfirmKill(pid, name) => match k.code {
                KeyCode::Char('y') | KeyCode::Char('s') | KeyCode::Enter => {
                    self.kill(pid, &name, Signal::Term)
                }
                KeyCode::Char('9') => self.kill(pid, &name, Signal::Kill),
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
                    self.table.select(Some(self.rows.len().saturating_sub(1)))
                }
                KeyCode::Char('/') | KeyCode::F(3) => self.mode = Mode::Filter,
                KeyCode::Char('s') | KeyCode::F(6) | KeyCode::Tab => {
                    self.sort = self.sort.next();
                    self.rebuild_rows();
                }
                KeyCode::Char('c') => self.set_sort(SortBy::Cpu),
                KeyCode::Char('m') => self.set_sort(SortBy::Mem),
                KeyCode::Char('n') => self.set_sort(SortBy::Name),
                KeyCode::Char('i') => self.set_sort(SortBy::Pid),
                KeyCode::Char('t') => self.set_sort(SortBy::Time),
                KeyCode::Char('r') => {
                    self.reverse = !self.reverse;
                    self.rebuild_rows();
                }
                KeyCode::Char('p') | KeyCode::Char(' ') => {
                    self.paused = !self.paused;
                    self.flash(if self.paused { "pausado" } else { "corriendo" });
                }
                KeyCode::Char('l') => self.show_cmd = !self.show_cmd,
                KeyCode::Char('x') | KeyCode::F(9) | KeyCode::Delete => {
                    if let Some(i) = self.table.selected() {
                        if let Some(r) = self.rows.get(i) {
                            self.mode = Mode::ConfirmKill(r.pid, r.name.clone());
                        }
                    }
                }
                KeyCode::Char('?') | KeyCode::F(1) | KeyCode::Char('h') => self.mode = Mode::Help,
                _ => {}
            },
        }
        false
    }

    fn set_sort(&mut self, s: SortBy) {
        if self.sort == s {
            self.reverse = !self.reverse;
        } else {
            self.sort = s;
            self.reverse = false;
        }
        self.rebuild_rows();
    }

    fn kill(&mut self, pid: Pid, name: &str, sig: Signal) {
        let ok = self
            .sys
            .process(pid)
            .and_then(|p| p.kill_with(sig))
            .unwrap_or(false);
        let s = if sig == Signal::Kill { "SIGKILL" } else { "SIGTERM" };
        if ok {
            self.flash(format!("{s} -> {name} ({pid})"));
        } else {
            self.flash(format!("no pude matar {name} ({pid}) - permisos?"));
        }
    }
}

fn push(q: &mut VecDeque<u64>, v: u64) {
    if q.len() == HIST {
        q.pop_front();
    }
    q.push_back(v);
}

// ---------- formato ----------
fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{b}B")
    } else if v >= 100.0 {
        format!("{v:.0}{}", U[i])
    } else {
        format!("{v:.1}{}", U[i])
    }
}

fn hms(s: u64) -> String {
    let (h, m, s) = (s / 3600, (s % 3600) / 60, s % 60);
    if h >= 24 {
        format!("{}d{:02}h", h / 24, h % 24)
    } else {
        format!("{h:02}:{m:02}:{s:02}")
    }
}

fn status_char(s: ProcessStatus) -> (&'static str, Color) {
    match s {
        ProcessStatus::Run => ("R", OK),
        ProcessStatus::Sleep | ProcessStatus::Idle => ("S", DIM),
        ProcessStatus::Stop | ProcessStatus::Tracing => ("T", WARN),
        ProcessStatus::Zombie | ProcessStatus::Dead => ("Z", HOT),
        ProcessStatus::UninterruptibleDiskSleep => ("D", WARN),
        _ => ("?", DIM),
    }
}

/// barra estilo htop: `[|||||||     42.0%]`
fn bar_line(label: &str, pct: f32, width: u16, text: &str) -> Line<'static> {
    let label_w = label.chars().count() as u16 + 1;
    let inner = width.saturating_sub(label_w + 2) as usize;
    let filled = ((pct.clamp(0.0, 100.0) / 100.0) * inner as f32).round() as usize;
    let txt_len = text.chars().count();
    let mut spans = vec![
        Span::styled(format!("{label} "), Style::default().fg(ACCENT).bold()),
        Span::styled("[", Style::default().fg(DIM)),
    ];
    let mut body: Vec<char> = (0..inner).map(|i| if i < filled { '|' } else { ' ' }).collect();
    if inner > txt_len {
        for (i, c) in text.chars().enumerate() {
            body[inner - txt_len + i] = c;
        }
    }
    let fill: String = body[..filled.min(inner)].iter().collect();
    let rest: String = body[filled.min(inner)..].iter().collect();
    spans.push(Span::styled(fill, Style::default().fg(heat(pct))));
    spans.push(Span::styled(rest, Style::default().fg(Color::Gray)));
    spans.push(Span::styled("]", Style::default().fg(DIM)));
    Line::from(spans)
}

// ---------- UI ----------
fn ui(f: &mut Frame, app: &mut App) {
    let ncpu = app.sys.cpus().len();
    let cols = if ncpu > 16 { 4 } else { 2 };
    let cpu_rows = ncpu.div_ceil(cols) as u16;
    let header_h = cpu_rows.max(6) + 2;

    let [header, graphs, body, footer] = Layout::vertical([
        Constraint::Length(header_h),
        Constraint::Length(5),
        Constraint::Min(5),
        Constraint::Length(1),
    ])
    .areas(f.area());

    draw_header(f, app, header, cols);
    draw_graphs(f, app, graphs);
    draw_procs(f, app, body);
    draw_footer(f, app, footer);

    match &app.mode {
        Mode::Help => draw_help(f),
        Mode::ConfirmKill(pid, name) => draw_confirm(f, *pid, name),
        _ => {}
    }
}

fn block(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).bold(),
        ))
}

fn draw_header(f: &mut Frame, app: &App, area: Rect, cols: usize) {
    let [cpu_a, info_a] =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).areas(area);

    // CPUs
    let brand = app
        .sys
        .cpus()
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_default();
    let b = block(&format!("CPU  {brand}"));
    let inner = b.inner(cpu_a);
    f.render_widget(b, cpu_a);
    let col_areas = Layout::horizontal(vec![Constraint::Ratio(1, cols as u32); cols]).split(inner);
    let cpus = app.sys.cpus();
    let per_col = cpus.len().div_ceil(cols);
    for (ci, ca) in col_areas.iter().enumerate() {
        let lines: Vec<Line> = cpus
            .iter()
            .enumerate()
            .skip(ci * per_col)
            .take(per_col)
            .map(|(i, c)| {
                let u = c.cpu_usage();
                bar_line(&format!("{i:>2}"), u, ca.width, &format!("{u:.1}%"))
            })
            .collect();
        f.render_widget(Paragraph::new(lines), *ca);
    }

    // info
    let b = block(&System::host_name().unwrap_or_else(|| "sistema".into()));
    let inner = b.inner(info_a);
    f.render_widget(b, info_a);
    let w = inner.width;
    let (mt, mu) = (app.sys.total_memory(), app.sys.used_memory());
    let (st, su) = (app.sys.total_swap(), app.sys.used_swap());
    let mp = if mt > 0 { mu as f32 * 100.0 / mt as f32 } else { 0.0 };
    let sp = if st > 0 { su as f32 * 100.0 / st as f32 } else { 0.0 };
    let la = System::load_average();
    let running = app
        .rows
        .iter()
        .filter(|r| r.status == ProcessStatus::Run)
        .count();
    let temp = app
        .comps
        .iter()
        .filter_map(|c| c.temperature())
        .fold(f32::NAN, f32::max);
    let k = |s: &str| Span::styled(format!("{s:<7}"), Style::default().fg(ACCENT).bold());
    let v = |s: String| Span::styled(s, Style::default().fg(Color::White));
    let mut lines = vec![
        bar_line("Mem", mp, w, &format!("{}/{}", human(mu), human(mt))),
        bar_line("Swp", sp, w, &format!("{}/{}", human(su), human(st))),
        Line::from(vec![
            k("Carga"),
            Span::styled(format!("{:.2} ", la.one), Style::default().fg(heat((la.one as f32 / app.sys.cpus().len().max(1) as f32) * 100.0)).bold()),
            v(format!("{:.2} {:.2}", la.five, la.fifteen)),
        ]),
        Line::from(vec![
            k("Tareas"),
            v(format!("{} ", app.rows.len())),
            Span::styled(format!("{running} corriendo"), Style::default().fg(OK)),
        ]),
        Line::from(vec![k("Uptime"), v(hms(System::uptime()))]),
        Line::from(vec![
            k("Red"),
            Span::styled(format!("v {}/s ", human(app.rx_rate as u64)), Style::default().fg(OK)),
            Span::styled(format!("^ {}/s", human(app.tx_rate as u64)), Style::default().fg(WARN)),
        ]),
    ];
    if !temp.is_nan() {
        lines.push(Line::from(vec![
            k("Temp"),
            Span::styled(format!("{temp:.0}°C max"), Style::default().fg(heat(temp)).bold()),
        ]));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_graphs(f: &mut Frame, app: &App, area: Rect) {
    let [a, b] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);
    for (rect, hist, title, color) in [
        (a, &app.cpu_hist, "CPU total", ACCENT),
        (b, &app.mem_hist, "Memoria", Color::Rgb(200, 140, 255)),
    ] {
        let blk = block(&format!(
            "{title} {}%",
            hist.back().copied().unwrap_or(0)
        ));
        let w = blk.inner(rect).width as usize;
        let data: Vec<u64> = hist.iter().rev().take(w).rev().copied().collect();
        f.render_widget(
            Sparkline::default()
                .block(blk)
                .data(&data)
                .max(100)
                .style(Style::default().fg(color)),
            rect,
        );
    }
}

fn draw_procs(f: &mut Frame, app: &mut App, area: Rect) {
    let mt = app.sys.total_memory().max(1) as f32;
    let arrow = if app.reverse { "^" } else { "v" };
    let hdr = |name: &'static str, s: SortBy| {
        if app.sort == s {
            Cell::from(format!("{name}{arrow}")).style(Style::default().fg(Color::Black).bg(ACCENT).bold())
        } else {
            Cell::from(name).style(Style::default().fg(ACCENT).bold())
        }
    };
    let header = Row::new(vec![
        hdr("PID", SortBy::Pid),
        Cell::from("USUARIO").style(Style::default().fg(ACCENT).bold()),
        hdr("CPU%", SortBy::Cpu),
        hdr("MEM%", SortBy::Mem),
        Cell::from("RES").style(Style::default().fg(ACCENT).bold()),
        Cell::from("S").style(Style::default().fg(ACCENT).bold()),
        hdr("TIEMPO", SortBy::Time),
        hdr(if app.show_cmd { "COMANDO" } else { "NOMBRE" }, SortBy::Name),
    ]);

    let rows = app.rows.iter().map(|r| {
        let mp = r.mem as f32 * 100.0 / mt;
        let (sc, scol) = status_char(r.status);
        let label = if app.show_cmd && !r.cmd.is_empty() { &r.cmd } else { &r.name };
        let cpu_c = if r.cpu < 1.0 { DIM } else { heat(r.cpu) };
        let mem_c = if mp < 1.0 { DIM } else { heat(mp * 4.0) };
        Row::new(vec![
            Cell::from(format!("{:>7}", r.pid)).style(Style::default().fg(Color::Gray)),
            Cell::from(r.user.clone()).style(Style::default().fg(if r.user == "root" { HOT } else { Color::Rgb(150, 200, 220) })),
            Cell::from(format!("{:>5.1}", r.cpu)).style(Style::default().fg(cpu_c)),
            Cell::from(format!("{:>5.1}", mp)).style(Style::default().fg(mem_c)),
            Cell::from(format!("{:>6}", human(r.mem))),
            Cell::from(sc).style(Style::default().fg(scol)),
            Cell::from(hms(r.time)).style(Style::default().fg(DIM)),
            Cell::from(label.clone()).style(Style::default().fg(Color::White)),
        ])
    });

    let mut title = format!("Procesos ({})  orden: {}", app.rows.len(), app.sort.label());
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
    .block(block(&title))
    .column_spacing(1)
    .row_highlight_style(Style::default().bg(BG_SEL).bold())
    .highlight_symbol("> ");
    f.render_stateful_widget(table, area, &mut app.table);
}

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    if app.mode == Mode::Filter {
        let l = Line::from(vec![
            Span::styled(" filtro: ", Style::default().fg(Color::Black).bg(WARN).bold()),
            Span::raw(format!(" {}", app.filter)),
            Span::styled("_", Style::default().fg(WARN).add_modifier(Modifier::SLOW_BLINK)),
            Span::styled("   Enter confirma, Esc limpia", Style::default().fg(DIM)),
        ]);
        f.render_widget(Paragraph::new(l), area);
        return;
    }
    if let Some((m, t)) = &app.status_msg {
        if t.elapsed() < Duration::from_secs(3) {
            f.render_widget(
                Paragraph::new(Span::styled(format!(" {m}"), Style::default().fg(WARN).bold())),
                area,
            );
            return;
        }
    }
    let keys = [
        ("?", "ayuda"),
        ("/", "filtrar"),
        ("s", "orden"),
        ("r", "invertir"),
        ("l", "cmd"),
        ("x", "matar"),
        ("p", "pausa"),
        ("q", "salir"),
    ];
    let mut spans = vec![];
    for (k, d) in keys {
        spans.push(Span::styled(format!(" {k} "), Style::default().fg(Color::Black).bg(ACCENT).bold()));
        spans.push(Span::styled(format!("{d} "), Style::default().fg(Color::Gray)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

fn draw_help(f: &mut Frame) {
    let a = centered(f.area(), 52, 20);
    f.render_widget(Clear, a);
    let items = [
        ("j/k  flechas", "moverse"),
        ("PgUp/PgDn g/G", "saltar / inicio / fin"),
        ("/  F3", "filtrar (nombre, cmd, user, pid)"),
        ("s  Tab  F6", "ciclar orden"),
        ("c m t i n", "ordenar por cpu/mem/tiempo/pid/nombre"),
        ("r", "invertir orden"),
        ("l", "mostrar comando completo"),
        ("x  F9  Supr", "matar proceso (TERM, 9 = KILL)"),
        ("p  espacio", "pausar refresco"),
        ("q  Esc  F10", "salir"),
    ];
    let mut lines = vec![Line::raw("")];
    for (k, d) in items {
        lines.push(Line::from(vec![
            Span::styled(format!("  {k:<16}"), Style::default().fg(ACCENT).bold()),
            Span::raw(d),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("  cualquier tecla para cerrar", Style::default().fg(DIM)));
    f.render_widget(Paragraph::new(lines).block(block("rtop - ayuda")), a);
}

fn draw_confirm(f: &mut Frame, pid: Pid, name: &str) {
    let a = centered(f.area(), 50, 7);
    f.render_widget(Clear, a);
    let lines = vec![
        Line::raw(""),
        Line::from(vec![
            Span::raw("  Matar "),
            Span::styled(name.to_string(), Style::default().fg(HOT).bold()),
            Span::raw(format!(" ({pid})?")),
        ]),
        Line::raw(""),
        Line::styled("  y/Enter = TERM   9 = KILL   otra = cancelar", Style::default().fg(DIM)),
    ];
    f.render_widget(
        Paragraph::new(lines).block(block("confirmar").border_style(Style::default().fg(HOT))),
        a,
    );
}

fn main() -> io::Result<()> {
    if std::env::args().any(|a| a == "-V" || a == "--version") {
        println!("rtop {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let mut app = App::new();

    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(out))?;

    // restaurar la terminal si algo panica
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
        let timeout = TICK.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press && app.on_key(k) {
                    return Ok(());
                }
            }
        }
        if last_tick.elapsed() >= TICK {
            if !app.paused {
                app.refresh();
            }
            last_tick = Instant::now();
        }
    }
}
