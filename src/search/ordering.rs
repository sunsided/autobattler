//! Heuristic move ordering: the moves most likely to cut the search come first.

use crate::action::{Action, AppliedAction};
use crate::conflict::Conflict;
use std::cmp::Ordering;

/// The indices into `moves`, most promising first.
///
/// Lethal attacks come first, then the other attacks on the weakest targets
/// (harder hitters first among equals), then fleeing, then retreating. The
/// sort is stable, so ties keep their original order.
pub fn order(state: &Conflict, moves: &[AppliedAction]) -> Vec<usize> {
    let mut indices: Vec<usize> = (0..moves.len()).collect();
    indices.sort_by(|&a, &b| compare(&key(state, &moves[a]), &key(state, &moves[b])));
    indices
}

/// How a move ranks: lower sorts first.
enum Key {
    /// An attack that defeats its target.
    Lethal,
    /// An attack that leaves its target standing.
    Wounding {
        health: f32,
        damage: f32,
    },
    Flee,
    Retreat,
}

fn key(state: &Conflict, action: &AppliedAction) -> Key {
    match action {
        AppliedAction::Targeted(targeted) => {
            let Action::SimpleAttack(attack) = &targeted.action;
            let health = state.targeted_member(&targeted.target).health;
            if attack.damage >= health {
                Key::Lethal
            } else {
                Key::Wounding {
                    health,
                    damage: attack.damage,
                }
            }
        }
        AppliedAction::Flee => Key::Flee,
        AppliedAction::Retreat => Key::Retreat,
    }
}

fn compare(a: &Key, b: &Key) -> Ordering {
    let class = |key: &Key| match key {
        Key::Lethal => 0,
        Key::Wounding { .. } => 1,
        Key::Flee => 2,
        Key::Retreat => 3,
    };
    match (a, b) {
        (
            Key::Wounding {
                health: a_health,
                damage: a_damage,
            },
            Key::Wounding {
                health: b_health,
                damage: b_damage,
            },
        ) => a_health
            .total_cmp(b_health)
            .then(b_damage.total_cmp(a_damage)),
        _ => class(a).cmp(&class(b)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::party::Party;
    use crate::party_member::{Morale, PartyMember};
    use crate::solver::moves;
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

    /// A hero with fists (10 damage) facing villains of the given healths.
    fn conflict(villain_healths: &[f32]) -> Conflict {
        let heroes = Party {
            id: 0,
            // A second hero, so that retreating is on offer.
            members: vec![
                member(0, 50.0, Weapon::Fists(Fists { damage: 10.0 }), 10),
                member(1, 50.0, Weapon::Fists(Fists { damage: 10.0 }), 9),
            ],
            can_retreat: true,
        };
        let villains = Party {
            id: 1,
            members: villain_healths
                .iter()
                .enumerate()
                .map(|(id, &health)| member(id, health, Weapon::Stick(Stick { damage: 1.0 }), 1))
                .collect(),
            can_retreat: false,
        };
        Conflict::new(heroes, villains)
    }

    /// The ordered moves of the hero's first turn, as (kind, target health, damage).
    fn ordered(state: &Conflict) -> Vec<(&'static str, f32, f32)> {
        let turn = state.next_turn().expect("a turn");
        let moves = moves(state, &turn);
        order(state, &moves)
            .into_iter()
            .map(|index| match &moves[index] {
                AppliedAction::Flee => ("flee", 0.0, 0.0),
                AppliedAction::Retreat => ("retreat", 0.0, 0.0),
                AppliedAction::Targeted(t) => {
                    let Action::SimpleAttack(attack) = &t.action;
                    (
                        "attack",
                        state.targeted_member(&t.target).health,
                        attack.damage,
                    )
                }
            })
            .collect()
    }

    #[test]
    fn lethal_first_then_weakest_then_flee_then_retreat() {
        // Unarmed attacks (1 damage) never kill here, so they rank by health too.
        assert_eq!(
            ordered(&conflict(&[25.0, 8.0, 15.0])),
            [
                ("attack", 8.0, 10.0),
                ("attack", 8.0, 1.0),
                ("attack", 15.0, 10.0),
                ("attack", 15.0, 1.0),
                ("attack", 25.0, 10.0),
                ("attack", 25.0, 1.0),
                ("flee", 0.0, 0.0),
                ("retreat", 0.0, 0.0),
            ]
        );
    }

    #[test]
    fn ties_keep_their_original_order() {
        let state = conflict(&[15.0, 15.0]);
        let turn = state.next_turn().expect("a turn");
        let moves = moves(&state, &turn);
        let order = order(&state, &moves);
        // Both targets tie on health and damage, so the pairs stay in place.
        let target = |index: usize| match &moves[index] {
            AppliedAction::Targeted(t) => t.target.member_id,
            other => panic!("expected an attack, got {other:?}"),
        };
        let heavy: Vec<usize> = order[..2].iter().map(|&i| target(i)).collect();
        let light: Vec<usize> = order[2..4].iter().map(|&i| target(i)).collect();
        assert_eq!(heavy, [0, 1]);
        assert_eq!(light, [0, 1]);
    }
}
