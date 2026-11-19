//! Dibujo de la TUI: encabezado con barras, gráficos, tabla y ventanas.

use std::time::Duration;

use ratatui::{
    prelude::*,
    widgets::{Block, BorderType, Borders, Cell, Clear, Paragraph, Row, Sparkline, Table},
};
use rtop_core::{
    fmt::{hms, human},
    Config, Rgb, SortBy, State, Theme,
};

use crate::app::{App, Mode};

fn c(rgb: Rgb) -> Color {
    Color::Rgb(rgb.0, rgb.1, rgb.2)
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

/// Un gráfico chico: dónde, título, datos, tope (None = automático) y color.
type Spark = (Rect, String, Vec<u64>, Option<u64>, Rgb);

pub fn ui(f: &mut Frame, app: &mut App) {
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

    let items: [Spark; 6] = [
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
    if let Some((m, t)) = &app.status_msg
        && t.elapsed() < Duration::from_secs(3) {
            f.render_widget(
                Paragraph::new(Span::styled(format!(" {m}"), Style::default().fg(c(th.warn)).bold())),
                area,
            );
            return;
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
