use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
use crate::mouse::handle_mouse;
use crate::worker::{spawn_worker, WorkerRequest, WorkerResponse};

/// UI tick period (logo/toast animation, input poll budget).
const TICK_MS: u64 = 50;
/// Unread PM/notification poll interval in ticks (~30s at 50ms).
const UNREAD_CHECK_TICKS: u64 = 600;

pub async fn run<C: ForumClient + Send + Sync + 'static>(
    mut app: App,
    client: C,
) -> io::Result<()> {
    let mut guard = TerminalGuard::enter()?;
    // Terminal image protocol is probed exactly once here (Picker::clone() reuses it).
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
    app.init_images(picker);
    run_loop(guard.terminal_mut(), &mut app, client).await
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

    let interrupted = Arc::new(AtomicBool::new(false));
    spawn_interrupt_watcher(Arc::clone(&interrupted));

    let tick_rate = Duration::from_millis(TICK_MS);
    let mut last_tick = std::time::Instant::now();

    loop {
        if interrupted.load(Ordering::Relaxed) {
            app.quit = true;
        }

        terminal.draw(|frame| draw(frame, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    handle_key(app, key, &worker_tx);
                }
                Event::Mouse(mouse) => {
                    handle_mouse(app, mouse, &worker_tx);
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
            // Coalesce: never enqueue another check while one is still in the worker queue/flight.
            if app.session.logged_in
                && !app.unread.check_in_flight
                && app.tick.is_multiple_of(UNREAD_CHECK_TICKS)
            {
                app.unread.check_in_flight = true;
                if worker_tx.send(WorkerRequest::CheckUnread).is_err() {
                    app.unread.check_in_flight = false;
                }
            }
        }

        if app.quit {
            break;
        }
    }

    Ok(())
}

fn spawn_interrupt_watcher(interrupted: Arc<AtomicBool>) {
    tokio::spawn(async move {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigterm = match signal(SignalKind::terminate()) {
                Ok(s) => s,
                Err(_) => {
                    // Fall back to ctrl_c only if we cannot register SIGTERM.
                    let _ = tokio::signal::ctrl_c().await;
                    interrupted.store(true, Ordering::Relaxed);
                    return;
                }
            };
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = sigterm.recv() => {}
            }
            interrupted.store(true, Ordering::Relaxed);
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
            interrupted.store(true, Ordering::Relaxed);
        }
    });
}

/// Owns terminal modes and restores them on drop (normal exit, error, or panic unwind).
struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    restored: bool,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        install_panic_hook();
        let terminal = setup_terminal()?;
        Ok(Self {
            terminal,
            restored: false,
        })
    }

    fn terminal_mut(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        &mut self.terminal
    }

    fn restore(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        let _ = teardown_terminal(&mut self.terminal);
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.restore();
    }
}

fn install_panic_hook() {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort restore before the default hook prints the backtrace.
        restore_terminal_stdio();
        original(info);
    }));
}

/// Restore terminal without a `Terminal` handle (panic / partial setup).
fn restore_terminal_stdio() {
    use std::io::Write;
    let mut out = io::stdout();
    let _ = crossterm::execute!(
        out,
        crossterm::event::PopKeyboardEnhancementFlags,
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    );
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = out.flush();
    let _ = clear_terminal_graphics();
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    clear_terminal_graphics()?;
    crossterm::terminal::enable_raw_mode().inspect_err(|_| {
        restore_terminal_stdio();
    })?;
    if let Err(e) = crossterm::execute!(
        io::stdout(),
        crossterm::terminal::EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    ) {
        restore_terminal_stdio();
        return Err(e);
    }
    // Kitty keyboard protocol: lets Ctrl+Enter report as Enter+CONTROL (best-effort).
    let _ = crossterm::execute!(
        io::stdout(),
        crossterm::event::PushKeyboardEnhancementFlags(
            crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | crossterm::event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                | crossterm::event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        )
    );
    if let Err(e) = clear_terminal_graphics() {
        restore_terminal_stdio();
        return Err(e);
    }
    let backend = CrosstermBackend::new(io::stdout());
    match Terminal::new(backend) {
        Ok(t) => Ok(t),
        Err(e) => {
            restore_terminal_stdio();
            Err(e)
        }
    }
}

fn teardown_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let _ = crossterm::execute!(
        terminal.backend_mut(),
        crossterm::event::PopKeyboardEnhancementFlags
    );
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture,
        crossterm::terminal::LeaveAlternateScreen
    )?;
    crossterm::terminal::disable_raw_mode()?;
    let _ = clear_terminal_graphics();
    Ok(())
}
