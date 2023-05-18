use crate::action::AppliedAction;
use crate::conflict::Conflict;
use crate::party::Participant;
use log::trace;
use std::collections::VecDeque;

pub struct Solver;

impl Solver {
    /// Predicts the sequence of optimal moves to resolve the conflict,
    /// in favor of the initiating party.
    ///
    /// ## Arguments
    /// * `conflict` - The conflict situation to resolve.
    ///
    /// ## Returns
    /// The [`Outcome`] of the conflict.
    pub fn engage(conflict: &Conflict) -> Outcome {
        let max_depth = 200; // TODO: arbitrarily chosen number
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
        let mut nodes = vec![Node::new_root(conflict.clone(), max_depth)];

        let mut dfs_queue = VecDeque::from([0]);
        'dfs: while let Some(id) = dfs_queue.pop_front() {
            // The clone here is a hack to get around borrowing rules.
            let mut node = nodes[id].clone();

            // If this is a terminal node we either have a winner or loser.
            // TODO: We may decide to stop searching if the initiating party wins or decide to find the best possible outcome.
            //       Since all of minimax assumes both players play optimally, selecting the optimal win (i.e. the highest
            //       possible value) may give more tolerance for erratic behavior of the opponent.
            if let Some(value) = Self::get_utility(&node.state) {
                // Update the value in the nodes set first before iterating.
                node.value = value;
                node.best_child = Some(id);
                nodes[id].value = value;
                nodes[id].best_child = Some(id);
                Self::propagate_values(&mut nodes, &mut node);
                continue 'dfs;
            }

            // Also terminate iteration if the look-ahead depth is reached.
            if node.depth == 0 {
                node.best_child = Some(id);
                nodes[id].best_child = Some(id);
                Self::propagate_values(&mut nodes, &mut node);
                continue 'dfs;
            }

            trace!("Exploring node {} at depth {}", node.id, node.depth);
            let node = match Self::minimax_expand(node, &mut nodes, &mut dfs_queue) {
                None => continue 'dfs,
                Some(node) => node,
            };

            // Replace the node in the original array with our clone.
            let node_id = node.id;
            nodes[node_id] = node;
        }

        Self::backtrack(nodes, max_depth)
    }

    /// Implements the minimax recursion as an expansion of the search tree.
    fn minimax_expand(
        mut node: Node,
        nodes: &mut Vec<Node>,
        frontier: &mut VecDeque<usize>,
    ) -> Option<Node> {
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

        // Collect all child IDs to later update the node.
        let mut child_ids = Vec::default();

        // Members take actions in turns.
        let source_party_id = current.id;

        let members: Vec<_> = current
            .members
            .iter()
            .filter(|&m| m.id >= node.continue_with_member)
            .filter(|&m| !m.is_dead())
            .collect();

        // Not all members can be dead, as that was checked before, but we could have
        // exhausted our party's moves.
        let last_id = match members.last() {
            None => return None,
            Some(node) => node.id,
        };

        for member in members {
            // Only alive members are allowed to play.
            if member.is_dead() {
                continue;
            }

            let member_id = member.id;
            let is_last_in_party = last_id == member_id;

            // Each member can perform a variety of actions.
            for action in member.clone().actions() {
                // Each action can target an opponent or a party member.
                // TODO: An endless cycle may occur if we choose to heal opponents if that effects the utility (e.g. XP collected).
                let target_party_id = opponent.id;
                for target in &opponent.members {
                    // Only alive targets are allowed to play. See above TODO.
                    if target.is_dead() {
                        continue;
                    }

                    let target_id = target.id;

                    // TODO: Optimize state creation - only clone when action was applied.
                    let mut target = target.clone();
                    if !target.handle_action(&action) {
                        continue;
                    }

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
                            member_id,
                        },
                        target: Participant {
                            party_id: target_party_id,
                            member_id: target_id,
                        },
                    };

                    // If this is not the last member in the party we need to chain more
                    // moves. This will create multiple maximize/minimize layers in the tree.
                    let node = if is_last_in_party {
                        Node::next_party(nodes.len(), &node, state, action)
                    } else {
                        Node::next_in_same_party(nodes.len(), &node, state, member_id, action)
                    };

                    child_ids.push(node.id);
                    frontier.push_back(node.id);
                    nodes.push(node);
                }

                // TODO: We can also target ourselves.
            }

            // Ensure we expand only one party member at a time.
            if !is_last_in_party {
                break;
            }
        }

        // Append the newly generated child IDs to the current node.
        node.child_ids.append(&mut child_ids);
        Some(node)
    }

    /// Backtracks the events from the start to one of the the most likely outcomes.
    fn backtrack(nodes: Vec<Node>, max_depth: usize) -> Outcome {
        // The outcome is positive only if the value of the start
        // node is positive and under the assumption that the opposing
        // player attempts to play optimally.
        let value = nodes[0].value;
        let outcome = if value.is_infinite() {
            OutcomeType::Unknown
        } else if value >= 0.0 {
            OutcomeType::Win(value)
        } else {
            OutcomeType::Lose(value)
        };

        let mut stack = Vec::default();

        let mut node = &nodes[nodes[0].best_child.expect("A best child node is required")];
        'backtracking: loop {
            stack.push(Event {
                turn: node.turn,
                is_initiator_turn: node.is_maximizing,
                action: node.action.clone().expect(""),
                state: node.state.clone(),
                depth: max_depth - node.depth,
            });

            if let Some(parent_id) = node.parent_id {
                // The root node has no action, so stop here.
                if parent_id == 0 {
                    break;
                }

                node = &nodes[parent_id];
                continue 'backtracking;
            }

            break;
        }

        Outcome {
            outcome,
            timeline: stack.into_iter().rev().collect(),
        }
    }

    /// Propagates known terminal utility values upwards in the
    /// search tree.
    fn propagate_values(nodes: &mut Vec<Node>, node: &Node) {
        let mut parent_id = node.parent_id;
        let mut child_id = node.id;
        let mut outcome_changed = true;

        debug_assert_ne!(node.best_child, None, "No best child set");

        while let Some(id) = parent_id {
            // Note that the child value is only finite if we reached
            // a terminal state and will be ±infinite if the search
            // terminated due to search depth limitation.
            let child_value = nodes[child_id].value;
            let best_child = nodes[child_id].best_child;

            let node = &mut nodes[id];
            let old_value = node.value;

            if node.is_maximizing {
                node.value = child_value.max(node.value);
            } else {
                node.value = child_value.min(node.value);
            }

            // TODO: terminate propagation if value did not change.
            if !outcome_changed {
                // Here only for sanity checking. We'd like to terminate
                // the upwards propagation instead.
                debug_assert_eq!(
                    node.value, old_value,
                    "If the value started being unchanged, no new changes are introduced upstream."
                );
            } else if child_value.is_finite() && node.value == old_value {
                outcome_changed = false;
            }

            if outcome_changed || node.best_child.is_none() {
                node.best_child = best_child;
                debug_assert_ne!(node.best_child, None, "No best child detected");
            }

            parent_id = node.parent_id;
            child_id = id;
        }
    }

    /// Gets the utility of the current node. Will return `None` if this
    /// is not a terminal state, i.e. no party has won or lost.
    fn get_utility(state: &Conflict) -> Option<f32> {
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
            Some(utility)
        } else if state.opponent.is_defeated() {
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
            Some(utility)
        } else {
            // Neither party has lost, so this is not a terminal state.
            None
        }
    }
}

/// An outcome of a conflict.
pub struct Outcome {
    /// Whether the initiating party wins the conflict.
    pub outcome: OutcomeType,
    /// An optimal path of actions leading to the outcome.
    pub timeline: Vec<Event>,
}

/// The type of outcome.
pub enum OutcomeType {
    /// The initiating party wins.
    Win(f32),
    /// The initiating party loses.
    Lose(f32),
    /// Unknown outcome.
    Unknown,
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

/// A node in the game tree.
#[derive(Debug, Clone)]
struct Node {
    /// The ID of the node; corresponds to the index of the node
    /// in the vector of explored nodes.
    pub id: usize,
    /// The ID of the node's immediate parent.
    /// Is [`None`] only for the root node.
    pub parent_id: Option<usize>,
    /// The IDs of the node's children. Only meaningful
    /// if a terminal state was found, i.e. [`value`] is finite.
    pub child_ids: Vec<usize>,
    /// The turn at which this node appeared.
    pub turn: usize,
    /// The depth of the node. If it reaches zero, search is terminated.
    pub depth: usize,
    /// The member with whose ID we wish to continue to explore the tree.
    pub continue_with_member: usize,
    /// Whether this is a maximizing or minimizing node in minimax.
    /// If maximizing, the represents a move of the initiating party of the conflict.
    pub is_maximizing: bool,
    /// The utility value of this node. Only meaningful if this is
    /// a terminal node (i.e. win or loss for either side).
    pub value: f32,
    /// The ID of the child that optimized the value.
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
            child_ids: Vec::default(),
            depth: max_depth,
            turn: 0,
            continue_with_member: 0,
            is_maximizing: true,
            value: f32::NEG_INFINITY,
            best_child: None,
            action: None,
            state: conflict,
        }
    }

    /// Ends this party's turn and by changing from maximizing to minimizing
    /// and vice versa.
    pub fn next_party(id: usize, node: &Node, state: Conflict, action: AppliedAction) -> Self {
        Self {
            id,
            parent_id: Some(node.id),
            child_ids: Vec::default(),
            is_maximizing: !node.is_maximizing,
            value: if node.is_maximizing {
                // A minimizing node's starting value is positive infinity.
                f32::INFINITY
            } else {
                // A maximizing node's starting value is negative infinity.
                f32::NEG_INFINITY
            },
            best_child: None,
            depth: node.depth - 1,
            turn: node.turn + 1,
            continue_with_member: 0,
            action: Some(action),
            state,
        }
    }

    /// Creates a new node from within the same party. Assumes that
    /// this is not the last action to be taken. If you need to make
    /// the last move in the same party, use [`Node::next_party`] instead.
    pub fn next_in_same_party(
        id: usize,
        node: &Node,
        state: Conflict,
        member_id: usize,
        action: AppliedAction,
    ) -> Self {
        Self {
            id,
            parent_id: Some(node.id),
            child_ids: Vec::default(),
            is_maximizing: node.is_maximizing,
            value: node.value,
            best_child: node.best_child,
            depth: node.depth + 1,
            turn: node.turn + 1,
            continue_with_member: member_id + 1,
            action: Some(action),
            state,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::party::Party;
    use crate::party_member::PartyMember;
    use crate::weapon::{Stick, Weapon};

    #[test]
    fn it_works() {
        let villains = Party {
            id: 0,
            members: vec![PartyMember {
                id: 0,
                health: 25.0,
                damage_taken: 0.0,
                weapon: Weapon::Stick(Stick { damage: 10.0 }),
            }],
        };

        let heroes = Party {
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

        Solver::engage(&conflict);
    }

    #[test]
    fn same_value_works() {
        assert_eq!(f32::INFINITY, f32::INFINITY);
        assert_eq!(f32::NEG_INFINITY, f32::NEG_INFINITY);
        assert_ne!(f32::INFINITY, f32::NEG_INFINITY);
        assert_ne!(f32::NEG_INFINITY, f32::INFINITY);
    }
}
