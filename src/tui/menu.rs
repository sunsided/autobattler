//! The scenario menu shown before a fight: rows, cursor and settings.

use crate::cli::MAX_PARTY;
use crate::rng::XorShift64;
use crate::roster::Roster;
use crate::scenario::{Preset, Setup};
use crate::tui::view::too_small;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

const TITLE: &str = "autobattler · choose a fight";
const KEY_BAR: &str = "↑↓ choose · ←→ change · r reroll · 0-9 seed · enter fight · q quit";

/// A line of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    /// A named scenario.
    Preset(Preset),
    /// Generated parties of the chosen sizes.
    Custom,
    Heroes,
    Villains,
    Roster,
    Seed,
}

/// Every row, top to bottom.
pub const ROWS: [Row; 9] = [
    Row::Preset(Preset::Ambush),
    Row::Preset(Preset::Duel),
    Row::Preset(Preset::Skirmish),
    Row::Preset(Preset::Brawl),
    Row::Custom,
    Row::Heroes,
    Row::Villains,
    Row::Roster,
    Row::Seed,
];

/// A decoded key press in the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    Up,
    Down,
    Left,
    Right,
    /// Rolls a new seed, whatever row the cursor is on.
    Reroll,
    Digit(u8),
    Backspace,
}

/// The menu's state: what is chosen and where the cursor stands.
pub struct Menu {
    cursor: Row,
    chosen: Row,
    heroes: usize,
    villains: usize,
    roster: Roster,
    seed: u64,
    rng: XorShift64,
}

impl Menu {
    /// Starts on the row of `setup`, with its settings as the initial values.
    pub fn new(setup: Setup, roster: Roster, seed: u64) -> Self {
        let (chosen, heroes, villains) = match setup {
            Setup::Preset(preset) => (Row::Preset(preset), 2, 3),
            Setup::Custom { heroes, villains } => (Row::Custom, heroes, villains),
        };
        Self {
            cursor: chosen,
            chosen,
            heroes,
            villains,
            roster,
            seed,
            rng: XorShift64::new(seed),
        }
    }

    /// Applies a key press.
    pub fn handle(&mut self, key: MenuKey) {
        match key {
            MenuKey::Up => self.step(false),
            MenuKey::Down => self.step(true),
            MenuKey::Left => self.change(false),
            MenuKey::Right => self.change(true),
            MenuKey::Reroll => self.seed = self.rng.next_u64() % 100_000,
            MenuKey::Digit(digit) if self.cursor == Row::Seed => {
                self.seed = self
                    .seed
                    .checked_mul(10)
                    .and_then(|seed| seed.checked_add(u64::from(digit)))
                    .unwrap_or(self.seed);
            }
            MenuKey::Backspace if self.cursor == Row::Seed => self.seed /= 10,
            MenuKey::Digit(_) | MenuKey::Backspace => {}
        }
    }

    /// The row the cursor stands on.
    pub fn cursor(&self) -> Row {
        self.cursor
    }

    /// The scenario row that will be fought: a preset or custom.
    pub fn chosen(&self) -> Row {
        self.chosen
    }

    /// Whether the cursor can stand on `row`.
    pub fn enabled(&self, row: Row) -> bool {
        match row {
            Row::Heroes | Row::Villains => self.chosen == Row::Custom,
            Row::Roster => self.chosen != Row::Preset(Preset::Ambush),
            _ => true,
        }
    }

    /// Hero count used by the custom scenario.
    pub fn heroes(&self) -> usize {
        self.heroes
    }

    /// Villain count used by the custom scenario.
    pub fn villains(&self) -> usize {
        self.villains
    }

    /// How generated members are chosen.
    pub fn roster(&self) -> Roster {
        self.roster
    }

    /// The seed the fight is built from.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// What to fight: the setup, roster and seed as currently shown.
    pub fn selection(&self) -> (Setup, Roster, u64) {
        let setup = match self.chosen {
            Row::Preset(preset) => Setup::Preset(preset),
            _ => Setup::Custom {
                heroes: self.heroes,
                villains: self.villains,
            },
        };
        (setup, self.roster, self.seed)
    }

    /// Moves to the previous or next enabled row, staying put at the ends.
    fn step(&mut self, down: bool) {
        let at = ROWS.iter().position(|&row| row == self.cursor).unwrap_or(0);
        let target = if down {
            ROWS[at + 1..].iter().find(|&&row| self.enabled(row))
        } else {
            ROWS[..at].iter().rev().find(|&&row| self.enabled(row))
        };
        if let Some(&row) = target {
            self.cursor = row;
            if matches!(row, Row::Preset(_) | Row::Custom) {
                self.chosen = row;
            }
        }
    }

    /// Changes the setting under the cursor; scenario and seed rows ignore it.
    fn change(&mut self, up: bool) {
        let step = |n: usize| {
            if up {
                (n + 1).min(MAX_PARTY)
            } else {
                n.saturating_sub(1).max(1)
            }
        };
        match self.cursor {
            Row::Heroes => self.heroes = step(self.heroes),
            Row::Villains => self.villains = step(self.villains),
            Row::Roster => {
                self.roster = match self.roster {
                    Roster::Random => Roster::Archetypes,
                    Roster::Archetypes => Roster::Random,
                };
            }
            _ => {}
        }
    }
}

/// Draws the menu.
pub fn draw_menu(frame: &mut Frame, menu: &Menu) {
    if too_small(frame) {
        return;
    }
    let [title, scenario, settings, _, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(7),
        Constraint::Length(6),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    let bold = Style::new().add_modifier(Modifier::BOLD);
    frame.render_widget(Paragraph::new(TITLE).style(bold), title);
    draw_block(frame, scenario, " scenario ", &ROWS[..5], menu);
    draw_block(frame, settings, " settings ", &ROWS[5..], menu);
    frame.render_widget(
        Paragraph::new(KEY_BAR).style(Style::new().add_modifier(Modifier::DIM)),
        keys,
    );
}

fn draw_block(frame: &mut Frame, area: Rect, title: &str, rows: &[Row], menu: &Menu) {
    let block = Block::bordered().title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines: Vec<Line> = rows.iter().map(|&row| row_line(menu, row)).collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

/// One menu line: the chosen marker, the name and the value or description.
fn row_line(menu: &Menu, row: Row) -> Line<'static> {
    let (name, value) = match row {
        Row::Preset(preset) => (preset.name(), preset.description().to_string()),
        Row::Custom => (
            "custom",
            format!("{} vs {} generated", menu.heroes(), menu.villains()),
        ),
        Row::Heroes => ("heroes", stepper(menu.heroes())),
        Row::Villains => ("villains", stepper(menu.villains())),
        Row::Roster if !menu.enabled(row) => ("roster", "fixed".to_string()),
        Row::Roster => (
            "roster",
            match menu.roster() {
                Roster::Random => "◂ random ▸".to_string(),
                Roster::Archetypes => "◂ archetypes ▸".to_string(),
            },
        ),
        Row::Seed => ("seed", menu.seed().to_string()),
    };
    let marker = match row {
        Row::Preset(_) | Row::Custom if menu.chosen() == row => "▸ ",
        Row::Preset(_) | Row::Custom => "  ",
        _ => "",
    };
    let indent = if marker.is_empty() { "    " } else { "  " };
    let mut style = Style::new();
    if menu.cursor() == row {
        style = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
    } else if !menu.enabled(row) {
        style = style.add_modifier(Modifier::DIM);
    }
    Line::from(Span::styled(
        format!("{indent}{marker}{name:<11}{value}"),
        style,
    ))
}

fn stepper(n: usize) -> String {
    format!("◂ {n} ▸")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::view::{assert_golden, render_with};
    use ratatui::style::Modifier;

    const AMBUSH: Row = Row::Preset(Preset::Ambush);

    fn ambush() -> Menu {
        Menu::new(Setup::Preset(Preset::Ambush), Roster::Random, 1)
    }

    fn custom(heroes: usize, villains: usize) -> Menu {
        Menu::new(Setup::Custom { heroes, villains }, Roster::Random, 1)
    }

    fn press(menu: &mut Menu, key: MenuKey, times: usize) {
        for _ in 0..times {
            menu.handle(key);
        }
    }

    #[test]
    fn starts_on_the_chosen_row() {
        let menu = custom(3, 1);
        assert_eq!(menu.cursor(), Row::Custom);
        assert_eq!(menu.chosen(), Row::Custom);
        assert_eq!((menu.heroes(), menu.villains()), (3, 1));

        let menu = Menu::new(Setup::Preset(Preset::Duel), Roster::Random, 1);
        assert_eq!(menu.cursor(), Row::Preset(Preset::Duel));
        assert_eq!((menu.heroes(), menu.villains()), (2, 3));
    }

    #[test]
    fn down_skips_disabled_rows() {
        let mut menu = ambush();
        press(&mut menu, MenuKey::Down, 4);
        assert_eq!(menu.cursor(), Row::Custom);
        menu.handle(MenuKey::Down);
        assert_eq!(menu.cursor(), Row::Heroes);

        let mut menu = ambush();
        press(&mut menu, MenuKey::Down, 5);
        assert_eq!(menu.cursor(), Row::Heroes);
    }

    #[test]
    fn moving_onto_a_preset_disables_custom_settings() {
        let mut menu = custom(2, 3);
        menu.handle(MenuKey::Down);
        assert_eq!(menu.cursor(), Row::Heroes);
        assert!(menu.enabled(Row::Heroes));
        press(&mut menu, MenuKey::Up, 2);
        assert_eq!(menu.cursor(), Row::Preset(Preset::Brawl));
        assert_eq!(menu.chosen(), Row::Preset(Preset::Brawl));
        assert!(!menu.enabled(Row::Heroes));
        assert!(!menu.enabled(Row::Villains));
        assert!(menu.enabled(Row::Roster));
        // Setting rows keep the choice.
        menu.handle(MenuKey::Down);
        assert_eq!(menu.cursor(), Row::Custom);
        assert!(menu.enabled(Row::Heroes));
    }

    #[test]
    fn stops_at_the_ends() {
        let mut menu = ambush();
        menu.handle(MenuKey::Up);
        assert_eq!(menu.cursor(), AMBUSH);
        press(&mut menu, MenuKey::Down, 20);
        assert_eq!(menu.cursor(), Row::Seed);
        menu.handle(MenuKey::Down);
        assert_eq!(menu.cursor(), Row::Seed);
    }

    #[test]
    fn left_right_clamp_party_sizes() {
        let mut menu = custom(2, 3);
        menu.handle(MenuKey::Down);
        press(&mut menu, MenuKey::Left, 5);
        assert_eq!(menu.heroes(), 1);
        press(&mut menu, MenuKey::Right, 9);
        assert_eq!(menu.heroes(), MAX_PARTY);
        menu.handle(MenuKey::Down);
        assert_eq!(menu.cursor(), Row::Villains);
        press(&mut menu, MenuKey::Right, 9);
        assert_eq!(menu.villains(), MAX_PARTY);
        press(&mut menu, MenuKey::Left, 9);
        assert_eq!(menu.villains(), 1);
    }

    #[test]
    fn left_right_do_nothing_on_scenario_rows() {
        let mut menu = custom(2, 3);
        press(&mut menu, MenuKey::Right, 3);
        assert_eq!((menu.heroes(), menu.villains()), (2, 3));
        assert_eq!(menu.roster(), Roster::Random);
    }

    #[test]
    fn roster_toggles_and_is_disabled_for_ambush() {
        assert!(!ambush().enabled(Row::Roster));
        let mut menu = custom(2, 3);
        press(&mut menu, MenuKey::Down, 3);
        assert_eq!(menu.cursor(), Row::Roster);
        menu.handle(MenuKey::Right);
        assert_eq!(menu.roster(), Roster::Archetypes);
        menu.handle(MenuKey::Left);
        assert_eq!(menu.roster(), Roster::Random);
    }

    fn on_seed(seed: u64) -> Menu {
        let mut menu = Menu::new(Setup::Preset(Preset::Duel), Roster::Random, seed);
        press(&mut menu, MenuKey::Down, 20);
        assert_eq!(menu.cursor(), Row::Seed);
        menu
    }

    #[test]
    fn seed_digits_and_backspace() {
        let mut menu = on_seed(47);
        menu.handle(MenuKey::Digit(1));
        menu.handle(MenuKey::Digit(1));
        assert_eq!(menu.seed(), 4711);
        menu.handle(MenuKey::Backspace);
        assert_eq!(menu.seed(), 471);
    }

    #[test]
    fn seed_digits_ignore_overflow() {
        let mut menu = on_seed(u64::MAX);
        menu.handle(MenuKey::Digit(9));
        assert_eq!(menu.seed(), u64::MAX);
    }

    #[test]
    fn digits_only_on_the_seed_row() {
        let mut menu = Menu::new(Setup::Preset(Preset::Duel), Roster::Random, 47);
        menu.handle(MenuKey::Digit(1));
        menu.handle(MenuKey::Backspace);
        assert_eq!(menu.seed(), 47);
    }

    #[test]
    fn reroll_is_deterministic_and_below_100000() {
        let mut a = Menu::new(Setup::Preset(Preset::Duel), Roster::Random, 7);
        let mut b = Menu::new(Setup::Preset(Preset::Duel), Roster::Random, 7);
        a.handle(MenuKey::Reroll);
        b.handle(MenuKey::Reroll);
        assert_eq!(a.seed(), b.seed());
        assert!(a.seed() < 100_000);
        a.handle(MenuKey::Reroll);
        assert!(a.seed() < 100_000);
    }

    #[test]
    fn selection_matches_the_menu() {
        let menu = Menu::new(
            Setup::Custom {
                heroes: 3,
                villains: 1,
            },
            Roster::Archetypes,
            9,
        );
        assert_eq!(
            menu.selection(),
            (
                Setup::Custom {
                    heroes: 3,
                    villains: 1
                },
                Roster::Archetypes,
                9
            )
        );
        assert_eq!(
            ambush().selection(),
            (Setup::Preset(Preset::Ambush), Roster::Random, 1)
        );
    }

    fn render(menu: &Menu, width: u16, height: u16) -> String {
        render_with(width, height, |frame| draw_menu(frame, menu))
    }

    #[test]
    fn menu_frame() {
        let menu = Menu::new(Setup::Preset(Preset::Ambush), Roster::Random, 4711);
        let text = render(&menu, 80, 24);
        assert_golden(&text, include_str!("golden/menu_frame.txt"));
    }

    #[test]
    fn menu_custom_frame() {
        let mut menu = Menu::new(
            Setup::Custom {
                heroes: 2,
                villains: 3,
            },
            Roster::Archetypes,
            7,
        );
        menu.handle(MenuKey::Down);
        let text = render(&menu, 80, 24);
        assert_golden(&text, include_str!("golden/menu_custom_frame.txt"));
    }

    #[test]
    fn menu_too_small() {
        let text = render(&ambush(), 79, 24);
        assert!(text.contains("terminal too small (needs 80×24)"), "{text}");
    }

    #[test]
    fn menu_cursor_row_is_reversed() {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let menu = ambush();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| draw_menu(frame, &menu)).unwrap();
        let buffer = terminal.backend().buffer();
        // Row 2 is the first scenario row; column 5 is the first letter of its name.
        let cell = &buffer[(5, 2)];
        assert_eq!(cell.symbol(), "a");
        assert!(cell.modifier.contains(Modifier::REVERSED));
        assert!(cell.modifier.contains(Modifier::BOLD));
        assert!(!buffer[(5, 3)].modifier.contains(Modifier::REVERSED));
    }
}
