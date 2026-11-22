//! The search on game-tree-search: the conflict as a [`Game`], and the search kinds as its
//! searchers.

use super::ordering::order_in_place;
use super::table::position_key;
use super::{Search, SearchKind};
use crate::action::AppliedAction;
use crate::conflict::Conflict;
use crate::solver::{
    child_state, is_maximizing, moves, outcome_type, Alternative, Event, Outcome, OutcomeType,
};
use crate::utility_value::get_utility;
use crate::value::{Bound, TerminalState};
use game_tree_search::{Algorithm, Game, Hashed, MapTable, SearchResult, Searcher, Side, Stats};
use std::cmp::Ordering;
use std::time::{Duration, Instant};

/// The most positions the transposition table keeps, as the old table did.
const TABLE_CAP: usize = 1_000_000;

/// A conflict as game-tree-search sees it, with move ordering on or off.
pub struct Combat {
    /// Whether moves are ordered most promising first.
    pub ordered: bool,
}

impl Game for Combat {
    type Position = Conflict;
    type Move = AppliedAction;
    type Score = TerminalState;

    fn side_to_move(&self, state: &Conflict) -> Side {
        let turn = state
            .next_turn()
            .expect("asked only where a combatant acts");
        if is_maximizing(state, &turn) {
            Side::Max
        } else {
            Side::Min
        }
    }

    fn moves(&self, state: &Conflict, out: &mut Vec<AppliedAction>) {
        if let Some(turn) = state.next_turn() {
            out.extend(moves(state, &turn));
        }
    }

    fn play(&self, state: &Conflict, action: &AppliedAction) -> Conflict {
        let turn = state
            .next_turn()
            .expect("a move is played where a combatant acts");
        child_state(state, &turn, action)
    }

    fn evaluate(&self, state: &Conflict) -> TerminalState {
        get_utility(state)
    }

    fn order_moves(&self, state: &Conflict, actions: &mut [AppliedAction]) {
        if self.ordered {
            order_in_place(state, actions);
        }
    }

    /// The villains stop at the first line in which the initiator loses.
    fn satisfices(&self, side: Side, score: TerminalState) -> bool {
        side == Side::Min && score.is_negative()
    }
}

impl Hashed for Combat {
    type Key = (u64, u64);

    fn key(&self, state: &Conflict) -> (u64, u64) {
        position_key(state)
    }
}

/// Predicts the best line of play from `conflict` with game-tree-search, as
/// [`super::engage`] does.
pub fn engage(conflict: &Conflict, search: Search) -> Outcome {
    let max_depth = u32::try_from(search.depth.max(1)).expect("a depth fits in u32");
    let game = Combat {
        ordered: search.kind >= SearchKind::Ordered,
    };
    let start = Instant::now();
    let unlimited = |_: &Stats| false;
    let until_won = |result: &SearchResult<AppliedAction, TerminalState>| {
        !matches!(outcome_type(result.score), OutcomeType::Win(_))
    };
    let result = match search.kind {
        SearchKind::Basic => deepen_without_hint(&game, conflict, max_depth),
        SearchKind::Pv | SearchKind::Ordered => {
            Searcher::new().deepen_with(&game, conflict, max_depth, unlimited, until_won)
        }
        SearchKind::Tt | SearchKind::Mtdf => {
            let algorithm = if search.kind == SearchKind::Mtdf {
                Algorithm::Mtdf
            } else {
                Algorithm::AlphaBeta
            };
            Searcher::with_table(MapTable::new(TABLE_CAP))
                .with_algorithm(algorithm)
                .deepen_with(&game, conflict, max_depth, unlimited, until_won)
        }
    };

    outcome(conflict, &result, (search.kind, start.elapsed()))
}

/// Iterative deepening without trying the previous line first, as the basic kind does: one
/// fresh search per depth, summing the counters.
fn deepen_without_hint(
    game: &Combat,
    conflict: &Conflict,
    max_depth: u32,
) -> SearchResult<AppliedAction, TerminalState> {
    let mut searcher = Searcher::new();
    let (mut nodes, mut cuts) = (0, 0);
    let mut depth = 1;
    loop {
        let mut result = searcher.search(game, conflict, depth);
        nodes += result.stats.nodes;
        cuts += result.stats.cuts;
        let won = matches!(outcome_type(result.score), OutcomeType::Win(_));
        if won || result.proven || depth >= max_depth {
            result.stats.nodes = nodes;
            result.stats.cuts = cuts;
            return result;
        }

        depth += 1;
    }
}

/// The outcome of `result`, a search of `conflict` by `kind` that took `elapsed`.
fn outcome(
    conflict: &Conflict,
    result: &SearchResult<AppliedAction, TerminalState>,
    (kind, elapsed): (SearchKind, Duration),
) -> Outcome {
    Outcome {
        outcome: outcome_type(result.score),
        timeline: timeline(conflict, result),
        evaluations: usize::try_from(result.stats.nodes).expect("counts fit in usize"),
        cuts: usize::try_from(result.stats.cuts).expect("counts fit in usize"),
        search_duration: elapsed,
        max_visited_depth: usize::try_from(result.stats.max_ply).expect("a ply fits in usize"),
        depth_limited: !result.proven,
        search: kind,
    }
}

/// Replays the principal variation of `result` from `conflict` as events; the first lists
/// the other root moves tried, best first for the mover.
fn timeline(
    conflict: &Conflict,
    result: &SearchResult<AppliedAction, TerminalState>,
) -> Vec<Event> {
    let mut alternatives = alternatives(conflict, result);
    let mut unexamined = result.unexamined;
    let mut events = Vec::with_capacity(result.pv.len());
    let mut state = conflict.clone();
    for step in &result.pv {
        let turn = state
            .next_turn()
            .expect("a best line only continues while the fight does");
        state = child_state(&state, &turn, &step.mv);
        events.push(Event {
            round: turn.round,
            slot: turn.slot,
            actor: turn.actor,
            action: step.mv.clone(),
            state: state.clone(),
            score: step.score,
            bound: bound(step.bound),
            alternatives: std::mem::take(&mut alternatives),
            unexamined: std::mem::take(&mut unexamined),
        });
    }

    events
}

/// The root moves other than the best, best first for the side to move; stable, so ties
/// keep the order they were tried in.
fn alternatives(
    conflict: &Conflict,
    result: &SearchResult<AppliedAction, TerminalState>,
) -> Vec<Alternative> {
    let Some(turn) = conflict.next_turn() else {
        return Vec::new();
    };

    let best = result.best();
    let mut skipped_best = false;
    let mut alternatives: Vec<Alternative> = result
        .root_moves
        .iter()
        .filter(|root| {
            let is_best = !skipped_best && Some(&root.mv) == best;
            skipped_best |= is_best;
            !is_best
        })
        .map(|root| Alternative {
            action: root.mv.clone(),
            score: root.score,
            bound: bound(root.bound),
        })
        .collect();
    let maximizing = is_maximizing(conflict, &turn);
    alternatives.sort_by(|a, b| {
        let ordering = a.score.partial_cmp(&b.score).unwrap_or(Ordering::Equal);
        if maximizing {
            ordering.reverse()
        } else {
            ordering
        }
    });

    alternatives
}

/// The autobattler bound of a game-tree-search bound.
fn bound(bound: game_tree_search::Bound) -> Bound {
    match bound {
        game_tree_search::Bound::Exact => Bound::Exact,
        game_tree_search::Bound::Lower => Bound::AtLeast,
        game_tree_search::Bound::Upper => Bound::AtMost,
        _ => Bound::Approximate,
    }
}

#[cfg(test)]
mod tests {
    use super::super::positions::bench_positions;
    use super::super::{engage as old_engage, Search, SearchKind};
    use super::engage;
    use std::mem::discriminant;

    /// The engine on game-tree-search plays like the old one: the same outcome, first move
    /// and score for every kind and depth on the bench positions.
    #[test]
    fn plays_like_the_old_engine() {
        for depth in [1, 4, 6] {
            for p in bench_positions(6) {
                for kind in SearchKind::ALL {
                    let search = Search { kind, depth };
                    let (old, new) = (old_engage(&p.conflict, search), engage(&p.conflict, search));
                    let label = format!("{} @ depth {depth}, {}", p.label, kind.name());
                    assert_eq!(
                        discriminant(&old.outcome),
                        discriminant(&new.outcome),
                        "{label}"
                    );
                    let (old, new) = (old.timeline.first(), new.timeline.first());
                    assert_eq!(old.map(|e| &e.action), new.map(|e| &e.action), "{label}");
                    assert_eq!(old.map(|e| e.score), new.map(|e| e.score), "{label}");
                }
            }
        }
    }
}
