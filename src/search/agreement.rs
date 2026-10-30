//! Whether two searches of the same position reached the same result.

use crate::solver::{Outcome, OutcomeType};
use crate::value::Bound;
use std::mem::discriminant;

/// How far apart two scores may be and still count as equal.
const TOLERANCE: f32 = 1e-4;

/// Whether two root results agree: the same kind of outcome and, for the
/// first event of each, scores whose allowed ranges overlap.
///
/// Without a first event on both sides, the outcome scores are compared.
pub fn agree(a: &Outcome, b: &Outcome) -> bool {
    if discriminant(&a.outcome) != discriminant(&b.outcome) {
        return false;
    }
    match (a.timeline.first(), b.timeline.first()) {
        (Some(ea), Some(eb)) => {
            ranges_overlap(ea.score.value(), ea.bound, eb.score.value(), eb.bound)
        }
        _ => {
            let value = |outcome: &Outcome| match outcome.outcome {
                OutcomeType::Win(v)
                | OutcomeType::Lose(v)
                | OutcomeType::Remain(v)
                | OutcomeType::Retreat(v)
                | OutcomeType::Unknown(v) => v,
            };
            ranges_overlap(value(a), Bound::Exact, value(b), Bound::Exact)
        }
    }
}

/// Whether the ranges allowed by two scores and their bounds overlap, give
/// or take the tolerance.
fn ranges_overlap(a: f32, a_bound: Bound, b: f32, b_bound: Bound) -> bool {
    let (a_low, a_high) = range(a, a_bound);
    let (b_low, b_high) = range(b, b_bound);
    a_low <= b_high + TOLERANCE && b_low <= a_high + TOLERANCE
}

/// The values a score with `bound` allows.
fn range(score: f32, bound: Bound) -> (f32, f32) {
    match bound {
        Bound::Exact => (score, score),
        Bound::AtMost => (f32::NEG_INFINITY, score),
        Bound::AtLeast => (score, f32::INFINITY),
        Bound::Approximate => (f32::NEG_INFINITY, f32::INFINITY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict::Conflict;
    use crate::party::Party;
    use crate::search::SearchKind;
    use crate::solver::Event;
    use crate::value::TerminalState;
    use std::time::Duration;

    fn outcome(outcome: OutcomeType) -> Outcome {
        Outcome {
            outcome,
            timeline: Vec::new(),
            evaluations: 0,
            cuts: 0,
            search_duration: Duration::ZERO,
            max_visited_depth: 0,
            depth_limited: false,
            search: SearchKind::Basic,
        }
    }

    #[test]
    fn ranges_overlap_by_bound() {
        use Bound::*;
        let table = [
            (1.0, Exact, 1.0, Exact, true),
            (1.0, Exact, 1.00005, Exact, true),
            (1.0, Exact, 2.0, Exact, false),
            (1.0, AtMost, 2.0, Exact, false),
            (2.0, AtMost, 1.0, Exact, true),
            (1.0, AtLeast, 2.0, Exact, true),
            (2.0, AtLeast, 1.0, Exact, false),
            (2.0, AtLeast, 1.0, AtMost, false),
            (1.0, AtLeast, 2.0, AtMost, true),
            (1.0, AtMost, 1.00005, AtLeast, true),
            (5.0, Approximate, -3.0, Exact, true),
            (5.0, Approximate, -3.0, AtMost, true),
        ];
        for (a, a_bound, b, b_bound, expected) in table {
            assert_eq!(
                ranges_overlap(a, a_bound, b, b_bound),
                expected,
                "{a} {a_bound:?} vs {b} {b_bound:?}"
            );
            assert_eq!(
                ranges_overlap(b, b_bound, a, a_bound),
                expected,
                "{b} {b_bound:?} vs {a} {a_bound:?}, swapped"
            );
        }
    }

    #[test]
    fn empty_timelines_compare_outcome_scores() {
        let win = |v| outcome(OutcomeType::Win(v));
        assert!(agree(&win(1.0), &win(1.00005)));
        assert!(!agree(&win(1.0), &win(2.0)));
    }

    #[test]
    fn different_outcome_kinds_disagree() {
        let win = outcome(OutcomeType::Win(1.0));
        let lose = outcome(OutcomeType::Lose(1.0));
        assert!(!agree(&win, &lose));
    }

    fn with_first_event(kind: OutcomeType, score: f32, bound: Bound) -> Outcome {
        let party = |id| Party {
            id,
            members: Vec::new(),
            can_retreat: false,
        };
        let state = Conflict::new(party(0), party(1));
        let mut outcome = outcome(kind);
        outcome.timeline.push(Event {
            round: 1,
            slot: 0,
            actor: crate::party::Participant {
                party_id: 0,
                member_id: 0,
            },
            action: crate::action::AppliedAction::Flee,
            state,
            score: TerminalState::Heuristic(score),
            bound,
            alternatives: Vec::new(),
            unexamined: 0,
        });
        outcome
    }

    #[test]
    fn first_events_compare_by_score_range() {
        let remain = |score, bound| with_first_event(OutcomeType::Remain(0.0), score, bound);
        assert!(agree(
            &remain(1.0, Bound::Exact),
            &remain(0.5, Bound::AtLeast)
        ));
        assert!(!agree(
            &remain(1.0, Bound::Exact),
            &remain(2.0, Bound::Exact)
        ));
    }
}
