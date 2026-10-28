//! Terminal UI for watching a fight play out.

pub mod app;
pub mod menu;
pub mod session;
pub mod view;

use menu::Menu;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::{cursor, execute};
use ratatui::{backend::CrosstermBackend, Terminal};
use session::{solve, Input, Session, Solution, Solved};
use std::io::{self, stdout, Stdout};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(16);

/// Decodes a key press; releases, repeats and unbound keys map to `None`.
fn map_key(event: KeyEvent) -> Option<Input> {
    if event.kind != KeyEventKind::Press {
        return None;
    }
    match event.code {
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => Some(Input::Quit),
        KeyCode::Char(' ') => Some(Input::Space),
        KeyCode::Char('h') | KeyCode::Left => Some(Input::Left),
        KeyCode::Char('l') | KeyCode::Right => Some(Input::Right),
        KeyCode::Char('k') | KeyCode::Up => Some(Input::Up),
        KeyCode::Char('j') | KeyCode::Down => Some(Input::Down),
        KeyCode::Enter => Some(Input::Enter),
        KeyCode::Esc => Some(Input::Esc),
        KeyCode::Backspace => Some(Input::Backspace),
        KeyCode::Char(c) => Some(Input::Char(c)),
        _ => None,
    }
}

/// Leaves the alternate screen, disables raw mode and shows the cursor.
///
/// Ignores errors, so it is safe to call repeatedly and from a panic hook.
fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, cursor::Show);
}

/// Restores the terminal when dropped.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

/// Why [`run`] stopped early.
#[derive(Debug)]
pub enum TuiError {
    /// The terminal could not be set up; nothing was drawn.
    Setup(io::Error),
    /// Reading keys or drawing failed after the fight was on screen.
    Terminal(io::Error),
}

/// Runs the menu, lineup and fight screens until the user quits.
///
/// Returns the last fight seen, if any, even when the terminal failed later.
pub fn run(menu: Menu, depth: usize, explain: bool) -> (Option<Solved>, Result<(), TuiError>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));

    let (_guard, mut terminal) = match setup() {
        Ok(parts) => parts,
        Err(error) => return (None, Err(TuiError::Setup(error))),
    };
    let mut session = Session::new(menu, depth, explain);
    let result = play(&mut terminal, &mut session).map_err(TuiError::Terminal);
    (session.into_last(), result)
}

/// Enters raw mode and the alternate screen; the guard leaves them again.
fn setup() -> io::Result<(TerminalGuard, Terminal<CrosstermBackend<Stdout>>)> {
    enable_raw_mode()?;
    let guard = TerminalGuard;
    execute!(stdout(), EnterAlternateScreen, cursor::Hide)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    Ok((guard, terminal))
}

/// Runs the event loop until the session wants to quit.
///
/// Solving happens on detached threads; a result that arrives after the user
/// left its lineup is dropped by the session.
fn play(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    session: &mut Session,
) -> io::Result<()> {
    let (tx, rx) = mpsc::channel::<(u64, Solution)>();
    let mut dirty = true;
    let mut last = Instant::now();
    while !session.should_quit() {
        if event::poll(POLL)? {
            match event::read()? {
                Event::Key(key) => {
                    if let Some(input) = map_key(key) {
                        if let Some(job) = session.handle(input) {
                            let tx = tx.clone();
                            std::thread::spawn(move || {
                                let solution =
                                    solve(&job.conflict, &job.names, job.seed, job.depth);
                                let _ = tx.send((job.generation, solution));
                            });
                        }
                        dirty = true;
                    }
                }
                Event::Resize(..) => dirty = true,
                _ => {}
            }
        }
        while let Ok((generation, solution)) = rx.try_recv() {
            dirty |= session.solved(generation, solution);
        }
        let now = Instant::now();
        dirty |= session.tick(now - last);
        last = now;
        if dirty {
            terminal.draw(|frame| session.draw(frame))?;
            dirty = false;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn map_key_table() {
        let table = [
            (press(KeyCode::Char(' ')), Some(Input::Space)),
            (press(KeyCode::Left), Some(Input::Left)),
            (press(KeyCode::Char('h')), Some(Input::Left)),
            (press(KeyCode::Right), Some(Input::Right)),
            (press(KeyCode::Char('l')), Some(Input::Right)),
            (press(KeyCode::Up), Some(Input::Up)),
            (press(KeyCode::Char('k')), Some(Input::Up)),
            (press(KeyCode::Down), Some(Input::Down)),
            (press(KeyCode::Char('j')), Some(Input::Down)),
            (press(KeyCode::Enter), Some(Input::Enter)),
            (press(KeyCode::Esc), Some(Input::Esc)),
            (press(KeyCode::Backspace), Some(Input::Backspace)),
            (press(KeyCode::Char('b')), Some(Input::Char('b'))),
            (press(KeyCode::Char('q')), Some(Input::Char('q'))),
            (press(KeyCode::Char('7')), Some(Input::Char('7'))),
            (press(KeyCode::Char('+')), Some(Input::Char('+'))),
            (
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Some(Input::Quit),
            ),
            (press(KeyCode::Char('c')), Some(Input::Char('c'))),
            (press(KeyCode::Tab), None),
            (press(KeyCode::F(1)), None),
        ];
        for (event, expected) in table {
            assert_eq!(map_key(event), expected, "{event:?}");
        }
    }

    #[test]
    fn map_key_ignores_releases_and_repeats() {
        for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
            let event = KeyEvent::new_with_kind(KeyCode::Char('q'), KeyModifiers::NONE, kind);
            assert_eq!(map_key(event), None);
        }
    }

    #[test]
    fn restore_is_idempotent() {
        restore();
        restore();
    }
}
