//! A recursive alpha-beta search, in kinds that add speedups on top of each other.

mod agreement;
pub mod bench;
mod engine;
mod ordering;
mod positions;
#[cfg(test)]
mod properties;
mod table;

use crate::conflict::Conflict;
use crate::solver::{outcome_type, Outcome};
use crate::utility_value::get_utility;
use std::time::Duration;

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
/// Deepens one action at a time up to `search.depth` (at least 1) with game-tree-search,
/// stopping early once a win is found or the fight ends within the search. The evaluations,
/// cuts and duration are summed over all steps; the deepest ply is the deepest any step
/// reached.
///
/// Each kind adds its speedup on top of the ones before: the previous step's best line
/// tried first, move ordering, a transposition table that lives for the whole call, and
/// MTD(f), which starts each step from the previous step's score.
pub fn engage(conflict: &Conflict, search: Search) -> Outcome {
    engine::engage(conflict, search)
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

#[cfg(test)]
mod tests {
    use super::agreement::agree;
    use super::positions::bench_positions;
    use super::*;
    use crate::party::Party;
    use crate::party_member::{Morale, PartyMember};
    use crate::roster::Roster;
    use crate::scenario::{self, Preset, Setup};
    use crate::solver::{outcome_type, OutcomeType};
    use crate::utility_value::get_utility;
    use crate::weapon::{Fists, Stick, Weapon};

    #[test]
    fn kind_names_round_trip() {
        for kind in SearchKind::ALL {
            assert_eq!(SearchKind::from_name(kind.name()), Some(kind));
        }
        assert_eq!(SearchKind::from_name("fast"), None);
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
