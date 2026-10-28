use crate::action::{AppliedAction, TargetedAction};
use crate::action_iterator::ActionIterator;
use crate::conflict::{Conflict, Turn};
use crate::party::{Participant, Party};
use crate::utility_value::get_utility;
use crate::value::{combine_bound, Bound, Cutoff, TerminalState, Value};
use log::trace;
use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::time::{Duration, Instant};

pub struct Solver;

/// The most rounds [`Solver::play`] plays before calling the fight undecided.
pub const MAX_ROUNDS: usize = 50;

/// Classifies a value from the initiator's perspective.
fn outcome_type(value: TerminalState) -> OutcomeType {
    match value {
        TerminalState::Win(score) => OutcomeType::Win(score),
        TerminalState::Defeat(score) => OutcomeType::Lose(score),
        TerminalState::Remain(score) => OutcomeType::Remain(score),
        TerminalState::Retreat(score) => OutcomeType::Retreat(score),
        TerminalState::Heuristic(score) => OutcomeType::Unknown(score),
        TerminalState::OpenUnexplored(score) => OutcomeType::Unknown(score),
    }
}

/// The strategy to use with the solver.
#[derive(Debug, Copy, Clone)]
#[allow(dead_code)]
pub enum SolverStrategy {
    /// Use a simple depth-limited search.
    DepthLimited(usize),
    /// Use iterative deepening.
    IterativeDeepening(usize),
}

impl Solver {
    /// Predicts the sequence of optimal moves to resolve the conflict,
    /// in favor of the initiating party.
    ///
    /// ## Arguments
    /// * `conflict` - The conflict situation to resolve.
    /// * `max_depth` - The maximum search depth.
    ///
    /// ## Returns
    /// The [`Outcome`] of the conflict.
    pub fn engage(conflict: &Conflict, strategy: SolverStrategy) -> Outcome {
        match strategy {
            SolverStrategy::DepthLimited(max_depth) => {
                let max_depth = max_depth.max(1);
                Self::minimax(conflict, max_depth)
            }
            SolverStrategy::IterativeDeepening(max_depth) => {
                let max_depth = max_depth.max(1);
                let mut depth = 1;
                loop {
                    log_increase_search_depth_to(depth, max_depth);
                    let outcome = Self::minimax(conflict, depth);
                    let should_stop = match outcome.outcome {
                        OutcomeType::Win(_) => true,
                        OutcomeType::Lose(_) => false,
                        OutcomeType::Remain(_) => false, // we may want to accept retreats too
                        OutcomeType::Retreat(_) => false,
                        OutcomeType::Unknown(_) => false,
                    };

                    if !outcome.depth_limited || should_stop {
                        log_finish_iddfs(depth);
                        return outcome;
                    }

                    depth += 1;
                    if depth > max_depth {
                        log_stop_iddfs(max_depth);
                        return outcome;
                    }
                }
            }
        }
    }

    /// Plays the conflict to its end, searching anew before every action.
    ///
    /// Each search looks ahead as far as `strategy` allows; only its first move
    /// is played. Stops early, with an unknown outcome, once `max_rounds` rounds
    /// have been played. Statistics are summed over all searches; the depth is
    /// the deepest any search went.
    pub fn play(conflict: &Conflict, strategy: SolverStrategy, max_rounds: usize) -> Outcome {
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
            let outcome = Self::engage(&state, strategy);
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
        }
    }

    /// Uses the minimax algorithm to find the optimal outcome.
    ///
    /// ## Arguments
    /// * `conflict` - The conflict situation to resolve.
    /// * `max_depth` - The maximum search depth in the tree. Can be used to limit search complexity.
    ///
    /// ## Returns
    /// The [`Outcome`] of the conflict.
    fn minimax(conflict: &Conflict, max_depth: usize) -> Outcome {
        // The root value is initialized according to who acts first.
        let mut nodes = vec![Node::new_root(conflict.clone(), 0)];

        let mut depth_limited = false;

        // Track expansion statistics.
        let mut evaluations = 0;
        let mut pruning_cuts = 0;
        let mut max_visited_depth = 0;
        let start_time = Instant::now();

        let mut dfs_queue = Vec::from([0]);
        'dfs: while let Some(id) = dfs_queue.pop() {
            evaluations += 1;

            // The clone here is a hack to get around borrowing rules.
            let mut node = nodes[id].clone();
            log_exploring_node(&node);

            // Track the deepest depths.
            max_visited_depth = max_visited_depth.max(node.depth);

            // Terminate iteration if the look-ahead depth is reached.
            // Note that we can only know if a state is terminal once we have
            // fully expanded it. This information is available further below,
            // after the node expansion step.
            //
            // A cutoff leaves the remaining children unexplored, so the value
            // is at best a bound: a maximizer could still find more, a minimizer less.
            let continue_expansion = if node.is_maximizing && node.value.is_beta_cutoff() {
                log_beta_cutoff(&node);
                pruning_cuts += 1;
                node.bound = node.finalized_bound(true);
                nodes[node.id].bound = node.bound;
                false
            } else if !node.is_maximizing && node.value.is_alpha_cutoff() {
                log_alpha_cutoff(&node);
                pruning_cuts += 1;
                node.bound = node.finalized_bound(true);
                nodes[node.id].bound = node.bound;
                false
            } else if !node.is_maximizing && node.value.is_negative() {
                log_minimizer_detected_defeat(&node);
                pruning_cuts += 1;
                node.bound = node.finalized_bound(true);
                nodes[node.id].bound = node.bound;
                false
            } else if node.depth == max_depth {
                *node.value = get_utility(&node.state);
                nodes[node.id].value = node.value.clone();
                depth_limited = true;
                log_max_search_depth_reached(&node);
                false
            } else {
                true
            };

            if !continue_expansion {
                Self::propagate_to_parent(&mut nodes, &node);
                continue 'dfs;
            }

            // Expand the search tree at the current node.
            let expansion_result = Self::minimax_expand(node, nodes.len());

            // Handle expansion or exhaustion of the node.
            let node = match expansion_result {
                ExpansionResult::Exhausted(Exhaustion { mut node }) => {
                    let value = if (*node.value).is_finite() {
                        log_node_fully_explored(&node, &nodes);
                        node.bound = node.finalized_bound(false);
                        *node.value
                    } else {
                        // If this is a terminal node we either have a winner or loser.
                        let value = get_utility(&node.state);
                        log_node_terminal_state(&node, &value, &nodes);
                        value
                    };

                    // Update the value in the nodes set first before iterating.
                    node.value.value = value;
                    nodes[id].value = node.value.clone();

                    // Update the value in the nodes set first before iterating.
                    Self::propagate_to_parent(&mut nodes, &node);
                    node
                }
                ExpansionResult::Expanded(Expansion { parent, child }) => {
                    // Since the actions were not exhausted yet, we push the parent first
                    // so that we can continue from it later.
                    dfs_queue.push(parent.id);
                    dfs_queue.push(child.id);

                    nodes.push(child);
                    parent
                }
            };

            // Replace the node in the original array with our clone.
            // This is necessary because the expansion updated the iterator within the node.
            let node_id = node.id;
            nodes[node_id] = node;
        }

        let search_duration = Instant::now() - start_time;
        Self::backtrack(
            nodes,
            evaluations,
            pruning_cuts,
            max_visited_depth,
            search_duration,
            depth_limited,
        )
    }

    /// Implements the minimax recursion as an expansion of the search tree.
    ///
    /// ## Arguments
    /// * `node` - The search node we are expanding. We take ownership in order to avoid multiple borrows.
    /// * `next_child_id` - The next available child ID, typically the current length of the list
    ///   of all known and expanded nodes.
    ///
    /// ## Returns
    /// The same node that was passed in.
    fn minimax_expand(mut node: Node, next_child_id: usize) -> ExpansionResult {
        debug_assert!(next_child_id > node.id);

        // If nobody is left to act (a party was defeated, or the round in which
        // a party retreated is over), this is a terminal state.
        let Some(turn) = node.state.next_turn() else {
            return ExpansionResult::new_exhaustion(node);
        };

        // If we visit this node for the first time, create the iterator.
        // On all subsequent visits we continue from the last-known state.
        if node.action_iter.is_none() {
            node.action_iter = Some(Self::actions_for(&node.state, &turn));
        }

        // Select the party of the acting combatant and the opponent.
        let actor_is_initiator = turn.actor.party_id == node.state.initiator.id;
        debug_assert_eq!(actor_is_initiator, node.is_maximizing);
        let (current, opponent) = Self::parties_of(&node.state, &turn);

        let source_party_id = current.id;

        if let Some(action) = node.action_iter.as_mut().and_then(|i| i.next()) {
            match action {
                AppliedAction::Flee => {
                    let mut current = current.clone();
                    let _ = current.retreat();

                    // Create a new branch on the board.
                    let mut state = Self::rebuild_state(
                        &node.state,
                        actor_is_initiator,
                        current,
                        opponent.clone(),
                    );
                    state.finish_turn(&turn);

                    let child_node =
                        Node::new_branch_from(next_child_id, &node, turn, action, state);

                    if let Some(action) = &child_node.action {
                        log_expand_node_with_action(&node, &child_node, action);
                    }

                    return ExpansionResult::new_expansion(node, child_node);
                }
                AppliedAction::Targeted(action) => {
                    let source_id = action.source.member_id;
                    debug_assert_eq!(action.source.party_id, source_party_id);
                    debug_assert_eq!(action.source, turn.actor);

                    let target_party_id = action.target.party_id;
                    debug_assert_eq!(action.target.party_id, opponent.id);

                    let target_id = action.target.member_id;
                    let target = &opponent.members[action.target.member_id];

                    let action = &action.action;

                    // TODO: Optimize state creation - only clone when action was applied.
                    let mut target = target.clone();
                    if target.handle_action(action) {
                        // Branch off and replace the member with the updated state.
                        let mut opponent = opponent.clone();
                        opponent.replace_member(target);

                        // Create a new branch on the board.
                        let mut state = Self::rebuild_state(
                            &node.state,
                            actor_is_initiator,
                            current.clone(),
                            opponent,
                        );
                        state.finish_turn(&turn);

                        // The action to apply.
                        let action = AppliedAction::Targeted(TargetedAction {
                            action: action.clone(),
                            source: Participant {
                                party_id: source_party_id,
                                member_id: source_id,
                            },
                            target: Participant {
                                party_id: target_party_id,
                                member_id: target_id,
                            },
                        });

                        let child_node =
                            Node::new_branch_from(next_child_id, &node, turn, action, state);

                        if let Some(action) = &child_node.action {
                            log_expand_node_with_action(&node, &child_node, action);
                        }

                        return ExpansionResult::new_expansion(node, child_node);
                    }
                }
            }
        }

        ExpansionResult::new_exhaustion(node)
    }

    /// Splits the parties of `state` into the one of the acting combatant and its opponent.
    fn parties_of<'a>(state: &'a Conflict, turn: &Turn) -> (&'a Party, &'a Party) {
        if turn.actor.party_id == state.initiator.id {
            (&state.initiator, &state.opponent)
        } else {
            (&state.opponent, &state.initiator)
        }
    }

    /// Creates the iterator over all actions of the combatant acting in `turn`.
    ///
    /// Only the acting combatant is enumerated; member IDs equal their indices.
    fn actions_for(state: &Conflict, turn: &Turn) -> ActionIterator {
        let (current, opponent) = Self::parties_of(state, turn);
        let member_id = turn.actor.member_id;
        ActionIterator::new_in(current.clone(), opponent.clone(), member_id..member_id + 1)
    }

    /// Counts the actions available to the combatant acting in `turn`.
    fn legal_moves(state: &Conflict, turn: &Turn) -> usize {
        Self::actions_for(state, turn).count()
    }

    /// Builds the child state from the (possibly updated) acting and opposing parties,
    /// keeping the round and slot of the parent state.
    ///
    /// ## Arguments
    /// * `parent` - The state the action was applied to.
    /// * `actor_is_initiator` - Whether the `current` party is the initiator.
    /// * `current` - The party of the acting combatant.
    /// * `opponent` - The other party.
    fn rebuild_state(
        parent: &Conflict,
        actor_is_initiator: bool,
        current: Party,
        opponent: Party,
    ) -> Conflict {
        let (initiator, opponent) = if actor_is_initiator {
            (current, opponent)
        } else {
            (opponent, current)
        };
        Conflict {
            initiator,
            opponent,
            round: parent.round,
            slot: parent.slot,
            parting: parent.parting,
        }
    }

    /// Backtracks the events from the start to one of the the most likely outcomes.
    fn backtrack(
        nodes: Vec<Node>,
        evaluations: usize,
        pruning_cuts: usize,
        max_visited_depth: usize,
        search_duration: Duration,
        depth_limited: bool,
    ) -> Outcome {
        // The outcome is positive only if the value of the start
        // node is positive and under the assumption that the opposing
        // player attempts to play optimally.
        let outcome = outcome_type(nodes[0].value.value);

        // Follow the principal variation. If the root has no best child,
        // the conflict was already decided and the timeline stays empty.
        let mut stack = Vec::default();
        let mut next = nodes[0].best_child;
        while let Some(id) = next {
            let node = &nodes[id];
            let turn = node.turn.as_ref().expect("Only the root node has no turn");
            let parent_id = node.parent_id.expect("Only the root node has no parent");

            // All other explored moves of the same actor, best first from its perspective.
            let mut alternatives: Vec<Alternative> = nodes
                .iter()
                .filter(|n| n.parent_id == Some(parent_id) && n.id != node.id)
                .map(|n| Alternative {
                    action: n.action.clone().expect("Only the root node has no action"),
                    score: *n.value,
                    bound: n.bound,
                })
                .collect();
            let actor_is_initiator = turn.actor.party_id == nodes[0].state.initiator.id;
            alternatives.sort_by(|a, b| {
                let ordering = a.score.partial_cmp(&b.score).unwrap_or(Ordering::Equal);
                if actor_is_initiator {
                    ordering.reverse()
                } else {
                    ordering
                }
            });
            let unexamined =
                Self::legal_moves(&nodes[parent_id].state, turn) - (alternatives.len() + 1);

            stack.push(Event {
                round: turn.round,
                slot: turn.slot,
                actor: turn.actor.clone(),
                action: node
                    .action
                    .clone()
                    .expect("Only the root node has no action"),
                state: node.state.clone(),
                score: *node.value,
                bound: node.bound,
                alternatives,
                unexamined,
            });
            next = node.best_child;
        }

        Outcome {
            outcome,
            timeline: stack,
            evaluations,
            cuts: pruning_cuts,
            max_visited_depth,
            search_duration,
            depth_limited,
        }
    }

    /// Propagates known terminal utility values upwards in the
    /// search tree.
    fn propagate_to_parent(nodes: &mut [Node], child_node: &Node) -> Cutoff {
        let parent_id = child_node.parent_id;

        if let Some(id) = parent_id {
            // Note that the child value is only finite if we reached
            // a terminal state and will be ±infinite if the search
            // terminated due to search depth limitation.
            let child_value = child_node.value.clone();

            let parent_node = &mut nodes[id];
            if parent_node.is_maximizing {
                if child_value.value > *parent_node.value {
                    *parent_node.value = child_value.value;
                    parent_node.best_child = Some(child_node.id);
                    parent_node.best_bound = child_node.bound;
                }
                parent_node.widened |=
                    matches!(child_node.bound, Bound::AtLeast | Bound::Approximate);

                if parent_node.value.is_beta_cutoff() {
                    // beta-cutoff
                    return Cutoff::Beta;
                } else {
                    parent_node.value.alpha =
                        parent_node.value.alpha.max(child_value.value.value());
                }
            } else {
                if child_value.value < *parent_node.value {
                    *parent_node.value = child_value.value;
                    parent_node.best_child = Some(child_node.id);
                    parent_node.best_bound = child_node.bound;
                }
                parent_node.widened |=
                    matches!(child_node.bound, Bound::AtMost | Bound::Approximate);

                if parent_node.value.is_alpha_cutoff() {
                    // alpha-cutoff
                    return Cutoff::Alpha;
                } else {
                    parent_node.value.beta = parent_node.value.beta.min(child_value.value.value());
                }
            }
        }

        Cutoff::None
    }
}

/// An outcome of a conflict.
pub struct Outcome {
    /// Whether the initiating party wins the conflict.
    pub outcome: OutcomeType,
    /// An optimal path of actions leading to the outcome.
    pub timeline: Vec<Event>,
    /// The number of node evaluations performed.
    pub evaluations: usize,
    /// The number of pruning steps performed.
    pub cuts: usize,
    /// The search duration.
    pub search_duration: Duration,
    /// The depth of the deepest node evaluated.
    pub max_visited_depth: usize,
    /// `true` if the search was depth limited and has more nodes to explore.
    pub depth_limited: bool,
}

impl Outcome {
    /// The number of events in the timeline.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.timeline.len()
    }
}

/// The type of outcome.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum OutcomeType {
    /// The initiating party wins.
    Win(f32),
    /// The initiating party loses.
    Lose(f32),
    /// The initiating party remained after the opponent retreated.
    Remain(f32),
    /// The initiating party retreated.
    Retreat(f32),
    /// Unknown outcome.
    Unknown(f32),
}

/// An event in the timeline.
pub struct Event {
    /// The round in which the event took place, starting at 1.
    pub round: usize,
    /// The slot in the initiative order of the acting combatant.
    pub slot: usize,
    /// The acting combatant.
    pub actor: Participant,
    /// The action that was applied.
    pub action: AppliedAction,
    /// The state of the conflict after the action took place.
    pub state: Conflict,
    /// The search score of the action, from the initiator's perspective.
    pub score: TerminalState,
    /// Whether [`score`](Event::score) is exact or a bound due to pruning.
    pub bound: Bound,
    /// The other explored actions of the same combatant, best first from its perspective.
    pub alternatives: Vec<Alternative>,
    /// The number of legal actions that were never generated due to pruning.
    pub unexamined: usize,
}

/// An action the acting combatant could have taken instead.
#[derive(Debug, Clone)]
pub struct Alternative {
    /// The action.
    pub action: AppliedAction,
    /// The search score of the action, from the initiator's perspective.
    pub score: TerminalState,
    /// Whether [`score`](Alternative::score) is exact or a bound due to pruning.
    pub bound: Bound,
}

/// A tree expansion outcome.
///
/// Only ever lives as a short return value, so boxing the larger variant
/// would just add a heap allocation per expansion.
#[allow(clippy::large_enum_variant)]
enum ExpansionResult {
    /// A new node was added.
    Expanded(Expansion),
    /// No new node was added.
    Exhausted(Exhaustion),
}

struct Expansion {
    /// The original (now parent) node.
    parent: Node,
    /// The expanded child node.
    child: Node,
}

struct Exhaustion {
    /// The original node.
    node: Node,
}

impl ExpansionResult {
    /// Registers the `child` node as a new expansion to be explored.
    ///
    /// ## Arguments
    /// * `node` - The original (parent) node.
    /// * `child` - The new child node to explore.
    pub const fn new_expansion(node: Node, child: Node) -> Self {
        Self::Expanded(Expansion {
            parent: node,
            child,
        })
    }

    /// Registers the `node` as fully explored.
    ///
    /// ## Arguments
    /// * `node` - The original node.
    pub const fn new_exhaustion(node: Node) -> Self {
        Self::Exhausted(Exhaustion { node })
    }
}

/// A node in the game tree.
#[derive(Debug, Clone)]
struct Node {
    /// The ID of the node; corresponds to the index of the node
    /// in the vector of explored nodes.
    pub id: usize,
    /// The ID of the node's immediate parent.
    /// Is [`None`] only for the root node.
    pub parent_id: Option<usize>,
    /// The turn whose action produced this node.
    /// Is [`None`] only for the root node.
    pub turn: Option<Turn>,
    /// The depth of the node. If it reaches zero, search is terminated.
    pub depth: usize,
    /// The action iterator; if it yields none, the node is explored completely.
    pub action_iter: Option<ActionIterator>,
    /// Whether this is a maximizing or minimizing node in minimax.
    /// If maximizing, the next actor in [`state`](Node::state) belongs to the
    /// initiating party of the conflict.
    pub is_maximizing: bool,
    /// The utility value of this node. Only meaningful if this is
    /// a terminal node (i.e. win or loss for either side).
    pub value: Value,
    /// Whether [`value`](Node::value) is exact or only a bound because pruning
    /// cut this node or a node in its best line short. Set when the node is finalized.
    pub bound: Bound,
    /// The bound of [`best_child`](Node::best_child).
    pub best_bound: Bound,
    /// Whether any explored child may be better for this node than its value says.
    pub widened: bool,
    /// The ID of the child that optimized the value, if any. Could be [`None`]
    /// if this node itself is a terminal node.
    pub best_child: Option<usize>,
    /// The action taken to arrive at this node.
    /// Is [`None`] only for the root node.
    pub action: Option<AppliedAction>,
    /// The state after applying the action.
    pub state: Conflict,
}

impl Node {
    /// Creates a new root node.
    pub fn new_root(conflict: Conflict, max_depth: usize) -> Self {
        // The root may be a turn of either party, depending on initiative.
        // If nobody can act, the node is terminal and the flag is irrelevant.
        let is_maximizing = conflict
            .next_turn()
            .is_none_or(|turn| turn.actor.party_id == conflict.initiator.id);
        Self {
            id: 0,
            parent_id: None,
            depth: max_depth,
            turn: None,
            is_maximizing,
            value: Value::new(if is_maximizing {
                TerminalState::Heuristic(f32::NEG_INFINITY)
            } else {
                TerminalState::Heuristic(f32::INFINITY)
            }),
            bound: Bound::Exact,
            best_bound: Bound::Exact,
            widened: false,
            best_child: None,
            action: None,
            state: conflict,
            action_iter: None,
        }
    }

    /// Creates a child node after the combatant of `turn` acted.
    ///
    /// Whether the child is maximizing or minimizing depends on who acts next
    /// in the child `state`, so consecutive turns of the same party produce
    /// consecutive layers of the same kind.
    ///
    /// ## Arguments
    /// * `id` - The new ID for the node to be created.
    /// * `parent` - The parent node.
    /// * `turn` - The turn whose action lead to the child node.
    /// * `action` - The action that lead to the expansion into the child node.
    /// * `state` - The new state observed by the child node after applying the action.
    pub fn new_branch_from(
        id: usize,
        parent: &Node,
        turn: Turn,
        action: AppliedAction,
        state: Conflict,
    ) -> Self {
        // If nobody can act, the node is terminal and the flag is irrelevant.
        let is_maximizing = match state.next_turn() {
            Some(next) => next.actor.party_id == state.initiator.id,
            None => !parent.is_maximizing,
        };
        Self {
            id,
            parent_id: Some(parent.id),
            is_maximizing,
            value: if is_maximizing {
                parent
                    .value
                    .with_value(TerminalState::Heuristic(f32::NEG_INFINITY))
            } else {
                parent
                    .value
                    .with_value(TerminalState::Heuristic(f32::INFINITY))
            },
            bound: Bound::Exact,
            best_bound: Bound::Exact,
            widened: false,
            best_child: None,
            depth: parent.depth + 1,
            turn: Some(turn),
            action: Some(action),
            action_iter: None,
            state,
        }
    }

    /// Derives the bound of this node's value from its explored children.
    ///
    /// ## Arguments
    /// * `cut` - Whether the node stopped expanding at a cutoff.
    fn finalized_bound(&self, cut: bool) -> Bound {
        combine_bound(self.is_maximizing, cut, self.best_bound, self.widened)
    }
}

impl Display for Node {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_maximizing {
            write!(f, "↑{}", self.id)
        } else {
            write!(f, "↓{}", self.id)
        }
    }
}

#[inline]
fn log_exploring_node(node: &Node) {
    trace!(
        "Exploring node {node} at depth {depth}; {direction} within α={alpha} β={beta}, best={value} at={best_child:?}",
        depth = node.depth,
        direction = if node.is_maximizing { "maximizing"} else {"minimizing"},
        alpha = node.value.alpha,
        beta = node.value.beta,
        value = node.value.value,
        best_child = node.best_child
    );
}

#[inline]
fn log_max_search_depth_reached(node: &Node) {
    trace!(
        "Search depth reached, terminating search on node {node} with {value}",
        value = *node.value
    );
}

#[inline]
fn log_alpha_cutoff(node: &Node) {
    trace!(
        "Alpha cutoff at value={value} <= α={alpha} - stopping expansion",
        value = node.value.value,
        alpha = node.value.alpha
    );
}

#[inline]
fn log_beta_cutoff(node: &Node) {
    trace!(
        "Beta cutoff at value={value} >= β={beta} - stopping expansion",
        value = node.value.value,
        beta = node.value.beta
    );
}

#[inline]
fn log_minimizer_detected_defeat(node: &Node) {
    trace!(
        "Minimizer found defeat with value={value} - stopping expansion",
        value = node.value.value
    );
}

#[inline]
fn log_node_fully_explored(node: &Node, nodes: &[Node]) {
    trace!(
        "Node {node} (child of {parent_node}) fully explored, got value {value}",
        value = *node.value,
        parent_node = nodes[node.parent_id.unwrap_or(0)]
    );
}

#[inline]
fn log_node_terminal_state(node: &Node, value: &TerminalState, nodes: &[Node]) {
    match value {
        TerminalState::Win(value) => trace!(
            "Node {node} (child of {parent_node}) is a terminal, got value {value} (win)",
            parent_node = nodes[node.parent_id.unwrap_or(0)]
        ),
        TerminalState::Defeat(value) => trace!(
            "Node {node} (child of {parent_node}) is a terminal, got value {value} (defeat)",
            parent_node = nodes[node.parent_id.unwrap_or(0)]
        ),
        TerminalState::Remain(value) => trace!(
            "Node {node} (child of {parent_node}) is a terminal, got value {value} (opponent retreated)",
            parent_node = nodes[node.parent_id.unwrap_or(0)]
        ),
        TerminalState::Retreat(value) => trace!(
            "Node {node} (child of {parent_node}) is a terminal, got value {value} (retreat)",
            parent_node = nodes[node.parent_id.unwrap_or(0)]
        ),
        TerminalState::Heuristic(value) => trace!(
            "Node {node} (child of {parent_node}) exhausted, got value {value} (heuristic)",
            parent_node = nodes[node.parent_id.unwrap_or(0)]
        ),
        TerminalState::OpenUnexplored(_) => {
            unreachable!("An open/unexplored node must have a defined terminal value")
        }
    }
}

#[inline]
fn log_expand_node_with_action(node: &Node, child_node: &Node, action: &AppliedAction) {
    trace!("Expand node {node} into {child_node} with action: {action}");
}

#[inline]
fn log_increase_search_depth_to(depth: usize, max_depth: usize) {
    trace!("Increase search depth to {depth}/{max_depth}");
}

#[inline]
fn log_stop_iddfs(max_depth: usize) {
    trace!("Iterative deepening DFS stopped at depth {max_depth}");
}

#[inline]
fn log_finish_iddfs(depth: usize) {
    trace!("Iterative deepening DFS finished at depth {depth}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::party_member::{Morale, PartyMember};
    use crate::weapon::{Fists, Stick, Weapon};

    #[test]
    fn simple_fight_works() {
        // The hero is faster, so this is identical to strict alternation.
        let heroes = build_default_hero_party(false, 25.0, 6);
        let villains = build_simple_villain_party();

        let conflict = Conflict::new(heroes, villains);

        let solution = Solver::engage(&conflict, SolverStrategy::DepthLimited(200));
        assert_eq!(solution.outcome, OutcomeType::Win(5.0));
    }

    #[test]
    fn complex_fight_works() {
        let conflict = complex_works_conflict();

        // In this version, the enemy is not allowed to flee. The hero kills
        // Molphige first, then trades blows with Ziuon.
        let solution = Solver::engage(&conflict, SolverStrategy::DepthLimited(100));
        assert_eq!(solution.outcome, OutcomeType::Win(10.0));
        assert_eq!(solution.len(), 5);
    }

    #[test]
    fn complex_fight_opponent_flees() {
        let conflict = complex_flees_conflict();

        // The hero kills Molphige. Fleeing later would let the hero's parting
        // blow finish Ziuon off, so he flees at once and survives it with 5
        // health: 20 health * 0.1.
        let solution = Solver::engage(&conflict, SolverStrategy::DepthLimited(100));
        assert_eq!(solution.outcome, OutcomeType::Remain(2.0));
        assert_eq!(solution.len(), 3);
        assert_eq!(solution.timeline[1].action, AppliedAction::Flee);
    }

    #[test]
    fn complex_fight_initiator_cannot_outrun_faster_enemy() {
        let heroes = build_default_hero_party(true, 20.0, 6); // 👈 an opponent exists that does equal damage
        let villains = build_complex_villain_party(true, 15.0); // 👈 more health than hero does damage

        let conflict = Conflict::new(heroes, villains);

        let solution = Solver::engage(&conflict, SolverStrategy::DepthLimited(100));

        // Fleeing in slot 0 still lets Molphige (slot 1) strike, which
        // defeats the hero in one hit.
        assert_eq!(solution.outcome, OutcomeType::Lose(-20.0));
        assert_eq!(solution.len(), 2);
    }

    #[test]
    fn complex_fight_initiator_defeated() {
        let heroes = build_default_hero_party(false, 20.0, 6); // 👈 an opponent exists that does equal damage
        let villains = build_complex_villain_party(true, 15.0); // 👈 more health than hero does damage

        let conflict = Conflict::new(heroes, villains);

        let solution = Solver::engage(&conflict, SolverStrategy::DepthLimited(100));

        // Any first move is answered by Molphige's 20-damage punch.
        assert_eq!(solution.outcome, OutcomeType::Lose(-20.0));
        assert_eq!(solution.len(), 2);
    }

    #[test]
    fn slow_initiator_pays_parting_blows_when_fleeing() {
        // Order: Molphige(5), Ziuon(3), hero(1). Hero takes 20 + 5, flees from
        // the last slot, and both villains strike again: Retreat(-50 * 0.1).
        let conflict = Conflict::new(
            build_default_hero_party(true, 60.0, 1),
            build_complex_villain_party(true, 15.0),
        );
        let solution = Solver::engage(&conflict, SolverStrategy::DepthLimited(100));
        assert_eq!(solution.outcome, OutcomeType::Retreat(-5.0));
        assert_eq!(solution.len(), 5);
        assert_eq!(solution.timeline[2].action, AppliedAction::Flee);
        assert_eq!(
            solution.timeline[0].actor,
            Participant {
                party_id: 1,
                member_id: 1
            }
        );
    }

    #[test]
    fn events_carry_round_slot_and_actor() {
        let conflict = Conflict::new(
            build_default_hero_party(true, 20.0, 6),
            build_complex_villain_party(false, 10.0),
        );
        let solution = Solver::engage(&conflict, SolverStrategy::DepthLimited(100));
        let e = &solution.timeline;
        assert_eq!((e[0].round, e[0].slot), (1, 0)); // Brull
        assert_eq!((e[1].round, e[1].slot), (1, 2)); // Ziuon; Molphige's slot skipped
        assert_eq!((e[2].round, e[2].slot), (2, 0));
        assert_eq!(
            e[1].actor,
            Participant {
                party_id: 1,
                member_id: 0
            }
        );
    }

    #[test]
    fn already_decided_conflict_has_empty_timeline() {
        let mut villains = build_simple_villain_party();
        villains.members[0].health = 0.0;
        let conflict = Conflict::new(build_default_hero_party(false, 20.0, 6), villains);
        let solution = Solver::engage(&conflict, SolverStrategy::IterativeDeepening(10));
        assert!(solution.timeline.is_empty());
        assert!(matches!(solution.outcome, OutcomeType::Win(_)));
    }

    fn simple_conflict() -> Conflict {
        Conflict::new(
            build_default_hero_party(false, 25.0, 6),
            build_simple_villain_party(),
        )
    }

    #[test]
    fn play_continues_past_the_lookahead() {
        let conflict = simple_conflict();
        let strategy = SolverStrategy::DepthLimited(2);
        assert!(matches!(
            Solver::engage(&conflict, strategy).outcome,
            OutcomeType::Unknown(_)
        ));

        let played = Solver::play(&conflict, strategy, MAX_ROUNDS);
        assert_eq!(played.outcome, OutcomeType::Win(5.0));
        let end = &played.timeline.last().expect("events").state;
        assert!(end.next_turn().is_none());
        assert_eq!(played.timeline.len(), 5);
    }

    #[test]
    fn play_matches_a_full_search_when_it_sees_the_end() {
        let conflict = complex_works_conflict();
        let strategy = SolverStrategy::DepthLimited(100);
        let searched = Solver::engage(&conflict, strategy);
        let played = Solver::play(&conflict, strategy, MAX_ROUNDS);
        assert_eq!(played.outcome, searched.outcome);
        assert_eq!(played.timeline.len(), searched.timeline.len());
        assert!(played.evaluations >= searched.evaluations);
    }

    #[test]
    fn play_stops_at_the_round_limit() {
        let played = Solver::play(&simple_conflict(), SolverStrategy::DepthLimited(2), 1);
        assert!(matches!(played.outcome, OutcomeType::Unknown(_)));
        assert!(played.timeline.iter().all(|e| e.round == 1));
        assert_eq!(played.timeline.len(), 2);
    }

    #[test]
    fn play_of_a_decided_conflict_is_empty() {
        let mut villains = build_simple_villain_party();
        villains.members[0].health = 0.0;
        let conflict = Conflict::new(build_default_hero_party(false, 20.0, 6), villains);
        let played = Solver::play(
            &conflict,
            SolverStrategy::IterativeDeepening(10),
            MAX_ROUNDS,
        );
        assert!(played.timeline.is_empty());
        assert!(matches!(played.outcome, OutcomeType::Win(_)));
    }

    #[test]
    fn principal_variation_is_exact_when_fight_is_won() {
        let solution = Solver::engage(&complex_works_conflict(), SolverStrategy::DepthLimited(100));
        assert!(solution.timeline.iter().all(|e| e.bound == Bound::Exact));
        assert_eq!(solution.timeline[0].score, TerminalState::Win(10.0));
    }

    #[test]
    fn first_move_lists_all_other_options() {
        // Brull: fists/stare x Ziuon/Molphige + flee = 5 legal moves.
        let e = &Solver::engage(&complex_works_conflict(), SolverStrategy::DepthLimited(100))
            .timeline[0];
        assert_eq!(e.alternatives.len() + e.unexamined, 4);
        // Punching Ziuon lets Molphige punch back: cut off as soon as the defeat is seen.
        assert!(e
            .alternatives
            .iter()
            .any(|a| a.bound == Bound::AtMost && a.score == TerminalState::Defeat(-20.0)));
        let punch_ziuon = e
            .alternatives
            .iter()
            .find(|a| matches!(&a.action, AppliedAction::Targeted(t) if is_weapon_attack(t, 1, 0)))
            .expect("Punching Ziuon is an alternative");
        assert_eq!(punch_ziuon.bound, Bound::AtMost);
        assert_eq!(punch_ziuon.score, TerminalState::Defeat(-20.0));
    }

    #[test]
    fn bounds_propagate_through_fully_explored_nodes() {
        // The hero cannot win: every round-2 hero move is cut off at Molphige's lethal
        // reply (AtMost). Ziuon's round-1 hit leads to a maximizing node that explores
        // all of those moves without a cutoff itself, so its value is AtMost too.
        let conflict = Conflict::new(
            build_default_hero_party(false, 30.0, 6),
            build_complex_villain_party(false, 15.0),
        );
        let solution = Solver::engage(&conflict, SolverStrategy::DepthLimited(100));
        let e = &solution.timeline[2];
        assert_eq!(
            e.actor,
            Participant {
                party_id: 1,
                member_id: 0
            }
        );
        assert_eq!(e.score, TerminalState::Defeat(-30.0));
        assert_eq!(e.bound, Bound::AtMost);
    }

    fn is_weapon_attack(action: &TargetedAction, party_id: usize, member_id: usize) -> bool {
        let Action::SimpleAttack(attack) = &action.action;
        attack.weapon.is_some()
            && action.source
                == Participant {
                    party_id: 0,
                    member_id: 0,
                }
            && action.target
                == Participant {
                    party_id,
                    member_id,
                }
    }

    #[test]
    fn alternatives_are_ranked_from_the_movers_perspective() {
        for conflict in [complex_works_conflict(), complex_flees_conflict()] {
            let solution = Solver::engage(&conflict, SolverStrategy::DepthLimited(100));
            for e in &solution.timeline {
                let hero = e.actor.party_id == conflict.initiator.id;
                for pair in e.alternatives.windows(2) {
                    if hero {
                        assert!(pair[0].score >= pair[1].score)
                    } else {
                        assert!(pair[0].score <= pair[1].score)
                    }
                }
                for a in &e.alternatives {
                    if hero {
                        assert!(a.score <= e.score)
                    } else {
                        assert!(a.score >= e.score)
                    }
                }
            }
        }
    }

    #[test]
    fn forced_defeat_reports_bounds_and_unexamined_moves() {
        // Every hero move is answered by a lethal punch; the minimizer stops at the first
        // defeat it sees, so the chosen line is itself a bound.
        let conflict = Conflict::new(
            build_default_hero_party(false, 20.0, 6),
            build_complex_villain_party(true, 15.0),
        );
        let solution = Solver::engage(&conflict, SolverStrategy::DepthLimited(100));
        assert_eq!(solution.timeline[0].bound, Bound::AtMost);
        // Molphige had punch, stare and flee; only the punch was generated.
        assert_eq!(solution.timeline[1].unexamined, 2);
    }

    fn complex_works_conflict() -> Conflict {
        let heroes = build_default_hero_party(true, 20.0, 6); // 👈 an opponent exists that does equal damage
        let villains = build_complex_villain_party(false, 10.0);
        Conflict::new(heroes, villains)
    }

    fn complex_flees_conflict() -> Conflict {
        let heroes = build_default_hero_party(true, 20.0, 6); // 👈 an opponent exists that does equal damage
        let villains = build_complex_villain_party(true, 10.0); // 👈 equal health to the hero damage
        Conflict::new(heroes, villains)
    }

    fn build_default_hero_party(can_retreat: bool, health: f32, speed: u32) -> Party {
        Party {
            id: 0,
            members: vec![PartyMember {
                id: 0,
                health,
                damage_taken: 0.0,
                weapon: Weapon::Fists(Fists { damage: 10.0 }),
                can_act: true,
                speed,
                morale: Morale::Tactical,
                fled: false,
            }],
            can_retreat,
        }
    }

    fn build_complex_villain_party(can_retreat: bool, dangerous_health: f32) -> Party {
        Party {
            id: 1,
            members: vec![
                PartyMember {
                    id: 0,
                    health: 15.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Stick(Stick { damage: 5.0 }),
                    can_act: true,
                    speed: 3,
                    morale: Morale::Tactical,
                    fled: false,
                },
                PartyMember {
                    id: 1,
                    health: dangerous_health,
                    damage_taken: 0.0,
                    weapon: Weapon::Fists(Fists {
                        damage: 20.0, // 👈 may defeat hero in one hit
                    }),
                    can_act: true,
                    speed: 5,
                    morale: Morale::Tactical,
                    fled: false,
                },
            ],
            can_retreat,
        }
    }

    fn build_simple_villain_party() -> Party {
        Party {
            id: 1,
            members: vec![PartyMember {
                id: 0,
                health: 25.0,
                damage_taken: 0.0,
                weapon: Weapon::Stick(Stick { damage: 10.0 }),
                can_act: true,
                speed: 3,
                morale: Morale::Tactical,
                fled: false,
            }],
            can_retreat: false,
        }
    }

    #[test]
    fn same_value_works() {
        assert_eq!(f32::INFINITY, f32::INFINITY);
        assert_eq!(f32::NEG_INFINITY, f32::NEG_INFINITY);
        assert_ne!(f32::INFINITY, f32::NEG_INFINITY);
        assert_ne!(f32::NEG_INFINITY, f32::INFINITY);
    }
}
