//! rtop: monitor del sistema en la terminal (`rtop --gui` abre la ventana).

mod app;
mod input;
mod ui;

use std::{io, time::Instant};

use crossterm::{
    event::{self, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use app::App;
use ui::ui;

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
        if event::poll(timeout)?
            && let Event::Key(k) = event::read()?
                && k.kind == KeyEventKind::Press && app.on_key(k) {
                    return Ok(());
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
    use std::time::Duration;

    use ratatui::backend::TestBackend;

    use super::*;

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
