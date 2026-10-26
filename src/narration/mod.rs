//! Turns a solved conflict into a round-based combat log.

mod flavor;
mod panel;
mod reasoning;

use crate::action::{Action, AppliedAction};
use crate::conflict::Conflict;
use crate::party::{Participant, Party};
use crate::solver::{Event, Outcome};
use colored::{ColoredString, Colorize};

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
}

/// Controls what the narrator includes.
pub struct NarrationOptions {
    /// Adds tactician notes explaining each choice.
    pub explain: bool,
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
        }];

        let timeline = &outcome.timeline;
        if timeline.is_empty() {
            beats.push(Beat {
                kind: BeatKind::Action,
                lines: vec!["Nothing happens.".to_string()],
            });
        }

        for (i, event) in timeline.iter().enumerate() {
            let previous = i.checked_sub(1).map(|p| &timeline[p]);
            if previous.is_none_or(|p| p.round != event.round) {
                let state = previous.map_or(start, |p| &p.state);
                beats.push(Beat {
                    kind: BeatKind::RoundHeader,
                    lines: vec![String::new(), self.round_header(event.round, state)],
                });
            }

            beats.push(Beat {
                kind: BeatKind::Action,
                lines: self.action_lines(start, event),
            });

            if timeline.get(i + 1).is_none_or(|n| n.round != event.round) {
                let mut lines = vec![String::new()];
                lines.extend(panel::panel_lines(start, &event.state, self.names));
                beats.push(Beat {
                    kind: BeatKind::Panel,
                    lines,
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
        });
        beats.push(Beat {
            kind: BeatKind::Stats,
            lines: vec![format!(
                "({} evaluations, {} cuts, depth {}, {:?})",
                outcome.evaluations,
                outcome.cuts,
                outcome.max_visited_depth,
                outcome.search_duration
            )],
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
    fn action_lines(&self, start: &Conflict, event: &Event) -> Vec<String> {
        let initiator = start.initiator.id;
        let actor = self.colored_name(initiator, &event.actor);
        let line = match &event.action {
            AppliedAction::Targeted(targeted) => {
                let Action::SimpleAttack(attack) = &targeted.action;
                let target = self.colored_name(initiator, &targeted.target);
                let mut line = format!(
                    "{actor} {} {target}{} — {} damage.",
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
                let party = event.state.action_target(event.actor.party_id);
                let fleeing: Vec<String> = party
                    .members
                    .iter()
                    .filter(|m| !m.is_dead())
                    .map(|m| {
                        let p = Participant {
                            party_id: party.id,
                            member_id: m.id,
                        };
                        self.colored_name(initiator, &p).to_string()
                    })
                    .collect();
                let fleeing: Vec<&str> = fleeing.iter().map(String::as_str).collect();
                let verb = if fleeing.len() == 1 { "flees" } else { "flee" };
                format!(
                    "{actor} sounds the retreat — {} {verb}!",
                    flavor::join_names(&fleeing)
                )
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

    /// Names the living members of `party`, or all of them if none survived.
    fn survivor_names(&self, party: &Party) -> Vec<&str> {
        let alive: Vec<&str> = party
            .members
            .iter()
            .filter(|m| !m.is_dead())
            .map(|m| self.names[party.id][m.id].as_str())
            .collect();
        if alive.is_empty() {
            self.party_names(party.id)
        } else {
            alive
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario;
    use crate::solver::{Solver, SolverStrategy};

    fn names() -> Vec<Vec<String>> {
        vec![
            vec!["Brull".to_string()],
            vec!["Ziuon".to_string(), "Molphige".to_string()],
        ]
    }

    fn render_conflict(conflict: &crate::conflict::Conflict, explain: bool) -> String {
        colored::control::set_override(false);
        let names = names();
        let outcome = Solver::engage(conflict, SolverStrategy::IterativeDeepening(10));
        let narrator = Narrator::new(&names, NarrationOptions { explain });
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
    fn narrates_nothing_for_empty_timeline() {
        let mut conflict = scenario::default_conflict();
        for member in conflict.opponent.members.iter_mut() {
            member.health = 0.0;
        }
        let text = render_conflict(&conflict, true);
        assert!(text.contains("Nothing happens."), "{text}");
        assert!(!text.contains("Round"), "{text}");
    }
}
