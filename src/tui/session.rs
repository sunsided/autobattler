//! The screen machine: menu, lineup and fight, and the keys that move between them.
//!
//! The session never solves a fight itself. [`Session::handle`] returns a [`Job`]
//! for the event loop to run on a thread, and [`Session::solved`] takes the result.

use crate::conflict::Conflict;
use crate::narration::{Beat, NarrationOptions, Narrator};
use crate::scenario::{self, Fight};
use crate::solver::{Outcome, Solver, SolverStrategy, MAX_ROUNDS};
use crate::tui::app::{App, Key};
use crate::tui::menu::{draw_menu, Menu, MenuKey};
use crate::tui::view::{draw, draw_lineup, FightView, LineupView};
use ratatui::Frame;
use std::time::Duration;

/// A decoded key press, independent of the terminal library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Space,
    Esc,
    Backspace,
    Char(char),
    /// Ctrl-C: quits from every screen.
    Quit,
}

/// Which screen is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenKind {
    Menu,
    Lineup,
    Fight,
}

/// A fight to solve off the UI thread.
pub struct Job {
    /// Hands the result back to the lineup that asked for it.
    pub generation: u64,
    pub conflict: Conflict,
    pub names: Vec<Vec<String>>,
    pub seed: u64,
    pub depth: usize,
}

/// What solving a [`Job`] produces.
pub struct Solution {
    pub outcome: Outcome,
    pub beats: Vec<Beat>,
}

/// A fight with everything the fight screen and the closing summary need.
pub struct Solved {
    pub conflict: Conflict,
    pub names: Vec<Vec<String>>,
    pub outcome: Outcome,
    pub beats: Vec<Beat>,
    pub scenario: String,
    pub seed: u64,
}

/// Solves with `Solver::play(IterativeDeepening(depth), MAX_ROUNDS)`, then narrates
/// with `explain: true` and `seed: Some(seed)`.
pub fn solve(conflict: &Conflict, names: &[Vec<String>], seed: u64, depth: usize) -> Solution {
    let outcome = Solver::play(
        conflict,
        SolverStrategy::IterativeDeepening(depth),
        MAX_ROUNDS,
    );
    let options = NarrationOptions {
        explain: true,
        seed: Some(seed),
    };
    let beats = Narrator::new(names, options).narrate(conflict, &outcome);
    Solution { outcome, beats }
}

enum Screen {
    Menu,
    Lineup {
        generation: u64,
        conflict: Conflict,
        names: Vec<Vec<String>>,
        scenario: String,
        seed: u64,
        intro: String,
        solution: Option<Solution>,
    },
    Fight {
        fight: Solved,
        app: App,
    },
}

/// The menu, the lineup and the fight, and which of them is showing.
pub struct Session {
    menu: Menu,
    depth: usize,
    explain: bool,
    generation: u64,
    screen: Screen,
    last: Option<Solved>,
    quit: bool,
}

impl Session {
    /// Starts on `menu`; `explain` seeds the tactician toggle of the first fight.
    pub fn new(menu: Menu, depth: usize, explain: bool) -> Self {
        Self {
            menu,
            depth,
            explain,
            generation: 0,
            screen: Screen::Menu,
            last: None,
            quit: false,
        }
    }

    pub fn screen(&self) -> ScreenKind {
        match self.screen {
            Screen::Menu => ScreenKind::Menu,
            Screen::Lineup { .. } => ScreenKind::Lineup,
            Screen::Fight { .. } => ScreenKind::Fight,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Applies a key; returns the job to solve when the menu starts a lineup.
    pub fn handle(&mut self, input: Input) -> Option<Job> {
        if input == Input::Quit {
            self.quit = true;
            return None;
        }
        match self.screen {
            Screen::Menu => self.handle_menu(input),
            Screen::Lineup { .. } => {
                self.handle_lineup(input);
                None
            }
            Screen::Fight { .. } => {
                self.handle_fight(input);
                None
            }
        }
    }

    fn handle_menu(&mut self, input: Input) -> Option<Job> {
        let key = match input {
            Input::Up => MenuKey::Up,
            Input::Down => MenuKey::Down,
            Input::Left => MenuKey::Left,
            Input::Right => MenuKey::Right,
            Input::Backspace => MenuKey::Backspace,
            Input::Char('r') => MenuKey::Reroll,
            Input::Char(c) => match c.to_digit(10) {
                // `to_digit(10)` is at most 9.
                Some(digit) => MenuKey::Digit(digit as u8),
                None => {
                    self.quit |= c == 'q';
                    return None;
                }
            },
            Input::Enter => return Some(self.start_lineup()),
            Input::Esc => {
                self.quit = true;
                return None;
            }
            Input::Space | Input::Quit => return None,
        };
        self.menu.handle(key);
        None
    }

    fn start_lineup(&mut self) -> Job {
        let (setup, roster, seed) = self.menu.selection();
        let Fight { conflict, names } = scenario::build(setup, roster, seed);
        let intro = Narrator::new(
            &names,
            NarrationOptions {
                explain: self.explain,
                seed: Some(seed),
            },
        )
        .intro(&conflict);
        self.generation += 1;
        let job = Job {
            generation: self.generation,
            conflict: conflict.clone(),
            names: names.clone(),
            seed,
            depth: self.depth,
        };
        self.screen = Screen::Lineup {
            generation: self.generation,
            conflict,
            names,
            scenario: setup.label(),
            seed,
            intro,
            solution: None,
        };
        job
    }

    fn handle_lineup(&mut self, input: Input) {
        match input {
            Input::Space | Input::Enter => self.begin(),
            Input::Esc | Input::Char('b') => self.screen = Screen::Menu,
            Input::Char('q') => self.quit = true,
            _ => {}
        }
    }

    /// Moves to the fight if the lineup has its solution.
    fn begin(&mut self) {
        let Screen::Lineup { solution, .. } = &mut self.screen else {
            return;
        };
        if solution.is_none() {
            return;
        }
        let Screen::Lineup {
            conflict,
            names,
            scenario,
            seed,
            solution: Some(Solution { outcome, beats }),
            ..
        } = std::mem::replace(&mut self.screen, Screen::Menu)
        else {
            unreachable!("checked above");
        };
        let rounds = outcome.timeline.iter().map(|e| e.round).collect();
        let app = App::new(rounds, self.explain);
        let fight = Solved {
            conflict,
            names,
            outcome,
            beats,
            scenario,
            seed,
        };
        self.screen = Screen::Fight { fight, app };
    }

    fn handle_fight(&mut self, input: Input) {
        let key = match input {
            Input::Space => Key::TogglePause,
            Input::Left | Input::Char('h') => Key::Back,
            Input::Right | Input::Char('l') => Key::Forward,
            Input::Char('+' | '=') => Key::Faster,
            Input::Char('-') => Key::Slower,
            Input::Char('t') => Key::ToggleTactician,
            Input::Esc | Input::Char('b') => return self.leave_fight(),
            Input::Char('q') => {
                self.quit = true;
                return;
            }
            _ => return,
        };
        if let Screen::Fight { app, .. } = &mut self.screen {
            app.handle(key);
        }
    }

    /// Back to the menu, keeping the fight as the last one and the tactician setting.
    fn leave_fight(&mut self) {
        if let Screen::Fight { fight, app } = std::mem::replace(&mut self.screen, Screen::Menu) {
            self.explain = app.explain();
            self.last = Some(fight);
        }
    }

    /// Takes the solution for `generation`; true if the lineup accepted it.
    pub fn solved(&mut self, generation: u64, solution: Solution) -> bool {
        match &mut self.screen {
            Screen::Lineup {
                generation: current,
                solution: slot,
                ..
            } if *current == generation => {
                *slot = Some(solution);
                true
            }
            _ => false,
        }
    }

    /// Advances playback; true if the fight screen needs a redraw.
    pub fn tick(&mut self, elapsed: Duration) -> bool {
        let Screen::Fight { app, .. } = &mut self.screen else {
            return false;
        };
        let (shown, flashing) = (app.shown(), app.flashing());
        app.tick(elapsed);
        app.shown() != shown || app.flashing() != flashing
    }

    pub fn draw(&self, frame: &mut Frame) {
        match &self.screen {
            Screen::Menu => draw_menu(frame, &self.menu),
            Screen::Lineup {
                conflict,
                names,
                scenario,
                seed,
                intro,
                solution,
                ..
            } => {
                let lineup = LineupView {
                    start: conflict,
                    names,
                    intro,
                    scenario: scenario.clone(),
                    seed: *seed,
                };
                draw_lineup(frame, &lineup, solution.is_some());
            }
            Screen::Fight { fight, app } => draw(frame, app, &fight_view(fight)),
        }
    }

    /// The fight on screen, or else the last one left with Back.
    pub fn into_last(self) -> Option<Solved> {
        match self.screen {
            Screen::Fight { fight, .. } => Some(fight),
            _ => self.last,
        }
    }
}

fn fight_view(fight: &Solved) -> FightView<'_> {
    FightView {
        start: &fight.conflict,
        outcome: &fight.outcome,
        names: &fight.names,
        beats: &fight.beats,
        scenario: fight.scenario.clone(),
        seed: fight.seed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::roster::Roster;
    use crate::scenario::{Preset, Setup};
    use crate::tui::view::render_with;

    fn session() -> Session {
        let menu = Menu::new(Setup::Preset(Preset::Ambush), Roster::Random, 7);
        Session::new(menu, 6, false)
    }

    fn solution() -> Solution {
        let Fight { conflict, names } =
            scenario::build(Setup::Preset(Preset::Ambush), Roster::Random, 7);
        solve(&conflict, &names, 7, 6)
    }

    fn job(session: &mut Session) -> Job {
        session.handle(Input::Enter).expect("a job")
    }

    fn fight(session: &mut Session) {
        let job = job(session);
        assert!(session.solved(job.generation, solution()));
        session.handle(Input::Enter);
        assert_eq!(session.screen(), ScreenKind::Fight);
    }

    #[test]
    fn enter_starts_a_lineup_and_returns_a_job() {
        let mut s = session();
        let job = job(&mut s);
        assert_eq!(s.screen(), ScreenKind::Lineup);
        assert_eq!((job.seed, job.depth), (7, 6));
        assert_eq!(job.names.len(), 2);
    }

    #[test]
    fn begin_waits_for_the_solution() {
        let mut s = session();
        let job = job(&mut s);
        assert!(s.handle(Input::Space).is_none());
        assert!(s.handle(Input::Enter).is_none());
        assert_eq!(s.screen(), ScreenKind::Lineup);
        assert!(s.solved(job.generation, solution()));
        s.handle(Input::Space);
        assert_eq!(s.screen(), ScreenKind::Fight);
    }

    #[test]
    fn stale_solution_is_dropped() {
        let mut s = session();
        let first = job(&mut s).generation;
        s.handle(Input::Char('b'));
        let second = job(&mut s).generation;
        assert_eq!(second, first + 1);
        assert!(!s.solved(first, solution()));
        s.handle(Input::Enter);
        assert_eq!(s.screen(), ScreenKind::Lineup);
        assert!(s.solved(second, solution()));
    }

    #[test]
    fn solution_is_dropped_off_the_lineup() {
        let mut s = session();
        let generation = job(&mut s).generation;
        s.handle(Input::Esc);
        assert!(!s.solved(generation, solution()));
    }

    #[test]
    fn back_from_lineup_and_fight_returns_to_the_menu_with_its_values() {
        let mut s = session();
        s.handle(Input::Char('r'));
        let seed = {
            let job = job(&mut s);
            job.seed
        };
        assert_ne!(seed, 7);
        s.handle(Input::Char('b'));
        assert_eq!(s.screen(), ScreenKind::Menu);
        assert_eq!(job(&mut s).seed, seed);
        s.handle(Input::Esc);
        fight(&mut s);
        s.handle(Input::Char('b'));
        assert_eq!(s.screen(), ScreenKind::Menu);
        assert_eq!(job(&mut s).seed, seed);
    }

    #[test]
    fn digits_edit_only_on_digit_chars() {
        let mut s = session();
        for _ in 0..8 {
            s.handle(Input::Down);
        }
        s.handle(Input::Char('x'));
        s.handle(Input::Char('1'));
        assert_eq!(s.screen(), ScreenKind::Menu);
    }

    #[test]
    fn quit_from_every_screen() {
        for quit in [Input::Char('q'), Input::Esc, Input::Quit] {
            let mut s = session();
            s.handle(quit);
            assert!(s.should_quit());
        }
        for quit in [Input::Char('q'), Input::Quit] {
            let mut s = session();
            job(&mut s);
            s.handle(quit);
            assert!(s.should_quit());
            let mut s = session();
            fight(&mut s);
            s.handle(quit);
            assert!(s.should_quit());
        }
    }

    #[test]
    fn esc_in_fight_goes_back_not_quit() {
        let mut s = session();
        fight(&mut s);
        s.handle(Input::Esc);
        assert!(!s.should_quit());
        assert_eq!(s.screen(), ScreenKind::Menu);
    }

    #[test]
    fn last_fight_is_kept_after_going_back() {
        let mut s = session();
        fight(&mut s);
        s.handle(Input::Esc);
        let last = s.into_last().expect("last fight");
        assert_eq!(last.seed, 7);
        assert!(!last.scenario.is_empty());
    }

    #[test]
    fn no_last_fight_without_a_begin() {
        let s = session();
        assert!(s.into_last().is_none());
        let mut s = session();
        job(&mut s);
        assert!(s.into_last().is_none());
    }

    #[test]
    fn quitting_in_a_fight_keeps_it_as_last() {
        let mut s = session();
        fight(&mut s);
        s.handle(Input::Quit);
        assert!(s.into_last().is_some());
    }

    #[test]
    fn tactician_toggle_carries_over() {
        let mut s = session();
        fight(&mut s);
        s.handle(Input::Char('t'));
        s.handle(Input::Char('b'));
        fight(&mut s);
        let Screen::Fight { app, .. } = &s.screen else {
            panic!("on the fight");
        };
        assert!(app.explain());
    }

    #[test]
    fn tick_reports_changes_only_in_the_fight() {
        let mut s = session();
        assert!(!s.tick(Duration::from_secs(1)));
        job(&mut s);
        assert!(!s.tick(Duration::from_secs(1)));
        let mut s = session();
        fight(&mut s);
        assert!(!s.tick(Duration::from_millis(1)));
        assert!(s.tick(Duration::from_secs(10)));
    }

    #[test]
    fn lineup_and_fight_render() {
        let mut s = session();
        let job = job(&mut s);
        let text = render_with(100, 30, |f| s.draw(f));
        assert!(!text.trim().is_empty());
        s.solved(job.generation, solution());
        s.handle(Input::Enter);
        let text = render_with(100, 30, |f| s.draw(f));
        assert!(!text.trim().is_empty());
    }
}
