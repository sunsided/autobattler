//! Turns a solved conflict into a round-based combat log.

mod flavor;
mod panel;
pub mod playback;
mod reasoning;

use crate::action::{Action, AppliedAction};
use crate::conflict::Conflict;
use crate::party::{Participant, Party};
use crate::party_member::{Morale, PartyMember};
use crate::solver::{Event, Outcome};
use colored::{ColoredString, Colorize};
use flavor::FleeReason;

/// The kind of a narration beat, so playback can pace beats differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatKind {
    /// The opening line.
    Intro,
    /// The start of a round with its initiative order.
    RoundHeader,
    /// A single action, optionally with the tactician's note.
    Action,
    /// The party status after a round.
    Panel,
    /// The closing line.
    Outro,
    /// Search statistics.
    Stats,
}

/// A group of lines that belong together.
pub struct Beat {
    /// What the lines describe.
    pub kind: BeatKind,
    /// The rendered lines.
    pub lines: Vec<String>,
    /// The timeline event the beat belongs to, if any.
    ///
    /// A round header points at the round's first event and a panel at its
    /// last; intro, outro and stats belong to no event.
    pub event: Option<usize>,
}

/// The log lines visible once `shown` events have played.
///
/// These are the intro lines plus, for every action beat of an event below
/// `shown`, its first line and, when `explain` is set, the remaining ones.
/// Empty lines are dropped.
#[allow(dead_code)]
pub fn log_lines(beats: &[Beat], shown: usize, explain: bool) -> Vec<String> {
    let intro = beats.iter().filter(|b| b.kind == BeatKind::Intro);
    let actions = beats
        .iter()
        .filter(|b| b.kind == BeatKind::Action && b.event.is_some_and(|e| e < shown));
    let mut lines: Vec<String> = intro.flat_map(|b| b.lines.clone()).collect();
    for beat in actions {
        let take = if explain { beat.lines.len() } else { 1 };
        lines.extend(beat.lines.iter().take(take).cloned());
    }
    lines.retain(|line| !line.is_empty());
    lines
}

/// Controls what the narrator includes.
pub struct NarrationOptions {
    /// Adds tactician notes explaining each choice.
    pub explain: bool,
    /// The seed the fight was built from, shown so it can be replayed.
    pub seed: Option<u64>,
}

/// Narrates the timeline of a solved conflict.
pub struct Narrator<'a> {
    names: &'a [Vec<String>],
    options: NarrationOptions,
}

impl<'a> Narrator<'a> {
    /// Creates a narrator using `names[party_id][member_id]` as display names.
    pub fn new(names: &'a [Vec<String>], options: NarrationOptions) -> Self {
        Self { names, options }
    }

    /// Builds the combat log for `outcome`, starting from `start`.
    pub fn narrate(&self, start: &Conflict, outcome: &Outcome) -> Vec<Beat> {
        let heroes = self.party_names(start.initiator.id);
        let villains = self.party_names(start.opponent.id);
        let mut beats = vec![Beat {
            kind: BeatKind::Intro,
            lines: vec![flavor::intro(&heroes, &villains)],
            event: None,
        }];

        let timeline = &outcome.timeline;
        if timeline.is_empty() {
            beats.push(Beat {
                kind: BeatKind::Action,
                lines: vec!["Nothing happens.".to_string()],
                event: None,
            });
        }

        for (i, event) in timeline.iter().enumerate() {
            let previous = i.checked_sub(1).map(|p| &timeline[p]);
            if previous.is_none_or(|p| p.round != event.round) {
                let state = previous.map_or(start, |p| &p.state);
                beats.push(Beat {
                    kind: BeatKind::RoundHeader,
                    lines: vec![String::new(), self.round_header(event.round, state)],
                    event: Some(i),
                });
            }

            // A flight starts the parting blows; every action after it is one.
            let parting = previous.is_some_and(|p| p.state.parting.is_some());
            let before = previous.map_or(start, |p| &p.state);
            beats.push(Beat {
                kind: BeatKind::Action,
                lines: self.action_lines(start, before, event, parting),
                event: Some(i),
            });

            if timeline.get(i + 1).is_none_or(|n| n.round != event.round) {
                let mut lines = vec![String::new()];
                lines.extend(panel::panel_lines(start, &event.state, self.names));
                beats.push(Beat {
                    kind: BeatKind::Panel,
                    lines,
                    event: Some(i),
                });
            }
        }

        let rounds = timeline.last().map_or(0, |e| e.round);
        let end = timeline.last().map_or(start, |e| &e.state);
        let heroes = self.survivor_names(&end.initiator);
        let villains = self.survivor_names(&end.opponent);
        beats.push(Beat {
            kind: BeatKind::Outro,
            lines: vec![
                String::new(),
                flavor::outro(&outcome.outcome, &heroes, &villains, rounds),
            ],
            event: None,
        });
        beats.push(Beat {
            kind: BeatKind::Stats,
            lines: vec![format!(
                "({}{} evaluations, {} cuts, depth {}, {:?})",
                self.options
                    .seed
                    .map_or(String::new(), |seed| format!("seed {seed}, ")),
                outcome.evaluations,
                outcome.cuts,
                outcome.max_visited_depth,
                outcome.search_duration
            )],
            event: None,
        });
        beats
    }

    /// Lists the members able to act in `state`, in initiative order.
    fn round_header(&self, round: usize, state: &Conflict) -> String {
        let order: Vec<String> = state
            .initiative_order()
            .iter()
            .filter_map(|p| {
                let member = state.targeted_member(p);
                member
                    .can_act()
                    .then(|| format!("{}({})", self.name(p), member.speed))
            })
            .collect();
        format!("══ Round {round} ══  initiative: {}", order.join(" → "))
    }

    /// Describes what happened in an event, plus the tactician note if enabled.
    ///
    /// `before` is the state the event's action was applied to.
    fn action_lines(
        &self,
        start: &Conflict,
        before: &Conflict,
        event: &Event,
        parting: bool,
    ) -> Vec<String> {
        let initiator = start.initiator.id;
        let actor = self.colored_name(initiator, &event.actor);
        let line = match &event.action {
            AppliedAction::Targeted(targeted) => {
                let Action::SimpleAttack(attack) = &targeted.action;
                let target = self.colored_name(initiator, &targeted.target);
                let mut line = format!(
                    "{}{actor} {} {target}{} — {} damage.",
                    if parting { "Parting blow: " } else { "" },
                    flavor::attack_verb(attack, event.round, event.slot),
                    flavor::instrument(attack),
                    format!("{}", attack.damage).yellow()
                );
                if event.state.targeted_member(&targeted.target).is_dead() {
                    line.push_str(&format!(
                        " {target} {}",
                        flavor::kill_phrase(event.round, event.slot)
                    ));
                }
                line
            }
            AppliedAction::Flee => {
                let member = before.targeted_member(&event.actor);
                let reason = match member.morale {
                    Morale::Courage(_) if member.must_flee(event.round) => FleeReason::Courage,
                    Morale::AfterRounds(_) if member.must_flee(event.round) => FleeReason::Rounds,
                    _ => FleeReason::Choice,
                };
                flavor::flee_line(&actor.to_string(), reason)
            }
            AppliedAction::Retreat => {
                // Only those who were still on the field leave with the retreat.
                let was_on_field = before.action_target(event.actor.party_id);
                let party = event.state.action_target(event.actor.party_id);
                let fleeing: Vec<Participant> = party
                    .members
                    .iter()
                    .filter(|m| m.fled && was_on_field.members[m.id].on_field())
                    .map(|m| Participant {
                        party_id: party.id,
                        member_id: m.id,
                    })
                    .collect();
                let actor_alone = fleeing == std::slice::from_ref(&event.actor);
                let fleeing: Vec<String> = fleeing
                    .iter()
                    .map(|p| self.colored_name(initiator, p).to_string())
                    .collect();
                let fleeing: Vec<&str> = fleeing.iter().map(String::as_str).collect();
                // Those who stayed behind hold the line.
                let holding: Vec<String> = party
                    .members
                    .iter()
                    .filter(|m| m.on_field())
                    .map(|m| {
                        let p = Participant {
                            party_id: party.id,
                            member_id: m.id,
                        };
                        self.colored_name(initiator, &p).to_string()
                    })
                    .collect();
                let holding: Vec<&str> = holding.iter().map(String::as_str).collect();
                retreat_line(&actor.to_string(), &fleeing, actor_alone, &holding)
            }
        };

        let mut lines = vec![line];
        if self.options.explain {
            lines.extend(reasoning::tactician_lines(
                event,
                event.actor.party_id == initiator,
                self.names,
            ));
        }
        lines
    }

    fn name(&self, p: &Participant) -> &str {
        &self.names[p.party_id][p.member_id]
    }

    /// Colors initiator names blue and opponent names purple.
    fn colored_name(&self, initiator: usize, p: &Participant) -> ColoredString {
        if p.party_id == initiator {
            self.name(p).blue()
        } else {
            self.name(p).purple()
        }
    }

    fn party_names(&self, party_id: usize) -> Vec<&str> {
        self.names[party_id].iter().map(String::as_str).collect()
    }

    /// Names the members of `party` still on the field, else the living ones,
    /// else all of them.
    fn survivor_names(&self, party: &Party) -> Vec<&str> {
        let names = |keep: fn(&PartyMember) -> bool| -> Vec<&str> {
            party
                .members
                .iter()
                .filter(|m| keep(m))
                .map(|m| self.names[party.id][m.id].as_str())
                .collect()
        };
        let on_field = names(PartyMember::on_field);
        if !on_field.is_empty() {
            return on_field;
        }
        let alive = names(|m| !m.is_dead());
        if alive.is_empty() {
            self.party_names(party.id)
        } else {
            alive
        }
    }
}

/// Announces a retreat; `actor_alone` means the actor is the only one fleeing,
/// and `holding` are those who stayed behind.
fn retreat_line(actor: &str, fleeing: &[&str], actor_alone: bool, holding: &[&str]) -> String {
    let mut line = if actor_alone {
        format!("{actor} sounds the retreat and flees!")
    } else {
        let verb = if fleeing.len() == 1 { "flees" } else { "flee" };
        format!(
            "{actor} sounds the retreat — {} {verb}!",
            flavor::join_names(fleeing)
        )
    };
    if !holding.is_empty() {
        let verb = if holding.len() == 1 { "holds" } else { "hold" };
        line.push_str(&format!(
            " {} {verb} the line.",
            flavor::join_names(holding)
        ));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict::Conflict;
    use crate::party_member::{Morale, PartyMember};
    use crate::scenario;
    use crate::solver::{Solver, SolverStrategy, MAX_ROUNDS};
    use crate::weapon::{Fists, Weapon};

    fn names() -> Vec<Vec<String>> {
        vec![
            vec!["Brull".to_string()],
            vec!["Ziuon".to_string(), "Molphige".to_string()],
        ]
    }

    fn render_conflict(conflict: &crate::conflict::Conflict, explain: bool) -> String {
        render_with(conflict, &names(), explain)
    }

    fn render_with(
        conflict: &crate::conflict::Conflict,
        names: &[Vec<String>],
        explain: bool,
    ) -> String {
        colored::control::set_override(false);
        let outcome = Solver::play(conflict, SolverStrategy::IterativeDeepening(10), MAX_ROUNDS);
        let narrator = Narrator::new(
            names,
            NarrationOptions {
                explain,
                seed: None,
            },
        );
        let mut text = narrator
            .narrate(conflict, &outcome)
            .into_iter()
            .filter(|beat| beat.kind != BeatKind::Stats)
            .flat_map(|beat| beat.lines)
            .collect::<Vec<_>>()
            .join("\n");
        text.push('\n');
        text
    }

    fn render(explain: bool) -> String {
        render_conflict(&scenario::default_conflict(), explain)
    }

    #[test]
    fn golden_with_explanations() {
        assert_eq!(render(true), include_str!("golden/explain.txt"));
    }

    #[test]
    fn golden_without_explanations() {
        assert_eq!(render(false), include_str!("golden/plain.txt"));
    }

    #[test]
    fn first_round_header_lists_initiative() {
        assert!(
            render(false).contains("══ Round 1 ══  initiative: Brull(6) → Molphige(5) → Ziuon(3)")
        );
    }

    #[test]
    fn plain_has_no_tactician() {
        assert!(!render(false).contains("tactician"));
    }

    #[test]
    fn stats_line_shows_the_seed() {
        let names = names();
        let conflict = scenario::default_conflict();
        let outcome = Solver::engage(&conflict, SolverStrategy::IterativeDeepening(10));
        let stats = |seed| {
            let options = NarrationOptions {
                explain: false,
                seed,
            };
            let beats = Narrator::new(&names, options).narrate(&conflict, &outcome);
            let last = beats.last().expect("stats beat");
            assert_eq!(last.kind, BeatKind::Stats);
            last.lines[0].clone()
        };
        assert!(stats(Some(42)).starts_with("(seed 42, "));
        assert!(stats(None).starts_with("(") && !stats(None).contains("seed"));
    }

    #[test]
    fn parting_blows_are_marked() {
        let text = render(false);
        let flight = text.find("Ziuon flees!").expect("a flight");
        let parting = text.find("Parting blow: ").expect("a parting blow");
        assert!(parting > flight);
        assert_eq!(text.matches("Parting blow: ").count(), 1);
    }

    #[test]
    fn narrates_nothing_for_empty_timeline() {
        let mut conflict = scenario::default_conflict();
        for member in conflict.opponent.members.iter_mut() {
            member.health = 0.0;
        }
        let text = render_conflict(&conflict, true);
        assert!(text.contains("Nothing happens."), "{text}");
        assert!(!text.contains("Round"), "{text}");

        let names = names();
        let outcome = Solver::engage(&conflict, SolverStrategy::IterativeDeepening(10));
        let kinds: Vec<BeatKind> = Narrator::new(
            &names,
            NarrationOptions {
                explain: true,
                seed: None,
            },
        )
        .narrate(&conflict, &outcome)
        .iter()
        .map(|beat| beat.kind)
        .collect();
        assert_eq!(
            kinds,
            vec![
                BeatKind::Intro,
                BeatKind::Action,
                BeatKind::Outro,
                BeatKind::Stats
            ]
        );
    }

    #[test]
    fn retreat_line_when_actor_flees_alone() {
        assert_eq!(
            retreat_line("Ziuon", &["Ziuon"], true, &[]),
            "Ziuon sounds the retreat and flees!"
        );
    }

    #[test]
    fn retreat_line_names_the_fleeing_party() {
        assert_eq!(
            retreat_line("Ziuon", &["Ziuon", "Molphige"], false, &[]),
            "Ziuon sounds the retreat — Ziuon and Molphige flee!"
        );
        assert_eq!(
            retreat_line("Ziuon", &["Molphige"], false, &[]),
            "Ziuon sounds the retreat — Molphige flees!"
        );
    }

    #[test]
    fn retreat_line_names_those_holding_the_line() {
        assert_eq!(
            retreat_line("Ziuon", &["Ziuon"], true, &["Molphige"]),
            "Ziuon sounds the retreat and flees! Molphige holds the line."
        );
        assert_eq!(
            retreat_line("Brull", &["Brull"], true, &["Ziuon", "Molphige"]),
            "Brull sounds the retreat and flees! Ziuon and Molphige hold the line."
        );
    }

    fn fighter(health: f32, damage: f32, speed: u32, morale: Morale) -> PartyMember {
        PartyMember {
            id: 0,
            health,
            damage_taken: 0.0,
            weapon: Weapon::Fists(Fists { damage }),
            can_act: true,
            speed,
            morale,
            fled: false,
        }
    }

    #[test]
    fn narrates_a_forced_flight() {
        colored::control::set_override(false);
        let conflict = Conflict::new(
            Party {
                id: 0,
                members: vec![fighter(20.0, 10.0, 6, Morale::Tactical)],
                can_retreat: false,
            },
            Party {
                id: 1,
                members: vec![fighter(25.0, 10.0, 3, Morale::Courage(1.0))],
                can_retreat: false,
            },
        );
        let names = vec![vec!["Brull".to_string()], vec!["Ziuon".to_string()]];
        let text = render_with(&conflict, &names, false);
        let flight = text.find("'s courage breaks — they flee!").expect(&text);
        let parting = text.find("Parting blow: ").expect(&text);
        assert!(parting > flight, "{text}");
    }

    #[test]
    fn round_header_omits_fled_members() {
        let mut conflict = scenario::default_conflict();
        conflict.opponent.members[0].fled = true;
        let names = names();
        let narrator = Narrator::new(
            &names,
            NarrationOptions {
                explain: false,
                seed: None,
            },
        );
        let header = narrator.round_header(1, &conflict);
        assert!(!header.contains("Ziuon"), "{header}");
        assert!(header.contains("Molphige"), "{header}");
    }

    fn ambush_beats() -> Vec<Beat> {
        let names = names();
        let conflict = scenario::default_conflict();
        let outcome = Solver::play(
            &conflict,
            SolverStrategy::IterativeDeepening(10),
            MAX_ROUNDS,
        );
        let options = NarrationOptions {
            explain: true,
            seed: None,
        };
        Narrator::new(&names, options).narrate(&conflict, &outcome)
    }

    #[test]
    fn beats_carry_event_indices() {
        colored::control::set_override(false);
        // golden/explain.txt shows one round with three actions: Brull's jab,
        // Ziuon's flight and Brull's parting blow, i.e. events 0, 1 and 2.
        let tagged: Vec<(BeatKind, Option<usize>)> = ambush_beats()
            .iter()
            .map(|beat| (beat.kind, beat.event))
            .collect();
        assert_eq!(
            tagged,
            vec![
                (BeatKind::Intro, None),
                (BeatKind::RoundHeader, Some(0)),
                (BeatKind::Action, Some(0)),
                (BeatKind::Action, Some(1)),
                (BeatKind::Action, Some(2)),
                (BeatKind::Panel, Some(2)),
                (BeatKind::Outro, None),
                (BeatKind::Stats, None),
            ]
        );
    }

    #[test]
    fn log_lines_match_the_text_log() {
        colored::control::set_override(false);
        let beats = ambush_beats();
        let intro: Vec<String> = beats[0].lines.clone();
        let actions: Vec<&Beat> = beats
            .iter()
            .filter(|beat| beat.kind == BeatKind::Action)
            .collect();
        for shown in 0..=actions.len() {
            let mut expected = intro.clone();
            let mut brief = intro.clone();
            for beat in &actions[..shown] {
                expected.extend(beat.lines.clone());
                brief.push(beat.lines[0].clone());
            }
            expected.retain(|line| !line.is_empty());
            brief.retain(|line| !line.is_empty());
            assert_eq!(log_lines(&beats, shown, true), expected, "shown {shown}");
            assert_eq!(log_lines(&beats, shown, false), brief, "shown {shown}");
        }
    }
}
