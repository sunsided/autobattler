//! The alpha-beta recursion.

use super::ordering::order;
use super::table::{position_key, Entry, Table};
use super::SearchKind;
use crate::action::AppliedAction;
use crate::conflict::Conflict;
use crate::solver::{child_state, is_maximizing, moves};
use crate::utility_value::get_utility;
use crate::value::{combine_bound, Bound, TerminalState};

/// The result of searching a position.
#[derive(Debug, Clone)]
pub(crate) struct Scored {
    /// The score, from the initiator's perspective.
    pub score: TerminalState,
    /// Whether [`score`](Scored::score) is exact or a bound due to pruning.
    pub bound: Bound,
    /// The best line from here, one step per action; empty at a leaf and
    /// where a stored score was used.
    pub line: Vec<Step>,
    /// Whether the search below never ran out of depth.
    pub complete: bool,
    /// The fewest actions left at which the search below runs into no depth
    /// limit: one more than the deepest ply it went below.
    pub reach: usize,
}

/// One action of a best line.
#[derive(Debug, Clone)]
pub(crate) struct Step {
    /// The index of the action in the [`moves`] of its position.
    pub index: usize,
    /// The score of the position after the action.
    pub score: TerminalState,
    /// Whether [`score`](Step::score) is exact or a bound due to pruning.
    pub bound: Bound,
}

/// The result of searching the root position.
pub(crate) struct RootResult {
    /// The index of the chosen action in the [`moves`] of the root, if any.
    pub best: Option<usize>,
    /// Every root action tried, in the order tried, with its result.
    pub explored: Vec<(usize, AppliedAction, Scored)>,
    /// The result of the root itself.
    pub scored: Scored,
}

/// Searches positions and counts what it did.
#[derive(Debug)]
pub(crate) struct Searcher {
    /// The number of positions visited.
    pub evaluations: usize,
    /// The number of positions whose remaining actions were pruned.
    pub cuts: usize,
    /// The deepest ply visited.
    pub max_visited_depth: usize,
    /// Whether any position was scored because the depth ran out.
    pub depth_limited: bool,
    /// The previous step's root line, as action indices; its action is tried
    /// first at each ply while the search stays on it.
    pub pv: Vec<usize>,
    /// The kind of search; decides which speedups apply.
    pub kind: SearchKind,
    /// Positions already searched; used by kinds from [`SearchKind::Tt`] up.
    pub table: Table,
    /// The number of null-window searches run by [`mtdf`](super::mtdf::mtdf).
    #[cfg(test)]
    pub passes: usize,
    /// The number of full-window searches [`mtdf`](super::mtdf::mtdf) fell back to.
    #[cfg(test)]
    pub fallbacks: usize,
}

impl Searcher {
    /// A searcher of the given kind, with its counters at zero.
    pub fn new(kind: SearchKind) -> Self {
        Self {
            evaluations: 0,
            cuts: 0,
            max_visited_depth: 0,
            depth_limited: false,
            pv: Vec::new(),
            kind,
            table: Table::new(),
            #[cfg(test)]
            passes: 0,
            #[cfg(test)]
            fallbacks: 0,
        }
    }

    /// Searches `state` up to `depth` actions deep within the window `(alpha, beta)`,
    /// recording every action tried at the root.
    pub fn root(&mut self, state: &Conflict, depth: usize, alpha: f32, beta: f32) -> RootResult {
        let mut explored = Vec::new();
        let scored = self.search(
            state,
            0,
            true,
            depth,
            (alpha, beta),
            |index, action, child| explored.push((index, action.clone(), child.clone())),
        );
        RootResult {
            best: scored.line.first().map(|step| step.index),
            explored,
            scored,
        }
    }

    /// Searches `state`, reached after `ply` actions, until ply `depth`.
    fn node(
        &mut self,
        state: &Conflict,
        ply: usize,
        on_pv: bool,
        depth: usize,
        window: (f32, f32),
    ) -> Scored {
        self.search(state, ply, on_pv, depth, window, |_, _, _| {})
    }

    /// The alpha-beta step shared by the root and every other node; `on_pv`
    /// tells whether the actions so far follow [`pv`](Searcher::pv), and `record`
    /// sees each child as soon as it is searched.
    fn search(
        &mut self,
        state: &Conflict,
        ply: usize,
        on_pv: bool,
        depth: usize,
        (mut alpha, mut beta): (f32, f32),
        mut record: impl FnMut(usize, &AppliedAction, &Scored),
    ) -> Scored {
        self.evaluations += 1;
        self.max_visited_depth = self.max_visited_depth.max(ply);

        // The depth runs out before the end of the fight is noticed, so a
        // decided position at the horizon still counts as depth limited.
        if ply == depth {
            self.depth_limited = true;
            return Scored::horizon(get_utility(state));
        }
        let Some(turn) = state.next_turn() else {
            return Scored::leaf(get_utility(state));
        };
        let actions = moves(state, &turn);
        if actions.is_empty() {
            return Scored::leaf(get_utility(state));
        }

        let key = (self.kind >= SearchKind::Tt).then(|| position_key(state));
        let mut stored_best = None;
        if let Some(entry) = key.and_then(|key| self.table.get(key)) {
            stored_best = entry.best.filter(|&index| index < actions.len());
            // The root always searches, so it can report every action it tried.
            if ply > 0 && Table::usable(entry, depth - ply) {
                if !entry.complete {
                    self.depth_limited = true;
                }
                match entry.bound {
                    Bound::Exact => return Scored::stored(entry),
                    Bound::AtLeast => alpha = alpha.max(entry.score.value()),
                    Bound::AtMost => beta = beta.min(entry.score.value()),
                    Bound::Approximate => {}
                }
                if alpha >= beta {
                    return Scored::stored(entry);
                }
            }
        }

        let is_max = is_maximizing(state, &turn);
        let mut score = TerminalState::Heuristic(if is_max {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        });
        let mut best_bound = Bound::Exact;
        let mut widened = false;
        let mut line = Vec::new();
        let mut complete = true;
        let mut reach = 0;

        let mut cut = false;
        let first = self
            .pv
            .get(ply)
            .copied()
            .filter(|&index| on_pv && index < actions.len());
        let rest = if self.kind >= SearchKind::Ordered {
            order(state, &actions)
        } else {
            (0..actions.len()).collect()
        };
        let stored_best = stored_best.filter(|&index| Some(index) != first);
        let order = first.into_iter().chain(stored_best).chain(
            rest.into_iter()
                .filter(|&index| Some(index) != first && Some(index) != stored_best),
        );
        for index in order {
            let action = &actions[index];
            if stops(is_max, score, alpha, beta) {
                cut = true;
                break;
            }

            let child = self.node(
                &child_state(state, &turn, action),
                ply + 1,
                first == Some(index),
                depth,
                (alpha, beta),
            );
            record(index, action, &child);
            complete &= child.complete;
            reach = reach.max(child.reach + 1);

            let improves = if is_max {
                child.score.value() > score.value()
            } else {
                child.score.value() < score.value()
            };
            widened |= if is_max {
                matches!(child.bound, Bound::AtLeast | Bound::Approximate)
            } else {
                matches!(child.bound, Bound::AtMost | Bound::Approximate)
            };
            let child_value = child.score.value();
            if improves {
                score = child.score;
                best_bound = child.bound;
                line = Vec::with_capacity(child.line.len() + 1);
                line.push(Step {
                    index,
                    score: child.score,
                    bound: child.bound,
                });
                line.extend(child.line);
            }

            // A cut position keeps its window; it stops before the next action.
            if is_max && !reaches_beta(score, beta) {
                alpha = alpha.max(child_value);
            } else if !is_max && !reaches_alpha(score, alpha) {
                beta = beta.min(child_value);
            }
        }
        // The last action can trigger a cut too, with nothing left to prune.
        let cut = cut || stops(is_max, score, alpha, beta);
        if cut {
            self.cuts += 1;
        }

        let bound = combine_bound(is_max, cut, best_bound, widened);
        if let Some(key) = key {
            self.table.put(
                key,
                Entry {
                    depth_left: depth - ply,
                    score,
                    bound,
                    best: line.first().map(|step| step.index),
                    complete,
                    reach,
                },
            );
        }
        Scored {
            score,
            bound,
            line,
            complete,
            reach,
        }
    }
}

impl Scored {
    /// A position where the fight is over, scored exactly.
    fn leaf(score: TerminalState) -> Self {
        Self {
            score,
            bound: Bound::Exact,
            line: Vec::new(),
            complete: true,
            reach: 1,
        }
    }

    /// A position where the depth ran out, scored exactly by its utility.
    fn horizon(score: TerminalState) -> Self {
        Self {
            complete: false,
            reach: 0,
            ..Self::leaf(score)
        }
    }

    /// A position scored by an earlier search of it.
    fn stored(entry: &Entry) -> Self {
        Self {
            score: entry.score,
            bound: entry.bound,
            line: Vec::new(),
            complete: entry.complete,
            reach: entry.reach,
        }
    }
}

/// Whether a node holding `score` prunes its remaining actions: a maximizer
/// at `beta`, a minimizer at `alpha` or as soon as the initiator would lose.
fn stops(is_max: bool, score: TerminalState, alpha: f32, beta: f32) -> bool {
    if is_max {
        reaches_beta(score, beta)
    } else {
        reaches_alpha(score, alpha) || score.is_negative()
    }
}

/// Whether a maximizer's `score` is at least `beta`; the unset start value never is.
fn reaches_beta(score: TerminalState, beta: f32) -> bool {
    score.is_finite() && score.value() >= beta
}

/// Whether a minimizer's `score` is at most `alpha`; the unset start value never is.
fn reaches_alpha(score: TerminalState, alpha: f32) -> bool {
    score.is_finite() && score.value() <= alpha
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::party::Party;
    use crate::party_member::{Morale, PartyMember};
    use crate::weapon::{Fists, Stick, Weapon};

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

    fn full_window(searcher: &mut Searcher, state: &Conflict, depth: usize) -> RootResult {
        searcher.root(state, depth, f32::NEG_INFINITY, f32::INFINITY)
    }

    #[test]
    fn a_win_at_the_horizon_is_scored_but_depth_limited() {
        let mut searcher = Searcher::new(SearchKind::Basic);
        let root = full_window(&mut searcher, &duel(), 5);
        // Scores compare by value only, so check the kind as well.
        assert_eq!(root.scored.score, TerminalState::Win(5.0));
        assert!(matches!(root.scored.score, TerminalState::Win(_)));
        assert_eq!(root.scored.line.len(), 5);
        // The fight ends exactly where the depth runs out, so the search
        // cannot tell it ended and counts as depth limited.
        assert!(searcher.depth_limited);
        assert!(!root.scored.complete);
        assert_eq!(searcher.max_visited_depth, 5);
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

        let mut searcher = Searcher::new(SearchKind::Basic);
        let root = full_window(&mut searcher, &state, 10);
        assert_eq!(root.scored.score, TerminalState::Defeat(-20.0));
        // Three moves were never tried, so the villains might do even better.
        assert_eq!(root.scored.bound, Bound::AtMost);
        assert_eq!(root.explored.len(), 1);
        assert_eq!(root.best, Some(root.explored[0].0));
        assert_eq!(searcher.cuts, 1);
    }

    #[test]
    fn the_last_move_can_cut_too() {
        // Broken courage leaves the hero nothing but fleeing: one move.
        let mut start = duel();
        start.initiator.members[0].morale = Morale::Courage(1.0);
        let turn = start.next_turn().expect("the hero acts");
        assert_eq!(moves(&start, &turn), vec![AppliedAction::Flee]);

        let mut searcher = Searcher::new(SearchKind::Basic);
        let exact = full_window(&mut searcher, &start, 1);
        assert_eq!(exact.scored.bound, Bound::Exact);
        assert_eq!(searcher.cuts, 0);

        // A window the only move reaches: it cuts with nothing left to prune.
        let score = exact.scored.score.value();
        let mut searcher = Searcher::new(SearchKind::Basic);
        let cut = searcher.root(&start, 1, score - 1.0, score);
        assert_eq!(cut.scored.score.value(), score);
        assert_eq!(cut.scored.bound, Bound::AtLeast);
        assert_eq!(cut.explored.len(), 1);
        assert_eq!(searcher.cuts, 1);
    }
}
