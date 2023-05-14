use crate::conflict::{Conflict, Factions};
use crate::party::Party;
use std::collections::VecDeque;

pub struct Solver;

impl Solver {
    /// Predicts the sequence of optimal moves to resolve the conflict,
    /// in favor of the initiating party.
    pub fn engage(conflict: Conflict) {
        let max_depth = 5; // TODO: arbitrarily chosen number
        Self::minimax(conflict, max_depth);
    }

    fn minimax(conflict: Conflict, max_depth: usize) {
        // We start with a maximizing step, so the value is
        // initialized to negative infinity.
        let mut nodes = vec![Node {
            id: 0,
            parent_id: None,
            child_ids: Vec::default(),
            depth: max_depth,
            is_maximizing: true,
            value: f32::NEG_INFINITY,
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

        todo!("searched full tree")
    }

    /// Propagates known terminal utility values upwards in the
    /// search tree.
    fn propagate_values(nodes: &mut Vec<Node>, node: &Node) {
        let mut parent_id = node.parent_id;
        let mut child_id = node.id;
        while let Some(id) = parent_id {
            // Note that the child value is only finite if we reached
            // a terminal state and will be ±infinite if the search
            // terminated due to search depth limitation.
            let child_value = nodes[child_id].value;

            let node = &mut nodes[id];
            if node.is_maximizing {
                node.value = child_value.max(node.value);
            } else {
                node.value = child_value.min(node.value);
            }

            // TODO: terminate propagation if value did not change.

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

#[derive(Debug, Clone)]
struct Node {
    pub id: usize,
    pub parent_id: Option<usize>,
    pub child_ids: Vec<usize>,
    pub depth: usize,
    pub is_maximizing: bool,
    pub value: f32,
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
}
