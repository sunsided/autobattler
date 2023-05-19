use crate::action::AppliedAction;
use crate::action_iterator::ActionIterator;
use crate::conflict::Conflict;
use crate::party::Participant;
use crate::value::{Cutoff, TerminalState, Value};
use log::trace;
use std::fmt::{Display, Formatter};
use std::time::{Duration, Instant};

pub struct Solver;

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
    pub fn engage(conflict: &Conflict, max_depth: usize) -> Outcome {
        let max_depth = max_depth.max(1);
        Self::minimax(conflict, max_depth)
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
        // We start with a maximizing step, so the value is
        // initialized to negative infinity.
        let mut nodes = vec![Node::new_root(conflict.clone(), 0)];

        // Track expansion statistics.
        let mut evaluations = 0;
        let mut pruning_cuts = 0;
        let start_time = Instant::now();

        let mut dfs_queue = Vec::from([0]);
        'dfs: while let Some(id) = dfs_queue.pop() {
            evaluations += 1;

            // The clone here is a hack to get around borrowing rules.
            let mut node = nodes[id].clone();
            log_exploring_node(&node);

            // Terminate iteration if the look-ahead depth is reached.
            // Note that we can only know if a state is terminal once we have
            // fully expanded it. This information is available further below,
            // after the node expansion step.
            let continue_expansion = if node.depth == max_depth {
                *node.value = Self::get_utility(&node.state);
                nodes[node.id].value = node.value.clone();
                log_max_search_reached(&node);
                false
            } else if node.is_maximizing && node.value.is_beta_cutoff() {
                log_beta_cutoff(&node);
                pruning_cuts += 1;
                false
            } else if !node.is_maximizing && node.value.is_alpha_cutoff() {
                log_alpha_cutoff(&node);
                pruning_cuts += 1;
                false
            } else if !node.is_maximizing && node.value.is_negative() {
                log_minimizer_detected_defeat(&node);
                pruning_cuts += 1;
                false
            } else {
                true
            };

            if !continue_expansion {
                Self::propagate_to_parent(&mut nodes, &mut node);
                continue 'dfs;
            }

            // Expand the search tree at the current node.
            let expansion_result = Self::minimax_expand(node, nodes.len());

            // Handle expansion or exhaustion of the node.
            let node = match expansion_result {
                ExpansionResult::Expanded(Expansion { mut parent, child }) => {
                    // Register the child node as a new child.
                    parent.child_nodes.push(child.id);

                    // Since the actions were not exhausted yet, we push the parent first
                    // so that we can continue from it later.
                    dfs_queue.push(parent.id);
                    dfs_queue.push(child.id);

                    nodes.push(child);
                    parent
                }
                ExpansionResult::Exhausted(Exhaustion { mut node }) => {
                    let value = if (*node.value).is_finite() {
                        log_node_fully_explored(&node, &nodes);
                        *node.value
                    } else {
                        // If this is a terminal node we either have a winner or loser.
                        let value = Self::get_utility(&node.state);
                        log_node_terminal_state(&node, &value, &nodes);
                        value
                    };

                    // Update the value in the nodes set first before iterating.
                    node.value.value = value;
                    nodes[id].value = node.value.clone();

                    // Update the value in the nodes set first before iterating.
                    Self::propagate_to_parent(&mut nodes, &mut node);
                    node
                }
            };

            // Replace the node in the original array with our clone.
            let node_id = node.id;
            nodes[node_id] = node;
        }

        let search_duration = Instant::now() - start_time;
        Self::backtrack(nodes, evaluations, pruning_cuts, search_duration)
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

        // Select the currently active party.
        let current = if node.is_maximizing {
            &node.state.initiator
        } else {
            &node.state.opponent
        };

        // Select the current opponent.
        let opponent = if node.is_maximizing {
            &node.state.opponent
        } else {
            &node.state.initiator
        };

        // If we visit this node for the first time, create the iterator.
        // On all subsequent visits we continue from the last-known state.
        if node.action_iter.is_none() {
            node.action_iter = Some(ActionIterator::new(current.clone(), opponent.clone()));
        }

        // Members take actions in turns.
        let source_party_id = current.id;

        if let Some(action) = node.action_iter.as_mut().map_or(None, |i| i.next()) {
            let source_id = action.source.member_id;
            debug_assert_eq!(action.source.party_id, source_party_id);

            let target_party_id = action.target.party_id;
            debug_assert_eq!(action.target.party_id, opponent.id);

            let target_id = action.target.member_id;
            let target = &opponent.members[target_id];

            let action = &action.action;

            // TODO: Optimize state creation - only clone when action was applied.
            let mut target = target.clone();
            if target.handle_action(action) {
                // Branch off and replace the member with the updated state.
                let mut opponent = opponent.clone();
                opponent.replace_member(target);

                // Create a new branch on the board.
                let state = if node.is_maximizing {
                    Conflict {
                        initiator: current.clone(),
                        opponent,
                        turn: node.state.turn + 1,
                    }
                } else {
                    Conflict {
                        initiator: opponent,
                        opponent: current.clone(),
                        turn: node.state.turn + 1,
                    }
                };

                // The action to apply.
                let action = AppliedAction {
                    action: action.clone(),
                    source: Participant {
                        party_id: source_party_id,
                        member_id: source_id,
                    },
                    target: Participant {
                        party_id: target_party_id,
                        member_id: target_id,
                    },
                };

                // If this is not the last member in the party we need to chain more
                // moves. This will create multiple maximize/minimize layers in the tree.
                let child_node = Node::new_branch_from(next_child_id, &node, action, state);

                if let Some(action) = &node.action {
                    log_expand_node_with_action(&node, &child_node, &action);
                }

                return ExpansionResult::new_expansion(node, child_node);
            }
        }

        ExpansionResult::new_exhaustion(node)
    }

    /// Backtracks the events from the start to one of the the most likely outcomes.
    fn backtrack(
        nodes: Vec<Node>,
        evaluations: usize,
        pruning_cuts: usize,
        search_duration: Duration,
    ) -> Outcome {
        // The outcome is positive only if the value of the start
        // node is positive and under the assumption that the opposing
        // player attempts to play optimally.
        let value = nodes[0].value.value;
        let outcome = match value {
            TerminalState::Win(score) => OutcomeType::Win(score),
            TerminalState::Defeat(score) => OutcomeType::Lose(score),
            TerminalState::Heuristic(score) => OutcomeType::Unknown(score),
            TerminalState::OpenUnexplored(score) => OutcomeType::Unknown(score),
        };

        let mut stack = Vec::default();

        let mut node = &nodes[nodes[0].best_child.expect("A best child node is required")];
        'backtracking: loop {
            stack.push(Event {
                turn: node.turn,
                is_initiator_turn: node.is_maximizing,
                action: node.action.clone().expect(""),
                state: node.state.clone(),
                depth: node.depth,
            });

            if let Some(best_child) = node.best_child {
                node = &nodes[best_child];
                continue 'backtracking;
            }

            break;
        }

        Outcome {
            outcome,
            timeline: stack,
            evaluations,
            cuts: pruning_cuts,
            search_duration,
        }
    }

    /// Propagates known terminal utility values upwards in the
    /// search tree.
    fn propagate_to_parent(nodes: &mut Vec<Node>, child_node: &Node) -> Cutoff {
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
                }

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
                }

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

    /// Gets the utility of the current node. Will return `None` if this
    /// is not a terminal state, i.e. no party has won or lost.
    fn get_utility(state: &Conflict) -> TerminalState {
        if state.initiator.is_defeated() {
            // The current party being dead is a terminal state and always is a negative reward.
            // We sum up the total damage taken to punish strong defeats
            // harder than slight defeats.
            let utility = state
                .initiator
                .members
                .iter()
                .map(|m| -m.damage_taken)
                .sum();
            debug_assert!(utility < 0.0);
            return TerminalState::Defeat(utility);
        }

        // As a naive choice, we simply sum up the health of each member.
        // This is to ensure we play less risky and don't need to heal as much.
        // Health can never be negative, but to be sure we cap it at zero.
        //
        // In theory we can also factor in the damage taken by the opponent
        // as dealing more damage could be useful. Whether or not that is a
        // useful idea depends on the remaining game mechanics (say, e.g., a massive
        // magical effect that takes a day to recover vs. death by a slap with a stick).
        let utility = state
            .initiator
            .members
            .iter()
            .map(|m| m.health.max(0.0))
            .sum();
        debug_assert!(utility > 0.0);

        if state.opponent.is_defeated() {
            TerminalState::Win(utility)
        } else {
            TerminalState::Heuristic(utility * 0.1)
        }
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
}

/// The type of outcome.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum OutcomeType {
    /// The initiating party wins.
    Win(f32),
    /// The initiating party loses.
    Lose(f32),
    /// Unknown outcome.
    Unknown(f32),
}

/// An event in the timeline.
pub struct Event {
    /// The turn in which an event took place.
    pub turn: usize,
    /// Whether this turn is performed by the initiating party.
    pub is_initiator_turn: bool,
    /// The action that was applied.
    pub action: AppliedAction,
    /// The depth at which this node was discovered.
    pub depth: usize,
    /// The state of the conflict after the action took place.
    pub state: Conflict,
}

/// A tree expansion outcome.
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
    /// The turn at which this node appeared.
    pub turn: usize,
    /// The depth of the node. If it reaches zero, search is terminated.
    pub depth: usize,
    /// The action iterator; if it yields none, the node is explored completely.
    pub action_iter: Option<ActionIterator>,
    /// Whether this is a maximizing or minimizing node in minimax.
    /// If maximizing, the represents a move of the initiating party of the conflict.
    pub is_maximizing: bool,
    /// The utility value of this node. Only meaningful if this is
    /// a terminal node (i.e. win or loss for either side).
    pub value: Value,
    /// The list of all known direct children of this node.
    pub child_nodes: Vec<usize>,
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
        Self {
            id: 0,
            parent_id: None,
            depth: max_depth,
            turn: 0,
            is_maximizing: true,
            value: Value::new(TerminalState::Heuristic(f32::NEG_INFINITY)),
            best_child: None,
            action: None,
            state: conflict,
            action_iter: None,
            child_nodes: Vec::default(),
        }
    }

    /// Ends this party's turn and by changing from maximizing to minimizing
    /// and vice versa.
    ///
    /// ## Arguments
    /// * `id` - The new ID for the node to be created.
    /// * `parent` - The parent node.
    /// * `action` - The action that lead to the expansion into the child node.
    /// * `state` - The new state observed by the child node after applying the action.
    pub fn new_branch_from(
        id: usize,
        parent: &Node,
        action: AppliedAction,
        state: Conflict,
    ) -> Self {
        let is_maximizing = !parent.is_maximizing;
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
            best_child: None,
            depth: parent.depth + 1,
            turn: parent.turn + 1,
            action: Some(action),
            action_iter: None,
            state,
            child_nodes: Vec::default(),
        }
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
fn log_max_search_reached(node: &Node) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::party::Party;
    use crate::party_member::PartyMember;
    use crate::weapon::{Fists, Stick, Weapon};

    #[test]
    fn simple_fight_works() {
        let heroes = Party {
            id: 0,
            members: vec![PartyMember {
                id: 0,
                health: 25.0,
                damage_taken: 0.0,
                weapon: Weapon::Stick(Stick { damage: 10.0 }),
            }],
        };

        let villains = Party {
            id: 1,
            members: vec![PartyMember {
                id: 0,
                health: 25.0,
                damage_taken: 0.0,
                weapon: Weapon::Stick(Stick { damage: 10.0 }),
            }],
        };

        let conflict = Conflict {
            turn: 0,
            initiator: heroes,
            opponent: villains,
        };

        let solution = Solver::engage(&conflict, 200);
        assert_eq!(solution.outcome, OutcomeType::Win(5.0));
    }

    #[test]
    fn complex_fight_works() {
        let heroes = Party {
            id: 0,
            members: vec![PartyMember {
                id: 0,
                health: 20.0,
                damage_taken: 0.0,
                weapon: Weapon::Fists(Fists { damage: 10.0 }),
            }],
        };

        let villains = Party {
            id: 1,
            members: vec![
                PartyMember {
                    id: 0,
                    health: 15.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Stick(Stick { damage: 5.0 }),
                },
                PartyMember {
                    id: 1,
                    health: 10.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Fists(Fists { damage: 20.0 }),
                },
            ],
        };

        let conflict = Conflict {
            turn: 0,
            initiator: heroes,
            opponent: villains,
        };

        let solution = Solver::engage(&conflict, 100);
        assert_eq!(solution.outcome, OutcomeType::Win(10.0));
    }

    #[test]
    fn same_value_works() {
        assert_eq!(f32::INFINITY, f32::INFINITY);
        assert_eq!(f32::NEG_INFINITY, f32::NEG_INFINITY);
        assert_ne!(f32::INFINITY, f32::NEG_INFINITY);
        assert_ne!(f32::NEG_INFINITY, f32::INFINITY);
    }
}
