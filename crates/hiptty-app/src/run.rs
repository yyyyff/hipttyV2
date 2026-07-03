use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use hiptty_adapter::ForumClient;
use hiptty_render::clear_terminal_graphics;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use ratatui_image::picker::Picker;
use tokio::sync::mpsc;

use crate::app::App;
use crate::draw::draw;
use crate::event::{handle_key, handle_worker_response, startup};
use crate::worker::{spawn_worker, WorkerRequest, WorkerResponse};

pub async fn run<C: ForumClient + Send + Sync + 'static>(
    mut app: App,
    client: C,
) -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    // Terminal image protocol is probed exactly once here (Picker::clone() reuses it).
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    app.init_images(picker);
    let result = run_loop(&mut terminal, &mut app, client).await;
    teardown_terminal(&mut terminal)?;
    result
}

async fn run_loop<C: ForumClient + Send + Sync + 'static>(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    client: C,
) -> io::Result<()> {
    let (worker_tx, worker_rx) = mpsc::unbounded_channel::<WorkerRequest>();
    let (response_tx, mut response_rx) = mpsc::unbounded_channel::<WorkerResponse>();
    spawn_worker(client, worker_rx, response_tx);
    startup(app, &worker_tx);

    let tick_rate = Duration::from_millis(50);
    let mut last_tick = std::time::Instant::now();

    loop {
        terminal.draw(|frame| draw(frame, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    handle_key(app, key, &worker_tx);
                }
                Event::Resize(_, _) => {
                    app.sync_feed_scroll();
                    if app.page == crate::app::Page::ThreadDetail {
                        app.sync_detail_scroll();
                    }
                }
                _ => {}
            }
        }

        while let Ok(response) = response_rx.try_recv() {
            handle_worker_response(app, response, &worker_tx);
        }

        if last_tick.elapsed() >= tick_rate {
            app.tick = app.tick.wrapping_add(1);
            last_tick = std::time::Instant::now();
        }

        if app.quit {
            break;
        }
    }

    Ok(())
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    clear_terminal_graphics()?;
    crossterm::terminal::enable_raw_mode()?;
    crossterm::execute!(
        io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    clear_terminal_graphics()?;
    let backend = CrosstermBackend::new(io::stdout());
    Terminal::new(backend)
}

fn teardown_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    crossterm::terminal::disable_raw_mode()?;
    Ok(())
}
