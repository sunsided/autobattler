//! Terminal UI for watching a fight play out.

pub mod app;
#[allow(dead_code)] // wired into the event loop in a later commit
pub mod menu;
pub mod view;

use app::{App, Key};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::crossterm::{cursor, execute};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, stdout, Stdout};
use std::time::{Duration, Instant};
use view::FightView;

const POLL: Duration = Duration::from_millis(16);

/// Decodes a key press; releases, repeats and unbound keys map to `None`.
fn map_key(event: KeyEvent) -> Option<Key> {
    if event.kind != KeyEventKind::Press {
        return None;
    }
    match event.code {
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => Some(Key::Quit),
        KeyCode::Char(' ') => Some(Key::TogglePause),
        KeyCode::Left | KeyCode::Char('h') => Some(Key::Back),
        KeyCode::Right | KeyCode::Char('l') => Some(Key::Forward),
        KeyCode::Char('+' | '=') => Some(Key::Faster),
        KeyCode::Char('-') => Some(Key::Slower),
        KeyCode::Char('t') => Some(Key::ToggleTactician),
        KeyCode::Char('q') | KeyCode::Esc => Some(Key::Quit),
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

/// Plays the fight in the terminal until the user quits.
pub fn run(fight: FightView, explain: bool) -> Result<(), TuiError> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        previous(info);
    }));

    let (_guard, mut terminal) = setup().map_err(TuiError::Setup)?;
    play(&mut terminal, &fight, explain).map_err(TuiError::Terminal)
}

/// Enters raw mode and the alternate screen; the guard leaves them again.
fn setup() -> io::Result<(TerminalGuard, Terminal<CrosstermBackend<Stdout>>)> {
    enable_raw_mode()?;
    let guard = TerminalGuard;
    execute!(stdout(), EnterAlternateScreen, cursor::Hide)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    Ok((guard, terminal))
}

/// Runs the event loop until the user quits.
fn play(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    fight: &FightView,
    explain: bool,
) -> io::Result<()> {
    let rounds = fight.outcome.timeline.iter().map(|e| e.round).collect();
    let mut app = App::new(rounds, explain);
    let mut dirty = true;
    let mut last = Instant::now();
    while !app.should_quit() {
        if event::poll(POLL)? {
            match event::read()? {
                Event::Key(key) => {
                    if let Some(key) = map_key(key) {
                        app.handle(key);
                        dirty = true;
                    }
                }
                Event::Resize(..) => dirty = true,
                _ => {}
            }
        }
        let now = Instant::now();
        let (shown, flashing) = (app.shown(), app.flashing());
        app.tick(now - last);
        last = now;
        dirty |= app.shown() != shown || app.flashing() != flashing;
        if dirty {
            terminal.draw(|frame| view::draw(frame, &app, fight))?;
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
            (press(KeyCode::Char(' ')), Some(Key::TogglePause)),
            (press(KeyCode::Left), Some(Key::Back)),
            (press(KeyCode::Char('h')), Some(Key::Back)),
            (press(KeyCode::Right), Some(Key::Forward)),
            (press(KeyCode::Char('l')), Some(Key::Forward)),
            (press(KeyCode::Char('+')), Some(Key::Faster)),
            (press(KeyCode::Char('=')), Some(Key::Faster)),
            (press(KeyCode::Char('-')), Some(Key::Slower)),
            (press(KeyCode::Char('t')), Some(Key::ToggleTactician)),
            (press(KeyCode::Char('q')), Some(Key::Quit)),
            (press(KeyCode::Esc), Some(Key::Quit)),
            (
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Some(Key::Quit),
            ),
            (press(KeyCode::Char('c')), None),
            (press(KeyCode::Char('x')), None),
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
