//! Tactician notes: explain why the AI chose an action and what it passed up.

use super::flavor::base_verb;
use crate::action::{Action, AppliedAction};
use crate::solver::Event;
use crate::value::{Bound, TerminalState};

/// Number of alternatives listed in a note.
const SHOWN_ALTERNATIVES: usize = 2;

/// Describes an outcome from the acting side's perspective.
///
/// Scores are the initiator's utility, so the words flip when the opponent acts.
pub(crate) fn outcome_words(score: &TerminalState, actor_is_initiator: bool) -> &'static str {
    match (score, actor_is_initiator) {
        (TerminalState::Win(_), true) => "a win",
        (TerminalState::Win(_), false) => "defeat",
        (TerminalState::Defeat(_), true) => "defeat",
        (TerminalState::Defeat(_), false) => "a win",
        (TerminalState::Remain(_), true) => "they escape",
        (TerminalState::Remain(_), false) => "escape",
        (TerminalState::Retreat(_), true) => "escape",
        (TerminalState::Retreat(_), false) => "they escape",
        (TerminalState::Heuristic(_) | TerminalState::OpenUnexplored(_), _) => "unclear",
    }
}

/// Renders a score, marking it as a bound or approximation where it is not exact.
pub(crate) fn score_text(score: &TerminalState, bound: Bound) -> String {
    let value = score.value();
    match bound {
        Bound::Exact => format!("{value}"),
        Bound::AtMost => format!("≤ {value}"),
        Bound::AtLeast => format!("≥ {value}"),
        Bound::Approximate => format!("≈ {value}"),
    }
}

/// Names an action briefly, e.g. "punch Ziuon" or "order retreat".
pub(crate) fn action_label(action: &AppliedAction, names: &[Vec<String>]) -> String {
    match action {
        AppliedAction::Flee => "order retreat".to_string(),
        AppliedAction::Targeted(targeted) => {
            let Action::SimpleAttack(attack) = &targeted.action;
            let target = &names[targeted.target.party_id][targeted.target.member_id];
            format!("{} {}", base_verb(attack), target)
        }
    }
}

/// Builds the tactician note for an event: the expectation, up to two
/// alternatives that were passed up, and a summary of the rest.
pub(crate) fn tactician_lines(
    event: &Event,
    actor_is_initiator: bool,
    names: &[Vec<String>],
) -> Vec<String> {
    let mut lines = vec![format!(
        "   ↳ tactician: expects {} ({})",
        outcome_words(&event.score, actor_is_initiator),
        score_text(&event.score, event.bound)
    )];

    let shown: Vec<_> = event
        .alternatives
        .iter()
        .take(SHOWN_ALTERNATIVES)
        .map(|alt| {
            (
                action_label(&alt.action, names),
                outcome_words(&alt.score, actor_is_initiator),
                alt,
            )
        })
        .collect();
    let label_width = shown
        .iter()
        .map(|(label, _, _)| label.chars().count())
        .max()
        .unwrap_or(0);
    let words_width = shown
        .iter()
        .map(|(_, words, _)| words.chars().count())
        .max()
        .unwrap_or(0);
    for (label, words, alt) in &shown {
        lines.push(format!(
            "       alt: {label:<label_width$} → {words:<words_width$}  ({})",
            score_text(&alt.score, alt.bound)
        ));
    }

    let more = event.alternatives.len().saturating_sub(SHOWN_ALTERNATIVES);
    let mut parts = Vec::new();
    if more > 0 {
        parts.push(format!("+{more} more"));
    }
    if event.unexamined > 0 {
        parts.push(format!("{} not examined", event.unexamined));
    }
    if !parts.is_empty() {
        lines.push(format!("       {}", parts.join(", ")));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{SimpleAttackAction, TargetedAction};
    use crate::conflict::Conflict;
    use crate::party::{Participant, Party};
    use crate::party_member::PartyMember;
    use crate::solver::Alternative;
    use crate::weapon::{Fists, Weapon};

    fn names() -> Vec<Vec<String>> {
        vec![
            vec!["Brull".to_string()],
            vec!["Ziuon".to_string(), "Molphige".to_string()],
        ]
    }

    fn attack(weapon: Option<Weapon>, target_member: usize) -> AppliedAction {
        AppliedAction::Targeted(TargetedAction {
            action: Action::SimpleAttack(SimpleAttackAction {
                weapon,
                damage: 1.0,
            }),
            source: Participant {
                party_id: 0,
                member_id: 0,
            },
            target: Participant {
                party_id: 1,
                member_id: target_member,
            },
        })
    }

    fn fists() -> Option<Weapon> {
        Some(Weapon::Fists(Fists { damage: 1.0 }))
    }

    fn alt(action: AppliedAction, score: TerminalState, bound: Bound) -> Alternative {
        Alternative {
            action,
            score,
            bound,
        }
    }

    fn event(alternatives: Vec<Alternative>, unexamined: usize) -> Event {
        let member = |id| PartyMember {
            id,
            health: 10.0,
            damage_taken: 0.0,
            weapon: Weapon::Fists(Fists { damage: 1.0 }),
            can_act: true,
            speed: 1,
        };
        let party = |id, members| Party {
            id,
            members,
            can_retreat: true,
            retreated: false,
        };
        Event {
            round: 1,
            slot: 0,
            actor: Participant {
                party_id: 0,
                member_id: 0,
            },
            action: attack(fists(), 0),
            state: Conflict::new(
                party(0, vec![member(0)]),
                party(1, vec![member(0), member(1)]),
            ),
            score: TerminalState::Win(10.0),
            bound: Bound::Exact,
            alternatives,
            unexamined,
        }
    }

    #[test]
    fn outcome_words_table() {
        let cases: [(TerminalState, &str, &str); 6] = [
            (TerminalState::Win(1.0), "a win", "defeat"),
            (TerminalState::Defeat(1.0), "defeat", "a win"),
            (TerminalState::Remain(1.0), "they escape", "escape"),
            (TerminalState::Retreat(1.0), "escape", "they escape"),
            (TerminalState::Heuristic(1.0), "unclear", "unclear"),
            (TerminalState::OpenUnexplored(1.0), "unclear", "unclear"),
        ];
        for (score, initiator, opponent) in cases {
            assert_eq!(outcome_words(&score, true), initiator, "{score:?}");
            assert_eq!(outcome_words(&score, false), opponent, "{score:?}");
        }
    }

    #[test]
    fn score_text_marks_bounds() {
        let s = TerminalState::Win(10.0);
        assert_eq!(score_text(&s, Bound::Exact), "10");
        assert_eq!(
            score_text(&TerminalState::Win(-20.0), Bound::AtMost),
            "≤ -20"
        );
        assert_eq!(score_text(&TerminalState::Win(2.0), Bound::AtLeast), "≥ 2");
        assert_eq!(
            score_text(&TerminalState::Win(3.0), Bound::Approximate),
            "≈ 3"
        );
    }

    #[test]
    fn action_labels() {
        let names = names();
        assert_eq!(action_label(&attack(fists(), 0), &names), "punch Ziuon");
        assert_eq!(action_label(&attack(None, 1), &names), "glare at Molphige");
        assert_eq!(action_label(&AppliedAction::Flee, &names), "order retreat");
    }

    #[test]
    fn note_lists_two_alternatives_and_a_summary() {
        let alts = vec![
            alt(attack(fists(), 0), TerminalState::Win(8.0), Bound::Exact),
            alt(attack(None, 1), TerminalState::Defeat(-20.0), Bound::AtMost),
            alt(
                AppliedAction::Flee,
                TerminalState::Retreat(-5.0),
                Bound::Exact,
            ),
            alt(attack(None, 0), TerminalState::Heuristic(0.0), Bound::Exact),
        ];
        assert_eq!(
            tactician_lines(&event(alts, 1), true, &names()),
            vec![
                "   ↳ tactician: expects a win (10)".to_string(),
                "       alt: punch Ziuon       → a win   (8)".to_string(),
                "       alt: glare at Molphige → defeat  (≤ -20)".to_string(),
                "       +2 more, 1 not examined".to_string(),
            ]
        );
    }

    #[test]
    fn alternative_scores_line_up() {
        let alts = vec![
            alt(attack(None, 0), TerminalState::Remain(1.0), Bound::AtMost),
            alt(
                AppliedAction::Flee,
                TerminalState::Retreat(-1.0),
                Bound::AtMost,
            ),
        ];
        assert_eq!(
            tactician_lines(&event(alts, 0), true, &names())[1..],
            [
                "       alt: glare at Ziuon → they escape  (≤ 1)".to_string(),
                "       alt: order retreat  → escape       (≤ -1)".to_string(),
            ]
        );
    }

    #[test]
    fn bare_note_is_a_single_line() {
        assert_eq!(
            tactician_lines(&event(vec![], 0), false, &names()),
            vec!["   ↳ tactician: expects defeat (10)".to_string()]
        );
    }

    #[test]
    fn summary_drops_zero_parts() {
        let only_unexamined = tactician_lines(&event(vec![], 3), true, &names());
        assert_eq!(only_unexamined[1], "       3 not examined");
    }
}
