//! Draws one frame of the standoff from the playback state and the fight.
//!
//! Layout at 80×24 and larger, top to bottom: a header, the initiative bar,
//! the standoff (heroes left, villains right, the current action in a center
//! band), the log and the key bar.

use crate::action::{Action, AppliedAction};
use crate::conflict::Conflict;
use crate::narration::panel::{filled_cells, morale_tag};
use crate::narration::{log_lines, Beat, BeatKind};
use crate::party::{Participant, Party};
use crate::solver::{Event, Outcome, OutcomeType};
use crate::tui::app::App;
use crate::weapon::Weapon;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use std::time::Duration;

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;
const TOO_SMALL: &str = "terminal too small (needs 80×24)";
const KEY_BAR: &str = "space pause · ← → step · +/- speed · t tactician · b back · q quit";
const LINEUP_KEY_BAR: &str = "space begin · b back · q quit";
const PARTING_PREFIX: &str = "parting blows ▸ ";
/// Cells in a card's health bar.
const BAR_CELLS: usize = 10;
/// Width of the center band that holds the arrow and the flight markers.
const BAND: u16 = 14;
/// The log's minimum height: six lines plus its borders.
const MIN_LOG: u16 = 8;
const HERO: Color = Color::Blue;
const VILLAIN: Color = Color::Magenta;

/// Everything about a solved fight the view needs, besides the playback state.
pub struct FightView<'a> {
    /// The state before the first event.
    pub start: &'a Conflict,
    /// The solved fight.
    pub outcome: &'a Outcome,
    /// `names[party_id][member_id]`.
    pub names: &'a [Vec<String>],
    /// The narration, made with tactician notes on and colors off.
    pub beats: &'a [Beat],
    /// The scenario label for the header.
    pub scenario: String,
    /// The seed the fight was built from.
    pub seed: u64,
}

impl FightView<'_> {
    /// The state after `shown` events.
    fn state(&self, shown: usize) -> &Conflict {
        shown
            .checked_sub(1)
            .map_or(self.start, |i| &self.outcome.timeline[i].state)
    }

    /// The event shown last, if any.
    fn current(&self, shown: usize) -> Option<&Event> {
        shown.checked_sub(1).map(|i| &self.outcome.timeline[i])
    }

    /// The state at the start of the round of the event shown last.
    fn round_start(&self, shown: usize) -> &Conflict {
        let Some(current) = self.current(shown) else {
            return self.start;
        };
        let first = self
            .outcome
            .timeline
            .iter()
            .position(|e| e.round == current.round)
            .unwrap_or(0);
        self.state(first)
    }

    fn name(&self, p: &Participant) -> &str {
        &self.names[p.party_id][p.member_id]
    }

    fn is_hero(&self, p: &Participant) -> bool {
        p.party_id == self.start.initiator.id
    }

    fn color(&self, p: &Participant) -> Color {
        if self.is_hero(p) {
            HERO
        } else {
            VILLAIN
        }
    }
}

/// What differs between the fight and the lineup around the shared standoff.
struct Chrome {
    header_left: String,
    log: Vec<String>,
    keys: &'static str,
}

/// Draws one frame for the playback position of `app`.
pub fn draw(frame: &mut Frame, app: &App, fight: &FightView) {
    let round = fight
        .current(app.shown())
        .map_or(fight.start.round, |e| e.round);
    let state = if app.playing() { "▶" } else { "⏸" };
    let chrome = Chrome {
        header_left: format!(
            "Round {round} · action {}/{} · {state} {}×",
            app.shown(),
            app.len(),
            app.speed()
        ),
        log: fight_log(app, fight),
        keys: KEY_BAR,
    };
    draw_screen(frame, app, fight, &chrome);
}

/// The lineup before a fight: everything [`FightView`] has to show at the start
/// state, with an intro line in the log.
pub struct LineupView<'a> {
    /// The state the fight starts from.
    pub start: &'a Conflict,
    /// `names[party_id][member_id]`.
    pub names: &'a [Vec<String>],
    /// The line that introduces the fight.
    pub intro: &'a str,
    /// The scenario label for the header.
    pub scenario: String,
    /// The seed the fight was built from.
    pub seed: u64,
}

/// Draws the lineup; `ready` once the tactician has solved the fight.
pub fn draw_lineup(frame: &mut Frame, lineup: &LineupView, ready: bool) {
    // The start state is the fight's opening frame: an empty timeline shows it.
    let outcome = Outcome {
        outcome: OutcomeType::Remain(0.0),
        timeline: Vec::new(),
        evaluations: 0,
        cuts: 0,
        search_duration: Duration::ZERO,
        max_visited_depth: 0,
        depth_limited: false,
    };
    let fight = FightView {
        start: lineup.start,
        outcome: &outcome,
        names: lineup.names,
        beats: &[],
        scenario: lineup.scenario.clone(),
        seed: lineup.seed,
    };
    let (status, prompt) = if ready {
        ("ready", "Press space to begin.")
    } else {
        ("tactician thinking…", "The tactician is thinking…")
    };
    let chrome = Chrome {
        header_left: format!("Round {} · {status}", lineup.start.round),
        log: vec![lineup.intro.to_string(), String::new(), prompt.to_string()],
        keys: LINEUP_KEY_BAR,
    };
    draw_screen(frame, &App::new(Vec::new(), false), &fight, &chrome);
}

/// Header, initiative bar, standoff, log and key bar: the layout both screens
/// share.
fn draw_screen(frame: &mut Frame, app: &App, fight: &FightView, chrome: &Chrome) {
    if too_small(frame) {
        return;
    }
    let area = frame.area();

    let fighters = fight
        .start
        .initiator
        .members
        .len()
        .max(fight.start.opponent.members.len()) as u16;
    let available = area.height - 3 - MIN_LOG;
    let stride = card_stride(fighters, available - 2);
    let standoff_height = (card_rows(fighters, stride) + 2).min(available);
    let [header, initiative, standoff, log, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(standoff_height),
        Constraint::Min(MIN_LOG),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(frame, header, &chrome.header_left, fight);
    frame.render_widget(
        Paragraph::new(initiative_line(app, fight, initiative.width)),
        initiative,
    );
    draw_standoff(frame, standoff, stride, app, fight);
    draw_log(frame, log, &chrome.log);
    frame.render_widget(
        Paragraph::new(chrome.keys).style(Style::new().add_modifier(Modifier::DIM)),
        keys,
    );
}

/// Draws the too-small message and returns true when the terminal is below
/// 80×24.
pub(crate) fn too_small(frame: &mut Frame) -> bool {
    let area = frame.area();
    if area.width >= MIN_WIDTH && area.height >= MIN_HEIGHT {
        return false;
    }
    let [_, middle, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(TOO_SMALL).alignment(Alignment::Center),
        middle,
    );
    true
}

/// Rows between the tops of two cards: a blank line between cards if they fit.
fn card_stride(fighters: u16, height: u16) -> u16 {
    if card_rows(fighters, 3) <= height {
        3
    } else {
        2
    }
}

fn card_rows(fighters: u16, stride: u16) -> u16 {
    (fighters * stride).saturating_sub(stride - 2)
}

fn draw_header(frame: &mut Frame, area: Rect, left: &str, fight: &FightView) {
    let right = format!("{} · seed {}", fight.scenario, fight.seed);
    let bold = Style::new().add_modifier(Modifier::BOLD);
    frame.render_widget(Paragraph::new(left).style(bold), area);
    frame.render_widget(Paragraph::new(right).alignment(Alignment::Right), area);
}

/// The members able to act in the shown round, in initiative order.
///
/// If the entries don't fit `width` joined by ` ▸ `, they are joined by a
/// bare `▸`, and if that is still too wide the bar ends in `…`.
fn initiative_line<'a>(app: &App, fight: &'a FightView, width: u16) -> Line<'a> {
    let shown = app.shown();
    let state = fight.state(shown);
    let round_start = fight.round_start(shown);
    let actor = fight.current(shown).map(|e| &e.actor);

    let mut entries = Vec::new();
    for p in state.initiative_order() {
        let now = state.targeted_member(&p);
        if !round_start.targeted_member(&p).can_act() || now.fled {
            continue;
        }
        let mut style = Style::new().fg(fight.color(&p));
        if now.is_dead() {
            style = style.add_modifier(Modifier::DIM | Modifier::CROSSED_OUT);
        }
        if actor == Some(&p) {
            style = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
        }
        entries.push(Span::styled(
            format!("{}({})", fight.name(&p), now.speed),
            style,
        ));
    }

    let prefix = state.parting.is_some().then(|| {
        Span::styled(
            PARTING_PREFIX,
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        )
    });
    let join = |separator: &'static str| {
        let mut spans: Vec<Span> = prefix.iter().cloned().collect();
        for (i, entry) in entries.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(separator));
            }
            spans.push(entry.clone());
        }
        Line::from(spans)
    };
    let width = usize::from(width);
    let line = join(" ▸ ");
    if line.width() <= width {
        return line;
    }
    let compact = join("▸");
    if compact.width() <= width {
        return compact;
    }
    let mut line = compact;
    while line.width() > width.saturating_sub(1) && !line.spans.is_empty() {
        line.spans.pop();
    }
    if line.spans.last().is_some_and(|span| span.content == "▸") {
        line.spans.pop();
    }
    line.spans.push(Span::raw("…"));
    line
}

fn draw_standoff(frame: &mut Frame, area: Rect, stride: u16, app: &App, fight: &FightView) {
    let block = Block::bordered()
        .title(Line::from(" heroes ").style(Style::new().fg(HERO)))
        .title(
            Line::from(" villains ")
                .style(Style::new().fg(VILLAIN))
                .right_aligned(),
        );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // The band holds the arrow on card line 1; card line 2 may run into it,
    // since nothing is drawn there on line-2 rows.
    let band_x = inner.x + (inner.width - BAND) / 2;
    let half = inner.width / 2 - 1;
    let buffer = frame.buffer_mut();
    let row = |member: usize| inner.y + member as u16 * stride;

    let state = fight.state(app.shown());
    // Where line 1 of the hero cards ends and that of the villain cards starts.
    let (mut hero_end, mut villain_x) = (inner.x, band_x + BAND);
    for party in [&state.initiator, &state.opponent] {
        let hero = party.id == fight.start.initiator.id;
        let (cards, width) = party_cards(party, app, fight);
        let width = width as u16;
        if hero {
            hero_end = (inner.x + width).min(band_x);
        } else {
            villain_x = inner.right().saturating_sub(width).max(band_x + BAND);
        }
        for (i, (line1, line2)) in cards.iter().enumerate() {
            let y = row(i);
            if y >= inner.bottom() {
                break;
            }
            if hero {
                buffer.set_line(inner.x, y, line1, band_x - inner.x);
            } else {
                buffer.set_line(villain_x, y, line1, inner.right() - villain_x);
            }
            if y + 1 >= inner.bottom() {
                break;
            }
            let w = (line2.width() as u16).min(half);
            let x = if hero { inner.x } else { inner.right() - w };
            buffer.set_line(x, y + 1, line2, w);
        }
    }

    let Some(event) = fight.current(app.shown()) else {
        return;
    };
    let style = Style::new()
        .fg(fight.color(&event.actor))
        .add_modifier(Modifier::BOLD);
    let hero = fight.is_hero(&event.actor);
    // Everything sits one column off the card it belongs to: an arrow's head
    // off its target's card, a flight marker off its actor's card.
    let (text, y, x) = match &event.action {
        AppliedAction::Targeted(targeted) => {
            let Action::SimpleAttack(attack) = &targeted.action;
            let text = if hero {
                format!("── {} ──▶", attack.damage)
            } else {
                format!("◀── {} ──", attack.damage)
            };
            let width = Line::from(text.as_str()).width() as u16;
            let x = if hero {
                villain_x.saturating_sub(width + 1)
            } else {
                hero_end + 1
            };
            (text, row(targeted.target.member_id), x)
        }
        AppliedAction::Flee | AppliedAction::Retreat => {
            let marker = if event.action == AppliedAction::Flee {
                "↩"
            } else {
                "⚑"
            };
            let x = if hero { hero_end + 1 } else { villain_x - 2 };
            (marker.to_string(), row(event.actor.member_id), x)
        }
    };
    if y < inner.bottom() {
        buffer.set_line(x, y, &Line::styled(text, style), BAND);
    }
}

/// Builds the two card lines for every member of `party`, padded so that
/// names and bars line up within the party, and returns them with the width
/// of line 1.
///
/// That width comes from the party's names and starting health alone, so a
/// card keeps its place however the fight goes.
fn party_cards<'a>(
    party: &Party,
    app: &App,
    fight: &'a FightView,
) -> (Vec<(Line<'a>, Line<'a>)>, usize) {
    let name_width = fight.names[party.id]
        .iter()
        .map(|n| n.chars().count())
        .max()
        .unwrap_or(0);
    let shown = app.shown();
    let before = fight.state(shown.saturating_sub(1));
    let current = fight.current(shown);
    let start = fight
        .start
        .action_target(party.id)
        .members
        .iter()
        .map(|m| m.health);
    let max_width = start
        .clone()
        .map(|max| format!("{max}").chars().count())
        .max()
        .unwrap_or(0);

    let mut cards: Vec<(Line, Line)> = party
        .members
        .iter()
        .zip(start)
        .map(|(member, max)| {
            let p = Participant {
                party_id: party.id,
                member_id: member.id,
            };
            let color = fight.color(&p);
            let mut name_style = Style::new().fg(color);
            if current.is_some_and(|e| e.actor == p) {
                name_style = name_style.add_modifier(Modifier::BOLD);
            }
            let mut line1 = vec![
                Span::styled(format!("{:<name_width$}", fight.name(&p)), name_style),
                Span::raw(" "),
            ];
            // A killing blow leaves no health cells to flash red, so the
            // target shows `✝ fallen` right away instead.
            if member.is_dead() {
                line1.push(Span::raw("✝ fallen"));
            } else if member.fled {
                line1.push(Span::raw("↩ fled"));
            } else {
                let filled = filled_cells(member.health, max, BAR_CELLS);
                let hit = app.flashing()
                    && current.is_some_and(
                        |e| matches!(&e.action, AppliedAction::Targeted(t) if t.target == p),
                    );
                let lost = if hit {
                    let was = before.targeted_member(&p).health;
                    filled_cells(was, max, BAR_CELLS).saturating_sub(filled)
                } else {
                    0
                };
                line1.push(Span::styled("█".repeat(filled), Style::new().fg(color)));
                line1.push(Span::styled("█".repeat(lost), Style::new().fg(Color::Red)));
                line1.push(Span::styled(
                    "░".repeat(BAR_CELLS - filled - lost),
                    Style::new().fg(Color::DarkGray),
                ));
                line1.push(Span::raw(format!(
                    " {:>max_width$}/{max}",
                    format!("{}", member.health)
                )));
            }

            let weapon = match member.weapon {
                Weapon::Fists(_) => "fists",
                Weapon::Stick(_) => "stick",
            };
            let mut stats = format!(
                "{weapon} {} · speed {}",
                member.weapon.damage(),
                member.speed
            );
            if let Some(tag) = morale_tag(member.morale) {
                stats.push_str(&format!(" · {tag}"));
            }
            let mut line1 = Line::from(line1);
            let mut line2 = Line::from(stats);
            if member.is_dead() {
                line1 = line1.style(Style::new().add_modifier(Modifier::DIM));
                line2 = line2.style(Style::new().add_modifier(Modifier::DIM));
            }
            (line1, line2)
        })
        .collect();

    // Pad line 1 to the width of a full card (name, bar and `hp/max` at the
    // widest), so right-aligned villain cards keep their names in a column
    // and `✝ fallen` or `↩ fled` take the room of the bar.
    let width = name_width + 1 + BAR_CELLS + 1 + max_width + 1 + max_width;
    for (line1, _) in &mut cards {
        let pad = width.saturating_sub(line1.width());
        line1.push_span(Span::raw(" ".repeat(pad)));
    }
    (cards, width)
}

/// The log lines of the fight up to the playback position.
fn fight_log(app: &App, fight: &FightView) -> Vec<String> {
    let mut lines = log_lines(fight.beats, app.shown(), app.explain());
    if app.finished() {
        // An empty timeline's "Nothing happens." belongs to no event.
        let closing = fight.beats.iter().filter(|b| match b.kind {
            BeatKind::Action => b.event.is_none(),
            BeatKind::Outro | BeatKind::Stats => true,
            _ => false,
        });
        lines.extend(
            closing
                .flat_map(|b| b.lines.iter().cloned())
                .filter(|line| !line.is_empty()),
        );
    }
    lines
}

fn draw_log(frame: &mut Frame, area: Rect, lines: &[String]) {
    let block = Block::bordered().title(" log ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows: Vec<String> = lines
        .iter()
        .flat_map(|line| wrap(line, usize::from(inner.width)))
        .collect();
    let skip = rows.len().saturating_sub(inner.height as usize);
    let rows: Vec<Line> = rows.into_iter().skip(skip).map(Line::from).collect();
    frame.render_widget(Paragraph::new(rows), inner);
}

/// Display width of `text`, measured the way the buffer lays it out.
fn display_width(text: &str) -> usize {
    Span::raw(text).width()
}

/// Wraps `line` into rows of at most `width` columns, breaking after words
/// where it can. Continuation rows hang two columns deeper than the line's own
/// indent.
fn wrap(line: &str, width: usize) -> Vec<String> {
    if display_width(line) <= width || width == 0 {
        return vec![line.to_string()];
    }
    let lead = line.len() - line.trim_start().len();
    let mut indent = format!("{}  ", &line[..lead]);
    if display_width(&indent) >= width / 2 {
        indent.clear();
    }

    let mut rows = Vec::new();
    let mut prefix = "";
    let mut rest = line;
    // On the first row, a break inside the leading whitespace is no break.
    let mut keep = lead;
    loop {
        let room = width - display_width(prefix);
        if display_width(rest) <= room {
            rows.push(format!("{prefix}{rest}"));
            return rows;
        }
        // The longest start of `rest` that fits, as a byte offset.
        let mut fit = 0;
        let mut used = 0;
        for (i, c) in rest.char_indices() {
            let w = display_width(c.encode_utf8(&mut [0; 4]));
            if used + w > room {
                break;
            }
            used += w;
            fit = i + c.len_utf8();
        }
        let space = if rest[fit..].starts_with(' ') {
            Some(fit)
        } else {
            rest[..fit].rfind(' ')
        };
        let cut = match space {
            Some(space) if space > keep => space,
            // One word wider than the row: break inside it.
            _ => fit.max(rest.chars().next().map_or(1, char::len_utf8)),
        };
        rows.push(format!("{prefix}{}", rest[..cut].trim_end()));
        rest = rest[cut..].trim_start();
        prefix = &indent;
        keep = 0;
    }
}

/// Renders one frame drawn by `draw` and returns its cell symbols row by row,
/// with trailing spaces trimmed.
#[cfg(test)]
pub(crate) fn render_with(width: u16, height: u16, draw: impl FnOnce(&mut Frame)) -> String {
    use ratatui::backend::TestBackend;
    use ratatui::buffer::CellWidth;
    use ratatui::Terminal;

    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test terminal");
    terminal.draw(draw).expect("draw a frame");
    let buffer = terminal.backend().buffer();
    let mut text = String::new();
    for row in buffer.content.chunks(width as usize) {
        let mut line = String::new();
        // Skip the cells hidden behind a wide symbol.
        let mut hidden: u16 = 0;
        for cell in row {
            if hidden == 0 {
                line.push_str(cell.symbol());
            }
            hidden = hidden.max(cell.symbol().cell_width()).saturating_sub(1);
        }
        text.push_str(line.trim_end());
        text.push('\n');
    }
    text
}

/// Fails with both frames shown when `actual` differs from the golden file.
#[cfg(test)]
#[track_caller]
pub(crate) fn assert_golden(actual: &str, expected: &str) {
    assert!(
        actual == expected,
        "frame differs from golden\n--- actual ---\n{actual}--- expected ---\n{expected}"
    );
}

#[cfg(test)]
fn render_to_text(app: &App, fight: &FightView, width: u16, height: u16) -> String {
    render_with(width, height, |frame| draw(frame, app, fight))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict::Conflict;
    use crate::narration::{BeatKind, NarrationOptions, Narrator};
    use crate::roster::Roster;
    use crate::scenario::{self, Setup};
    use crate::solver::{Outcome, Solver, SolverStrategy, MAX_ROUNDS};
    use crate::tui::app::{App, Key};

    /// Owns everything a [`FightView`] borrows.
    struct Fixture {
        start: Conflict,
        outcome: Outcome,
        names: Vec<Vec<String>>,
        beats: Vec<Beat>,
        scenario: String,
        seed: u64,
    }

    impl Fixture {
        fn new(start: Conflict, names: Vec<Vec<String>>, depth: usize, scenario: &str) -> Self {
            colored::control::set_override(false);
            let seed = 7;
            let outcome = Solver::play(
                &start,
                SolverStrategy::IterativeDeepening(depth),
                MAX_ROUNDS,
            );
            let options = NarrationOptions {
                explain: true,
                seed: Some(seed),
            };
            let mut beats = Narrator::new(&names, options).narrate(&start, &outcome);
            pin_search_duration(&mut beats);
            Self {
                start,
                outcome,
                names,
                beats,
                scenario: scenario.to_string(),
                seed,
            }
        }

        fn view(&self) -> FightView<'_> {
            FightView {
                start: &self.start,
                outcome: &self.outcome,
                names: &self.names,
                beats: &self.beats,
                scenario: self.scenario.clone(),
                seed: self.seed,
            }
        }

        /// An app paused at `shown`, with no flash.
        fn app(&self, shown: usize, explain: bool) -> App {
            let rounds = self.outcome.timeline.iter().map(|e| e.round).collect();
            let mut app = App::new(rounds, explain);
            for _ in 0..shown {
                app.handle(Key::Forward);
            }
            app
        }

        fn render(&self, app: &App, width: u16, height: u16) -> String {
            render_to_text(app, &self.view(), width, height)
        }
    }

    /// Replaces the search duration in the stats line, which differs between
    /// runs, with a fixed value. The evaluation counts are deterministic.
    fn pin_search_duration(beats: &mut [Beat]) {
        let stats = beats
            .iter_mut()
            .find(|b| b.kind == BeatKind::Stats)
            .expect("a stats beat");
        let line = &mut stats.lines[0];
        let cut = line.rfind(", ").expect("a duration field");
        line.replace_range(cut.., ", 1.5ms)");
    }

    fn names() -> Vec<Vec<String>> {
        vec![
            vec!["Brull".to_string()],
            vec!["Ziuon".to_string(), "Molphige".to_string()],
        ]
    }

    fn ambush() -> Fixture {
        Fixture::new(scenario::default_conflict(), names(), 10, "ambush")
    }

    fn four_versus_four() -> Fixture {
        let fight = scenario::build(
            Setup::Custom {
                heroes: 4,
                villains: 4,
            },
            Roster::Archetypes,
            7,
        );
        Fixture::new(fight.conflict, fight.names, 4, "custom 4v4")
    }

    #[test]
    fn opening_frame() {
        let f = ambush();
        let app = App::new(f.outcome.timeline.iter().map(|e| e.round).collect(), true);
        assert_golden(
            &f.render(&app, 80, 24),
            include_str!("golden/opening_frame.txt"),
        );
    }

    #[test]
    fn first_action_frame() {
        let f = ambush();
        let app = f.app(1, true);
        assert!(!app.flashing());
        assert_golden(
            &f.render(&app, 80, 24),
            include_str!("golden/first_action_frame.txt"),
        );
    }

    #[test]
    fn parting_pending_frame() {
        let f = ambush();
        assert!(f.outcome.timeline[1].state.parting.is_some());
        let text = f.render(&f.app(2, true), 80, 24);
        let initiative = text.lines().nth(1).unwrap();
        assert!(initiative.starts_with("parting blows ▸ "), "{initiative}");
        let ziuon = text.lines().find(|l| l.contains("Ziuon ")).unwrap();
        assert!(ziuon.contains('↩'), "{ziuon}");
        assert_golden(&text, include_str!("golden/parting_pending_frame.txt"));
    }

    #[test]
    fn end_frame_with_and_without_tactician() {
        let f = ambush();
        for (explain, golden) in [
            (true, include_str!("golden/end_frame.txt")),
            (false, include_str!("golden/end_frame_no_tactician.txt")),
        ] {
            let app = f.app(3, explain);
            assert!(app.finished());
            let text = f.render(&app, 80, 24);
            assert!(text.contains("✝ fallen"), "{text}");
            assert!(text.contains("↩ fled"), "{text}");
            assert!(text.contains("holds the field"), "{text}");
            assert!(text.contains("evaluations"), "{text}");
            assert_eq!(text.contains("↳ tactician"), explain, "{text}");
            assert_golden(&text, golden);
        }
    }

    #[test]
    fn four_versus_four_frame() {
        let f = four_versus_four();
        let text = f.render(&f.app(1, true), 80, 24);
        for name in f.names.iter().flatten() {
            assert!(text.contains(name.as_str()), "{name} missing\n{text}");
        }
        let initiative = text.lines().nth(1).unwrap();
        assert!(initiative.ends_with(')'), "clipped: {initiative}");
        assert_golden(&text, include_str!("golden/four_versus_four_frame.txt"));
    }

    #[test]
    fn too_small_frame() {
        let f = ambush();
        let text = f.render(&f.app(1, true), 60, 20);
        assert!(text.contains("terminal too small (needs 80×24)"), "{text}");
        assert_golden(&text, include_str!("golden/too_small_frame.txt"));
    }

    #[test]
    fn no_ansi_in_cells() {
        for f in [ambush(), four_versus_four()] {
            for shown in 0..=f.outcome.timeline.len() {
                for explain in [true, false] {
                    let text = f.render(&f.app(shown, explain), 80, 24);
                    assert!(!text.contains('\x1b'), "{text}");
                    assert!(!text.contains("[0m"), "{text}");
                }
            }
        }
    }

    #[test]
    fn key_bar_is_the_last_line() {
        let f = ambush();
        let text = f.render(&f.app(1, true), 80, 24);
        assert_eq!(
            text.lines().last().unwrap(),
            "space pause · ← → step · +/- speed · t tactician · b back · q quit"
        );
    }

    fn lineup_text(f: &Fixture, ready: bool, width: u16, height: u16) -> String {
        let narrator = Narrator::new(
            &f.names,
            NarrationOptions {
                explain: false,
                seed: None,
            },
        );
        let intro = narrator.intro(&f.start);
        let lineup = LineupView {
            start: &f.start,
            names: &f.names,
            intro: &intro,
            scenario: f.scenario.clone(),
            seed: f.seed,
        };
        render_with(width, height, |frame| draw_lineup(frame, &lineup, ready))
    }

    #[test]
    fn lineup_thinking_frame() {
        let text = lineup_text(&ambush(), false, 80, 24);
        assert!(text.contains("Round 1 · tactician thinking…"), "{text}");
        assert!(text.contains("The tactician is thinking…"), "{text}");
        assert_eq!(
            text.lines().last().unwrap(),
            "space begin · b back · q quit"
        );
        assert_golden(&text, include_str!("golden/lineup_thinking_frame.txt"));
    }

    #[test]
    fn lineup_ready_frame() {
        let text = lineup_text(&ambush(), true, 80, 24);
        assert!(text.contains("Round 1 · ready"), "{text}");
        assert!(text.contains("Press space to begin."), "{text}");
        assert_golden(&text, include_str!("golden/lineup_ready_frame.txt"));
    }

    #[test]
    fn lineup_cards_match_the_opening_frame() {
        let f = ambush();
        let lineup = lineup_text(&f, true, 80, 24);
        let app = App::new(f.outcome.timeline.iter().map(|e| e.round).collect(), true);
        let fight = f.render(&app, 80, 24);
        // Rows 2.. through the bottom border of the standoff.
        let bottom = fight
            .lines()
            .position(|l| l.starts_with('└'))
            .expect("standoff border");
        let rows = |text: &str| {
            text.lines()
                .skip(1)
                .take(bottom)
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(rows(&lineup), rows(&fight));
    }

    #[test]
    fn lineup_too_small() {
        let text = lineup_text(&ambush(), true, 60, 20);
        assert_golden(&text, include_str!("golden/too_small_frame.txt"));
    }

    #[test]
    fn empty_timeline_says_nothing_happens_at_the_end() {
        let mut start = scenario::default_conflict();
        for member in start.opponent.members.iter_mut() {
            member.health = 0.0;
        }
        let f = Fixture::new(start, names(), 10, "ambush");
        let app = f.app(0, true);
        assert!(app.finished());
        let text = f.render(&app, 80, 24);
        let nothing = text.find("Nothing happens.").expect(&text);
        let stats = text.find("evaluations").expect(&text);
        assert!(nothing < stats, "{text}");
    }

    #[test]
    fn initiative_bar_keeps_a_compact_bar_that_fits_exactly() {
        let f = four_versus_four();
        let app = f.app(1, true);
        let view = f.view();
        let text = |line: Line| {
            line.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        };
        let compact = text(initiative_line(&app, &view, 200)).replace(" ▸ ", "▸");
        let exact = display_width(&compact) as u16;
        assert_eq!(text(initiative_line(&app, &view, exact)), compact);
        let trimmed = text(initiative_line(&app, &view, exact - 1));
        assert!(trimmed.ends_with('…'), "{trimmed}");
        assert!(display_width(&trimmed) < usize::from(exact), "{trimmed}");
    }

    #[test]
    fn villain_cards_keep_their_column() {
        let f = ambush();
        let column = |shown: usize| {
            let text = f.render(&f.app(shown, true), 80, 24);
            let line = text
                .lines()
                .find(|l| l.contains("Ziuon "))
                .unwrap()
                .to_string();
            line.chars().position(|c| c == 'Z').unwrap()
        };
        let first = column(0);
        for shown in 1..=f.outcome.timeline.len() {
            assert_eq!(column(shown), first, "after {shown} actions");
        }
    }

    #[test]
    fn wrap_hangs_continuation_rows_and_fits_the_width() {
        let line = "   ↳ tactician: expects they escape before the parting blows land";
        let rows = wrap(line, 30);
        assert!(rows.len() > 1, "{rows:?}");
        for row in &rows {
            assert!(display_width(row) <= 30, "{rows:?}");
        }
        assert!(rows[0].starts_with("   ↳ "), "{rows:?}");
        for row in &rows[1..] {
            assert!(
                row.starts_with("     ") && !row.starts_with("      "),
                "{rows:?}"
            );
        }
        let joined: Vec<&str> = rows.iter().flat_map(|r| r.split_whitespace()).collect();
        assert_eq!(
            joined.join(" "),
            line.split_whitespace().collect::<Vec<_>>().join(" ")
        );
        assert_eq!(wrap("short", 30), vec!["short".to_string()]);
        // A word wider than the row is broken inside.
        assert!(wrap(&"x".repeat(25), 10)
            .iter()
            .all(|r| display_width(r) <= 10));
    }

    #[test]
    fn long_log_lines_wrap_inside_the_panel() {
        let f = four_versus_four();
        let intro = f
            .beats
            .iter()
            .find(|b| b.kind == BeatKind::Intro)
            .expect("an intro beat")
            .lines
            .iter()
            .find(|l| display_width(l) > 78)
            .expect("an intro line wider than the panel")
            .clone();
        let text = f.render(&f.app(0, true), 80, 24);
        let rows: Vec<&str> = text
            .lines()
            .skip_while(|l| !l.starts_with("┌ log"))
            .skip(1)
            .take_while(|l| !l.starts_with('└'))
            .collect();
        for row in &rows {
            assert!(display_width(row) <= 80, "{text}");
            assert!(row.starts_with('│') && row.ends_with('│'), "{text}");
        }
        let words: Vec<&str> = rows
            .iter()
            .map(|r| r.trim_start_matches('│').trim_end_matches('│'))
            .flat_map(str::split_whitespace)
            .collect();
        let expected: Vec<&str> = intro.split_whitespace().collect();
        assert_eq!(words[..expected.len()], expected[..], "{text}");
    }

    #[test]
    fn hit_flash_marks_lost_health() {
        // Find an event whose target survives with a visible health bar.
        let f = four_versus_four();
        let hit = f.outcome.timeline.iter().position(|e| match &e.action {
            AppliedAction::Targeted(t) => e.state.targeted_member(&t.target).on_field(),
            _ => false,
        });
        let Some(hit) = hit else {
            panic!("no surviving target in the 4v4 timeline");
        };
        let rounds: Vec<usize> = f.outcome.timeline.iter().map(|e| e.round).collect();
        let still = f.app(hit + 1, true);
        // Replay with ticks so the flash of the last event is still running.
        let mut flashing = App::new(rounds, true);
        while flashing.shown() < hit + 1 {
            flashing.tick(std::time::Duration::from_millis(50));
        }
        assert!(flashing.flashing());
        let full = |t: &str| t.matches('█').count();
        let still_text = f.render(&still, 80, 24);
        let flash_text = f.render(&flashing, 80, 24);
        assert!(
            full(&flash_text) > full(&still_text),
            "{still_text}\n{flash_text}"
        );
    }
}
