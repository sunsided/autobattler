//! Playback state machine: which event is shown, and when the next one lands.
//!
//! Pure logic with no terminal access; the view and event loop drive it with
//! [`Key`]s and [`App::tick`]s.

use std::time::Duration;

/// Playback speed multipliers, in quarters (so scaling stays in integers and
/// never drifts): 0.25x, 0.5x, 1x, 2x, 4x.
const SPEED_QUARTERS: [u32; 5] = [1, 2, 4, 8, 16];
const DEFAULT_SPEED: usize = 2;

const ROUND_DELAY: Duration = Duration::from_millis(800);
const ACTION_DELAY: Duration = Duration::from_millis(400);
const FLASH: Duration = Duration::from_millis(150);

/// A user command, already decoded from the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    TogglePause,
    Back,
    Forward,
    Faster,
    Slower,
    ToggleTactician,
    Quit,
}

/// Playback position and settings over a timeline of events.
#[derive(Debug, Clone)]
pub struct App {
    rounds: Vec<usize>,
    shown: usize,
    playing: bool,
    speed: usize,
    /// Speed-scaled time banked towards the next event.
    accumulator: Duration,
    /// Real time left on the flash of the latest event.
    flash: Duration,
    explain: bool,
    quit: bool,
}

impl App {
    /// `rounds[i]` is the round of event `i`.
    pub fn new(rounds: Vec<usize>, explain: bool) -> Self {
        let playing = !rounds.is_empty();
        Self {
            rounds,
            shown: 0,
            playing,
            speed: DEFAULT_SPEED,
            accumulator: Duration::ZERO,
            flash: Duration::ZERO,
            explain,
            quit: false,
        }
    }

    pub fn shown(&self) -> usize {
        self.shown
    }

    pub fn len(&self) -> usize {
        self.rounds.len()
    }

    pub fn playing(&self) -> bool {
        self.playing
    }

    pub fn speed(&self) -> f32 {
        SPEED_QUARTERS[self.speed] as f32 / 4.0
    }

    pub fn explain(&self) -> bool {
        self.explain
    }

    pub fn flashing(&self) -> bool {
        !self.flash.is_zero()
    }

    pub fn finished(&self) -> bool {
        self.shown == self.len()
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn handle(&mut self, key: Key) {
        match key {
            Key::TogglePause => {
                if !self.finished() {
                    self.playing = !self.playing;
                }
            }
            Key::Back => self.step(self.shown.saturating_sub(1)),
            Key::Forward => self.step((self.shown + 1).min(self.len())),
            Key::Faster => self.speed = (self.speed + 1).min(SPEED_QUARTERS.len() - 1),
            Key::Slower => self.speed = self.speed.saturating_sub(1),
            Key::ToggleTactician => self.explain = !self.explain,
            Key::Quit => self.quit = true,
        }
    }

    /// Advances real time by `elapsed`.
    ///
    /// The flash counts down in real time; the playback clock only runs while
    /// playing, scaled by speed. At most one event is revealed per tick: any
    /// surplus stays in the accumulator and counts towards the next one.
    pub fn tick(&mut self, elapsed: Duration) {
        self.flash = self.flash.saturating_sub(elapsed);
        if !self.playing || self.finished() {
            return;
        }
        let scaled = elapsed.as_nanos() * u128::from(SPEED_QUARTERS[self.speed]) / 4;
        self.accumulator += Duration::from_nanos(u64::try_from(scaled).unwrap_or(u64::MAX));
        let delay = self.delay(self.shown);
        if self.accumulator >= delay {
            self.accumulator -= delay;
            self.shown += 1;
            self.flash = FLASH;
            if self.finished() {
                self.playing = false;
            }
        }
    }

    /// Moves to `shown`, pausing and dropping any pending flash and progress.
    fn step(&mut self, shown: usize) {
        self.shown = shown;
        self.playing = false;
        self.flash = Duration::ZERO;
        self.accumulator = Duration::ZERO;
    }

    /// Wait before revealing event `next`: a beat longer at a round boundary.
    fn delay(&self, next: usize) -> Duration {
        if next == 0 || self.rounds[next] != self.rounds[next - 1] {
            ROUND_DELAY
        } else {
            ACTION_DELAY
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(vec![1, 1, 2], false)
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn starts_playing_at_the_opening_frame() {
        let a = app();
        assert_eq!(a.shown(), 0);
        assert_eq!(a.len(), 3);
        assert!(a.playing());
        assert_eq!(a.speed(), 1.0);
        assert!(!a.finished());
    }

    #[test]
    fn tick_waits_for_the_first_delay() {
        let mut a = app();
        a.tick(ms(799));
        assert_eq!(a.shown(), 0);
        a.tick(ms(1));
        assert_eq!(a.shown(), 1);
        assert!(a.flashing());
    }

    #[test]
    fn actions_within_a_round_take_400ms() {
        let mut a = app();
        a.tick(ms(800));
        a.tick(ms(399));
        assert_eq!(a.shown(), 1);
        a.tick(ms(1));
        assert_eq!(a.shown(), 2);
    }

    #[test]
    fn new_round_takes_800ms() {
        let mut a = app();
        a.tick(ms(800));
        a.tick(ms(400));
        assert_eq!(a.shown(), 2);
        a.tick(ms(400));
        assert_eq!(a.shown(), 2);
        a.tick(ms(400));
        assert_eq!(a.shown(), 3);
        assert!(a.finished());
        assert!(!a.playing());
    }

    #[test]
    fn speed_scales_ticks() {
        let mut a = app();
        a.handle(Key::Faster);
        a.handle(Key::Faster);
        assert_eq!(a.speed(), 4.0);
        a.tick(ms(200));
        assert_eq!(a.shown(), 1);

        for _ in 0..10 {
            a.handle(Key::Slower);
        }
        assert_eq!(a.speed(), 0.25);
        for _ in 0..10 {
            a.handle(Key::Faster);
        }
        assert_eq!(a.speed(), 4.0);
    }

    #[test]
    fn flash_lasts_150ms_real_time() {
        let mut a = app();
        a.handle(Key::Faster);
        a.handle(Key::Faster);
        a.tick(ms(200));
        assert_eq!(a.shown(), 1);
        // Pause so the scaled clock cannot reveal (and re-flash) another event.
        a.handle(Key::TogglePause);
        a.tick(ms(149));
        assert!(a.flashing());
        a.tick(ms(1));
        assert!(!a.flashing());
    }

    #[test]
    fn pause_stops_ticks() {
        let mut a = app();
        a.handle(Key::TogglePause);
        assert!(!a.playing());
        a.tick(Duration::from_secs(10));
        assert_eq!(a.shown(), 0);
        a.handle(Key::TogglePause);
        assert!(a.playing());
    }

    #[test]
    fn stepping_pauses_and_clamps() {
        let mut a = app();
        a.handle(Key::Back);
        assert_eq!(a.shown(), 0);
        assert!(!a.playing());
        for _ in 0..5 {
            a.handle(Key::Forward);
        }
        assert_eq!(a.shown(), 3);
        assert!(a.finished());
        assert!(!a.flashing());
    }

    #[test]
    fn toggle_pause_at_the_end_does_nothing() {
        let mut a = app();
        for _ in 0..3 {
            a.handle(Key::Forward);
        }
        a.handle(Key::TogglePause);
        assert!(!a.playing());
    }

    #[test]
    fn tactician_toggle_and_quit() {
        let mut a = app();
        assert!(!a.explain());
        a.handle(Key::ToggleTactician);
        assert!(a.explain());
        a.handle(Key::ToggleTactician);
        assert!(!a.explain());
        assert!(!a.should_quit());
        a.handle(Key::Quit);
        assert!(a.should_quit());
    }

    #[test]
    fn empty_timeline_is_finished() {
        let a = App::new(vec![], false);
        assert!(a.finished());
        assert!(!a.playing());
    }
}
