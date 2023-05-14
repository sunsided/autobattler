use crate::action::{ActionTarget, AppliedAction};
use crate::conflict::Conflict;
use crate::party::Party;
use std::collections::VecDeque;

pub struct Solver;

impl Solver {
    /// Predicts the sequence of optimal moves to resolve the conflict,
    /// in favor of the initiating party.
    pub fn engage(conflict: Conflict) -> Outcome {
        let max_depth = 5; // TODO: arbitrarily chosen number
        Self::minimax(conflict, max_depth)
    }

    fn minimax(conflict: Conflict, max_depth: usize) -> Outcome {
        // We start with a maximizing step, so the value is
        // initialized to negative infinity.
        let mut nodes = vec![Node {
            id: 0,
            parent_id: None,
            child_ids: Vec::default(),
            depth: max_depth,
            is_maximizing: true,
            value: f32::NEG_INFINITY,
            action: None,
            state: conflict.clone(),
        }];

        let mut dfs_queue = VecDeque::from([0]);
        'dfs: while let Some(id) = dfs_queue.pop_front() {
            // The clone here is a hack to get around borrowing rules.
            let mut node = nodes[id].clone();

            // TODO: Test if this is terminal; if so, update the value.
            if let Some(value) = Self::get_utility(&node.state) {
                // Update the value in the nodes set first before iterating.
                node.value = value;
                nodes[id].value = value;
                Self::propagate_values(&mut nodes, &mut node);
                continue 'dfs;
            }

            // TODO: Also terminate iteration if node.depth == 0
            if node.depth == 0 {
                Self::propagate_values(&mut nodes, &mut node);
                continue 'dfs;
            }

            if node.is_maximizing {
                debug_assert_eq!(node.state.turn & 1, 0);
                let current = &node.state.initiator;
                let opponent = &node.state.opponent;

                // Collect all child IDs to later update the node.
                let mut child_ids = Vec::default();

                // Members take actions in turns.
                for member in &current.members {
                    // Each member can perform a variety of actions.
                    for action in member.actions() {
                        // Each action can target an opponent or a party member.
                        // TODO: An endless cycle may occur if we choose to heal opponents if that effects the utility (e.g. XP collected).
                        for target in &opponent.members {
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
                            let state = Conflict {
                                initiator: current.clone(),
                                opponent,
                                turn: node.state.turn + 1,
                            };

                            // Create a new node in the game tree.
                            let node = Node {
                                id: nodes.len(),
                                parent_id: Some(node.id),
                                child_ids: Vec::default(),
                                value: f32::INFINITY,
                                depth: node.depth - 1,
                                is_maximizing: false,
                                action: Some(AppliedAction {
                                    action: action.clone(),
                                    target: ActionTarget {
                                        party_id: current.id,
                                        member_id: target_id,
                                    },
                                }),
                                state,
                            };

                            child_ids.push(node.id);
                            dfs_queue.push_back(node.id);
                            nodes.push(node);
                        }

                        // TODO: We can also target ourselves.
                    }
                }

                // Append the newly generated child IDs to the current node.
                node.child_ids.append(&mut child_ids);
            } else {
                debug_assert_eq!(node.state.turn & 1, 1);
                let current = &node.state.opponent;
                let opponent = &node.state.initiator;

                // Collect all child IDs to later update the node.
                let mut child_ids = Vec::default();

                // Members take actions in turns.
                for member in &current.members {
                    // Each member can perform a variety of actions.
                    for action in member.actions() {
                        // Each action can target an opponent or a party member.
                        // TODO: An endless cycle may occur if we choose to heal opponents if that effects the utility (e.g. XP collected).
                        for target in &opponent.members {
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
                            let state = Conflict {
                                initiator: opponent,
                                opponent: current.clone(),
                                turn: node.state.turn + 1,
                            };

                            // Create a new node in the game tree.
                            let node = Node {
                                id: nodes.len(),
                                parent_id: Some(node.id),
                                child_ids: Vec::default(),
                                value: f32::NEG_INFINITY,
                                depth: node.depth - 1,
                                is_maximizing: true,
                                action: Some(AppliedAction {
                                    action: action.clone(),
                                    target: ActionTarget {
                                        party_id: current.id,
                                        member_id: target_id,
                                    },
                                }),
                                state,
                            };

                            child_ids.push(node.id);
                            dfs_queue.push_back(node.id);
                            nodes.push(node);
                        }

                        // TODO: We can also target ourselves.
                    }
                }

                // Append the newly generated child IDs to the current node.
                node.child_ids.append(&mut child_ids);
            }

            // Replace the node in the original array with our clone.
            let node_id = node.id;
            nodes[node_id] = node;
        }

        Self::backtrack(nodes)
    }

    /// Backtracks the events from the start to one of the the most likely outcomes.
    fn backtrack(nodes: Vec<Node>) -> Outcome {
        // The outcome is positive only if the value of the start
        // node is positive and under the assumption that the opposing
        // player attempts to play optimally.
        let mut outcome = Outcome {
            win: nodes[0].value > 0.0,
            timeline: Vec::default(),
        };

        let mut node = &nodes[0];
        let mut turn = 0;
        'dfs: loop {
            'child: for child in &node.child_ids {
                let child = &nodes[*child];
                if !Self::values_equal(&child, &node) {
                    continue 'child;
                }

                outcome.timeline.push(Event {
                    turn,
                    action: child.action.clone().expect(""),
                    state: child.state.clone(),
                });
                node = child;
                turn += 1;
                continue 'dfs;
            }

            // No matching child found. End of iteration.
            break;
        }
        outcome
    }

    /// Test if two [`Node`] entries have the same finite value or the same infinity.
    fn values_equal(lhs: &Node, rhs: &Node) -> bool {
        lhs.value == rhs.value
    }

    /// Propagates known terminal utility values upwards in the
    /// search tree.
    fn propagate_values(nodes: &mut Vec<Node>, node: &Node) {
        let mut parent_id = node.parent_id;
        let mut child_id = node.id;
        let mut outcome_changed = true;

        while let Some(id) = parent_id {
            // Note that the child value is only finite if we reached
            // a terminal state and will be ±infinite if the search
            // terminated due to search depth limitation.
            let child_value = nodes[child_id].value;

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
    pub win: bool,
    /// An optimal path of actions leading to the outcome.
    pub timeline: Vec<Event>,
}

/// An event in the timeline.
pub struct Event {
    /// The turn in which an event took place.
    pub turn: usize,
    /// The action that was applied.
    pub action: AppliedAction,
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
    /// The depth of the node. If it reaches zero, search is terminated.
    pub depth: usize,
    /// Whether this is a maximizing or minimizing node in minimax.
    /// If maximizing, the represents a move of the initiating party of the conflict.
    pub is_maximizing: bool,
    /// The utility value of this node. Only meaningful if this is
    /// a terminal node (i.e. win or loss for either side).
    pub value: f32,
    /// The action taken to arrive at this node.
    /// Is [`None`] only for the root node.
    pub action: Option<AppliedAction>,
    /// The state after applying the action.
    pub state: Conflict,
}

#[cfg(test)]
mod tests {
    use super::*;
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

        Solver::engage(conflict);
    }

    #[test]
    fn same_value_works() {
        assert_eq!(f32::INFINITY, f32::INFINITY);
        assert_eq!(f32::NEG_INFINITY, f32::NEG_INFINITY);
        assert_ne!(f32::INFINITY, f32::NEG_INFINITY);
        assert_ne!(f32::NEG_INFINITY, f32::INFINITY);
    }
}
