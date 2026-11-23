//! The search on game-tree-search: the conflict as a [`Game`], and the search kinds as its
//! searchers.

use super::ordering::order;
use super::table::position_key;
use super::{Search, SearchKind};
use crate::action::AppliedAction;
use crate::conflict::{Conflict, Turn};
use crate::solver::{
    child_state, is_maximizing, outcome_type, Alternative, Event, Outcome, OutcomeType, Solver,
};
use crate::utility_value::get_utility;
use crate::value::{Bound, TerminalState};
use game_tree_search::{Algorithm, Game, Hashed, MapTable, SearchResult, Searcher, Side, Stats};
use std::cell::OnceCell;
use std::cmp::Ordering;
use std::time::{Duration, Instant};

/// The most positions the transposition table keeps, as the old table did.
const TABLE_CAP: usize = 1_000_000;

/// A conflict with its next turn, worked out at most once: the turn order is a sort, a
/// search asks for the side to move and the moves of a position it expands, and positions
/// at the depth limit only need their utility.
#[derive(Debug, Clone)]
pub struct Position {
    /// The conflict.
    pub state: Conflict,
    /// Its next turn, once asked for; `None` inside once the fight is over.
    turn: OnceCell<Option<Turn>>,
}

impl Position {
    /// `state`, its next turn not worked out yet.
    pub fn new(state: Conflict) -> Self {
        Self {
            state,
            turn: OnceCell::new(),
        }
    }

    /// The next turn; `None` once the fight is over.
    fn turn(&self) -> Option<&Turn> {
        self.turn.get_or_init(|| self.state.next_turn()).as_ref()
    }
}

/// A conflict as game-tree-search sees it, with move ordering on or off.
pub struct Combat {
    /// Whether moves are ordered most promising first.
    pub ordered: bool,
}

impl Game for Combat {
    type Position = Position;
    type Move = AppliedAction;
    type Score = TerminalState;

    fn side_to_move(&self, position: &Position) -> Side {
        let turn = position.turn().expect("asked only where a combatant acts");
        if is_maximizing(&position.state, turn) {
            Side::Max
        } else {
            Side::Min
        }
    }

    fn moves(&self, position: &Position, out: &mut Vec<AppliedAction>) {
        if let Some(turn) = position.turn() {
            out.extend(Solver::actions_for(&position.state, turn));
        }
    }

    fn play(&self, position: &Position, action: &AppliedAction) -> Position {
        let turn = position
            .turn()
            .expect("a move is played where a combatant acts");
        Position::new(child_state(&position.state, turn, action))
    }

    fn evaluate(&self, position: &Position) -> TerminalState {
        get_utility(&position.state)
    }

    fn order_moves(&self, position: &Position, actions: &mut [AppliedAction]) {
        if self.ordered {
            order(&position.state, actions);
        }
    }

    /// The villains stop at the first line in which the initiator loses.
    fn satisfices(&self, side: Side, score: TerminalState) -> bool {
        side == Side::Min && score.is_negative()
    }
}

impl Hashed for Combat {
    type Key = (u64, u64);

    fn key(&self, position: &Position) -> (u64, u64) {
        position_key(&position.state)
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
    let root = Position::new(conflict.clone());
    let unlimited = |_: &Stats| false;
    let until_won = |result: &SearchResult<AppliedAction, TerminalState>| {
        !matches!(outcome_type(result.score), OutcomeType::Win(_))
    };
    let result = match search.kind {
        SearchKind::Basic => deepen_without_hint(&game, &root, max_depth),
        SearchKind::Pv | SearchKind::Ordered => {
            Searcher::new().deepen_with(&game, &root, max_depth, unlimited, until_won)
        }
        SearchKind::Tt | SearchKind::Mtdf => {
            let algorithm = if search.kind == SearchKind::Mtdf {
                Algorithm::Mtdf
            } else {
                Algorithm::AlphaBeta
            };
            Searcher::with_table(MapTable::new(TABLE_CAP))
                .with_algorithm(algorithm)
                .deepen_with(&game, &root, max_depth, unlimited, until_won)
        }
    };

    outcome(conflict, &result, (search.kind, start.elapsed()))
}

/// Iterative deepening without trying the previous line first, as the basic kind does: one
/// fresh search per depth, summing the counters.
fn deepen_without_hint(
    game: &Combat,
    root: &Position,
    max_depth: u32,
) -> SearchResult<AppliedAction, TerminalState> {
    let mut searcher = Searcher::new();
    let (mut nodes, mut cuts) = (0, 0);
    let mut depth = 1;
    loop {
        let mut result = searcher.search(game, root, depth);
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
    use super::{Combat, Position};
    use crate::action::AppliedAction;
    use crate::conflict::Conflict;
    use crate::party::Party;
    use crate::party_member::{Morale, PartyMember};
    use crate::solver::{child_state, is_maximizing, moves};
    use crate::value::TerminalState;
    use crate::weapon::{Fists, Stick, Weapon};
    use game_tree_search::{Bound, Searcher};

    fn member(id: usize, health: f32, weapon: Weapon, speed: u32) -> PartyMember {
        PartyMember {
            id,
            health,
            damage_taken: 0.0,
            weapon,
            can_act: true,
            speed,
            morale: Morale::Tactical,
            fled: false,
        }
    }

    fn party(id: usize, members: Vec<PartyMember>, can_retreat: bool) -> Party {
        Party {
            id,
            members,
            can_retreat,
        }
    }

    /// A faster hero (fists, 10) against one villain (stick, 10), both at 25
    /// health: the hero's third punch, the fifth action, wins with 5 health.
    fn duel() -> Conflict {
        Conflict::new(
            party(
                0,
                vec![member(0, 25.0, Weapon::Fists(Fists { damage: 10.0 }), 6)],
                false,
            ),
            party(
                1,
                vec![member(0, 25.0, Weapon::Stick(Stick { damage: 10.0 }), 3)],
                false,
            ),
        )
    }

    const PLAIN: Combat = Combat { ordered: false };

    #[test]
    fn a_win_at_the_horizon_is_scored_but_depth_limited() {
        let result = Searcher::new().search(&PLAIN, &Position::new(duel()), 5);
        // Scores compare by value only, so check the kind as well.
        assert_eq!(result.score, TerminalState::Win(5.0));
        assert!(matches!(result.score, TerminalState::Win(_)));
        assert_eq!(result.pv.len(), 5);
        // The fight ends exactly where the depth runs out, so the search
        // cannot tell it ended and counts as depth limited.
        assert!(!result.proven);
        assert_eq!(result.stats.max_ply, 5);
    }

    #[test]
    fn a_minimizing_root_stops_at_the_first_defeat() {
        // The hero (20 health) moves first; Molphige's 20-damage punch then
        // ends the fight, so the villains' turn is the root here.
        let start = Conflict::new(
            party(
                0,
                vec![member(0, 20.0, Weapon::Fists(Fists { damage: 10.0 }), 6)],
                false,
            ),
            party(
                1,
                vec![
                    member(0, 15.0, Weapon::Stick(Stick { damage: 5.0 }), 3),
                    member(1, 15.0, Weapon::Fists(Fists { damage: 20.0 }), 5),
                ],
                true,
            ),
        );
        let turn = start.next_turn().expect("the hero acts");
        let state = child_state(&start, &turn, &moves(&start, &turn)[0]);
        let turn = state.next_turn().expect("Molphige acts");
        assert!(!is_maximizing(&state, &turn));
        assert_eq!(moves(&state, &turn).len(), 4);

        let result = Searcher::new().search(&PLAIN, &Position::new(state), 10);
        assert_eq!(result.score, TerminalState::Defeat(-20.0));
        // Three moves were never tried, so the villains might do even better.
        assert_eq!(result.bound, Bound::Upper);
        assert_eq!((result.root_moves.len(), result.unexamined), (1, 3));
        assert_eq!(result.best(), Some(&result.root_moves[0].mv));
        assert_eq!(result.stats.cuts, 1);
    }

    #[test]
    fn a_forced_move_is_exact() {
        // Broken courage leaves the hero nothing but fleeing: one move.
        let mut start = duel();
        start.initiator.members[0].morale = Morale::Courage(1.0);
        let turn = start.next_turn().expect("the hero acts");
        assert_eq!(moves(&start, &turn), vec![AppliedAction::Flee]);

        // Every move was searched, so nothing was cut and the score is exact.
        let result = Searcher::new().search(&PLAIN, &Position::new(start), 1);
        assert_eq!(
            (result.bound, result.stats.cuts, result.root_moves.len()),
            (Bound::Exact, 0, 1)
        );
    }
}
