//! A recursive alpha-beta search, in kinds that add speedups on top of each other.

mod agreement;
mod alphabeta;
pub mod bench;
mod mtdf;
mod ordering;
pub mod positions;
mod table;

use crate::conflict::Conflict;
use crate::solver::{
    child_state, is_maximizing, moves, outcome_type, Alternative, Event, Outcome, OutcomeType,
};
use crate::utility_value::get_utility;
use alphabeta::{RootResult, Searcher};
use log::trace;
use mtdf::mtdf;
use std::cmp::Ordering;
use std::time::{Duration, Instant};
use table::Table;

/// A search algorithm, from the plain alpha-beta search to the fastest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SearchKind {
    /// Alpha-beta search with moves in their fixed order.
    Basic,
    /// Tries the previous deepening step's best line first.
    Pv,
    /// Also orders the remaining moves, most promising first.
    Ordered,
    /// Also remembers searched positions in a transposition table.
    Tt,
    /// Also narrows the window to null windows around a guess (MTD(f)).
    Mtdf,
}

impl SearchKind {
    /// Every kind, each adding to the one before.
    pub const ALL: [SearchKind; 5] = [
        SearchKind::Basic,
        SearchKind::Pv,
        SearchKind::Ordered,
        SearchKind::Tt,
        SearchKind::Mtdf,
    ];

    /// The name used on the command line.
    pub fn name(self) -> &'static str {
        match self {
            SearchKind::Basic => "basic",
            SearchKind::Pv => "pv",
            SearchKind::Ordered => "ordered",
            SearchKind::Tt => "tt",
            SearchKind::Mtdf => "mtdf",
        }
    }

    /// Looks a kind up by its command-line name.
    pub fn from_name(name: &str) -> Option<SearchKind> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// How to search a position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Search {
    /// The algorithm.
    pub kind: SearchKind,
    /// The deepest the search looks, in combatant actions.
    pub depth: usize,
}

/// Predicts the best line of play from `conflict`, in favor of the initiator.
///
/// Deepens one action at a time up to `search.depth` (at least 1), stopping
/// early once a win is found or the fight ends within the search. The
/// evaluations, cuts and duration are summed over all steps; the depth
/// statistics are those of the last step.
///
/// Each kind adds its speedup on top of the ones before. The transposition
/// table of [`SearchKind::Tt`] lives for the whole call. [`SearchKind::Mtdf`]
/// guesses each step's score from the previous step's (0 for the first); its
/// best line comes from a null-window search, so its timeline holds only the
/// first event.
pub fn engage(conflict: &Conflict, search: Search) -> Outcome {
    let max_depth = search.depth.max(1);
    let mut evaluations = 0;
    let mut cuts = 0;
    let mut search_duration = Duration::ZERO;
    let mut depth = 1;
    let mut pv = Vec::new();
    let mut table = Table::new();
    let mut guess = 0.0;
    loop {
        trace!("Increase search depth to {depth}/{max_depth}");
        let start = Instant::now();
        let mut searcher = Searcher::new(search.kind);
        if search.kind >= SearchKind::Pv {
            searcher.pv = std::mem::take(&mut pv);
        }
        if search.kind >= SearchKind::Tt {
            searcher.table = std::mem::take(&mut table);
        }
        let root = if search.kind == SearchKind::Mtdf {
            mtdf(&mut searcher, conflict, depth, guess)
        } else {
            searcher.root(conflict, depth, f32::NEG_INFINITY, f32::INFINITY)
        };
        table = std::mem::take(&mut searcher.table);
        search_duration += start.elapsed();
        evaluations += searcher.evaluations;
        cuts += searcher.cuts;

        let won = matches!(outcome_type(root.scored.score), OutcomeType::Win(_));
        if won || !searcher.depth_limited || depth >= max_depth {
            trace!("Iterative deepening finished at depth {depth}");
            let outcome = outcome_type(root.scored.score);
            let mut timeline = timeline(conflict, root);
            if search.kind == SearchKind::Mtdf {
                timeline.truncate(1);
            }
            return Outcome {
                outcome,
                timeline,
                evaluations,
                cuts,
                search_duration,
                max_visited_depth: searcher.max_visited_depth,
                depth_limited: searcher.depth_limited,
                search: search.kind,
            };
        }
        pv = root.scored.line.iter().map(|step| step.index).collect();
        guess = root.scored.score.value();
        depth += 1;
    }
}

/// Plays the conflict to its end, searching anew before every action.
///
/// Each search looks ahead as far as `search` allows; only its first move
/// is played. Stops early, with an unknown outcome, once `max_rounds` rounds
/// have been played. Statistics are summed over all searches; the depth is
/// the deepest any search went.
pub fn play(conflict: &Conflict, search: Search, max_rounds: usize) -> Outcome {
    let mut state = conflict.clone();
    let mut timeline = Vec::new();
    let mut evaluations = 0;
    let mut cuts = 0;
    let mut search_duration = Duration::ZERO;
    let mut max_visited_depth = 0;
    let mut depth_limited = false;

    while let Some(turn) = state.next_turn() {
        if turn.round > max_rounds {
            break;
        }
        let outcome = engage(&state, search);
        evaluations += outcome.evaluations;
        cuts += outcome.cuts;
        search_duration += outcome.search_duration;
        max_visited_depth = max_visited_depth.max(outcome.max_visited_depth);
        depth_limited |= outcome.depth_limited;

        let Some(event) = outcome.timeline.into_iter().next() else {
            break;
        };
        state = event.state.clone();
        timeline.push(event);
    }

    Outcome {
        outcome: outcome_type(get_utility(&state)),
        timeline,
        evaluations,
        cuts,
        search_duration,
        max_visited_depth,
        depth_limited,
        search: search.kind,
    }
}

/// Replays the root's best line from `conflict` as events.
///
/// The first event lists the other root actions tried, best first for the
/// mover; later events list none.
fn timeline(conflict: &Conflict, root: RootResult) -> Vec<Event> {
    let mut alternatives = Vec::new();
    let mut unexamined = 0;
    if let Some(turn) = conflict.next_turn() {
        let actor_is_initiator = is_maximizing(conflict, &turn);
        alternatives = root
            .explored
            .into_iter()
            .filter(|(index, _, _)| Some(*index) != root.best)
            .map(|(_, action, scored)| Alternative {
                action,
                score: scored.score,
                bound: scored.bound,
            })
            .collect();
        // Stable, so ties keep the order they were tried in.
        alternatives.sort_by(|a, b| {
            let ordering = a.score.partial_cmp(&b.score).unwrap_or(Ordering::Equal);
            if actor_is_initiator {
                ordering.reverse()
            } else {
                ordering
            }
        });
        if root.best.is_some() {
            unexamined = moves(conflict, &turn).len() - (alternatives.len() + 1);
        }
    }

    let mut events = Vec::with_capacity(root.scored.line.len());
    let mut state = conflict.clone();
    for step in root.scored.line {
        let turn = state
            .next_turn()
            .expect("A best line only continues while the fight does");
        let action = moves(&state, &turn).swap_remove(step.index);
        state = child_state(&state, &turn, &action);
        events.push(Event {
            round: turn.round,
            slot: turn.slot,
            actor: turn.actor,
            action,
            state: state.clone(),
            score: step.score,
            bound: step.bound,
            alternatives: std::mem::take(&mut alternatives),
            unexamined: std::mem::take(&mut unexamined),
        });
    }
    events
}

#[cfg(test)]
mod tests {
    use super::agreement::agree;
    use super::positions::bench_positions;
    use super::*;
    use crate::party::Party;
    use crate::party_member::{Morale, PartyMember};
    use crate::roster::Roster;
    use crate::scenario::{self, Preset, Setup};
    use crate::solver::{outcome_type, Solver, SolverStrategy};
    use crate::utility_value::get_utility;
    use crate::weapon::{Fists, Stick, Weapon};
    use std::mem::discriminant;

    #[test]
    fn kind_names_round_trip() {
        for kind in SearchKind::ALL {
            assert_eq!(SearchKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(SearchKind::from_name("fast"), None);
    }

    #[test]
    fn basic_matches_legacy_on_bench_positions() {
        let search = Search {
            kind: SearchKind::Basic,
            depth: 6,
        };
        for p in bench_positions(6) {
            let label = &p.label;
            let basic = engage(&p.conflict, search);
            let legacy = Solver::engage(&p.conflict, SolverStrategy::IterativeDeepening(6));

            assert_eq!(basic.outcome, legacy.outcome, "{label}: outcome");
            assert_eq!(
                basic.depth_limited, legacy.depth_limited,
                "{label}: depth limited"
            );
            assert_eq!(
                basic.max_visited_depth, legacy.max_visited_depth,
                "{label}: max visited depth"
            );

            // The whole best line agrees, not only the move played.
            assert_eq!(
                basic.timeline.len(),
                legacy.timeline.len(),
                "{label}: timeline length"
            );
            for (i, (b, l)) in basic.timeline.iter().zip(&legacy.timeline).enumerate() {
                assert_eq!(b.action, l.action, "{label}: event {i} action");
                assert_eq!(b.score.value(), l.score.value(), "{label}: event {i} score");
                assert_eq!(
                    discriminant(&b.score),
                    discriminant(&l.score),
                    "{label}: event {i} score kind"
                );
                assert_eq!(b.bound, l.bound, "{label}: event {i} bound");
            }

            let (Some(b), Some(l)) = (basic.timeline.first(), legacy.timeline.first()) else {
                continue;
            };
            assert_eq!(b.unexamined, l.unexamined, "{label}: unexamined");
            let alternatives = |e: &crate::solver::Event| {
                e.alternatives
                    .iter()
                    .map(|a| (a.action.clone(), a.score.value(), a.bound))
                    .collect::<Vec<_>>()
            };
            assert_eq!(alternatives(b), alternatives(l), "{label}: alternatives");
        }
    }

    #[test]
    fn pv_agrees_with_basic_on_bench_positions() {
        let search = |kind| Search { kind, depth: 6 };
        for p in bench_positions(6) {
            let basic = engage(&p.conflict, search(SearchKind::Basic));
            let pv = engage(&p.conflict, search(SearchKind::Pv));
            assert!(agree(&basic, &pv), "{}: pv disagrees with basic", p.label);
        }
    }

    #[test]
    fn ordered_agrees_with_basic_on_bench_positions() {
        let search = |kind| Search { kind, depth: 6 };
        for p in bench_positions(6) {
            let basic = engage(&p.conflict, search(SearchKind::Basic));
            let ordered = engage(&p.conflict, search(SearchKind::Ordered));
            assert!(
                agree(&basic, &ordered),
                "{}: ordered disagrees with basic",
                p.label
            );
        }
    }

    #[test]
    fn tt_agrees_with_basic_on_bench_positions() {
        let search = |kind| Search { kind, depth: 6 };
        for p in bench_positions(6) {
            let basic = engage(&p.conflict, search(SearchKind::Basic));
            let tt = engage(&p.conflict, search(SearchKind::Tt));
            assert!(agree(&basic, &tt), "{}: tt disagrees with basic", p.label);
        }
    }

    #[test]
    fn mtdf_agrees_with_basic_on_bench_positions() {
        for depth in [6, 1] {
            let search = |kind| Search { kind, depth };
            for p in bench_positions(6) {
                let basic = engage(&p.conflict, search(SearchKind::Basic));
                let mtdf = engage(&p.conflict, search(SearchKind::Mtdf));
                assert!(
                    agree(&basic, &mtdf),
                    "{} at depth {depth}: mtdf disagrees with basic",
                    p.label
                );
            }
        }
    }

    #[test]
    fn mtdf_fallback_agrees_with_basic() {
        let outcome = |conflict: &Conflict, root: RootResult| Outcome {
            outcome: outcome_type(root.scored.score),
            timeline: timeline(conflict, root),
            evaluations: 0,
            cuts: 0,
            search_duration: Duration::ZERO,
            max_visited_depth: 0,
            depth_limited: false,
            search: SearchKind::Basic,
        };
        let mut fell_back = Vec::new();
        for p in bench_positions(6) {
            let mut searcher = Searcher::new(SearchKind::Mtdf);
            let mtdf = mtdf(&mut searcher, &p.conflict, 6, 0.0);
            if searcher.fallbacks == 0 {
                continue;
            }
            let basic = Searcher::new(SearchKind::Basic).root(
                &p.conflict,
                6,
                f32::NEG_INFINITY,
                f32::INFINITY,
            );
            let (basic, mtdf) = (outcome(&p.conflict, basic), outcome(&p.conflict, mtdf));
            assert!(agree(&basic, &mtdf), "{}: fallback disagrees", p.label);
            let turn = p.conflict.next_turn().expect("a search ran");
            fell_back.push(is_maximizing(&p.conflict, &turn));
        }
        // Covers a root with the hero to move and one with a villain to move.
        assert!(fell_back.contains(&true), "no maximizing root fell back");
        assert!(fell_back.contains(&false), "no minimizing root fell back");
    }

    #[test]
    fn mtdf_converges_on_a_known_score() {
        // The setup of the solver's simple fight: the faster hero wins with 5 health.
        let member = |id, health, weapon, speed| PartyMember {
            id,
            health,
            damage_taken: 0.0,
            weapon,
            can_act: true,
            speed,
            morale: Morale::Tactical,
            fled: false,
        };
        let party = |id, member| Party {
            id,
            members: vec![member],
            can_retreat: false,
        };
        let heroes = party(0, member(0, 25.0, Weapon::Fists(Fists { damage: 10.0 }), 6));
        let villains = party(1, member(0, 25.0, Weapon::Stick(Stick { damage: 10.0 }), 3));
        let conflict = Conflict::new(heroes, villains);

        let outcome = engage(
            &conflict,
            Search {
                kind: SearchKind::Mtdf,
                depth: 40,
            },
        );
        assert_eq!(outcome.outcome, OutcomeType::Win(5.0));
    }

    #[test]
    fn every_kind_handles_a_decided_position() {
        let mut conflict =
            scenario::build(Setup::Preset(Preset::Skirmish), Roster::Random, 1).conflict;
        for villain in &mut conflict.opponent.members {
            villain.health = 0.0;
        }
        for kind in SearchKind::ALL {
            let outcome = engage(&conflict, Search { kind, depth: 6 });
            assert!(outcome.timeline.is_empty(), "{}", kind.name());
            assert_eq!(
                outcome.outcome,
                outcome_type(get_utility(&conflict)),
                "{}",
                kind.name()
            );
        }
    }
}
