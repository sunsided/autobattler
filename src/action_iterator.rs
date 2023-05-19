use crate::action::{Action, AppliedAction};
use crate::party::{Participant, Party};
use crate::party_member::{AttackIterator, PartyMember};
use std::ops::Range;

/// An action iterator.
///
/// The iterator produces all permutations of party member attack actions
/// targeting each opponent. Actions are generated for the first party member,
/// with the first action applied to each individual opponent, then the second
/// action applied to each opponent, etc. If all actions are exhausted for all opponents,
/// the next party member is selected and the process repeats.
#[derive(Debug, Clone)]
pub struct ActionIterator {
    /// The party whose turn it is.
    current: Party,
    /// The opponent's party.
    opponent: Party,
    /// The index of the currently active member.
    current_index: usize,
    /// The index range to address in the current party.
    current_range: Range<usize>,
    /// The iterator used to generated actions targeting an enemy party member..
    iter: Option<ActionTargetIterator>,
}

/// An iterator for attack actions of a single player.
///
/// The iterator generates all permutations of actions and opponents,
/// emitting the same action for each opponent first, then producing the next
/// action for each opponent, etc.
#[derive(Debug, Clone)]
struct ActionTargetIterator {
    /// The current enemy being targeted.
    enemy_index: usize,
    /// The range of party member indices in the party member list.
    enemies: Range<usize>,
    /// The iterator used to generate the actions.
    iter: Option<AttackIterator>,
    /// The last action produced by the [`iter`].
    action: Option<Action>,
}

impl ActionIterator {
    /// Creates a new iterator selecting all current party members,
    /// targeting all enemy party members.
    pub fn new(current: Party, opponent: Party) -> Self {
        let range = 0..current.len();
        Self::new_in(current, opponent, range)
    }

    /// Creates a new iterator selecting only a range of current party members,
    /// targeting all enemy party members.
    pub const fn new_in(current: Party, opponent: Party, current_range: Range<usize>) -> Self {
        Self {
            current,
            opponent,
            current_index: current_range.start,
            current_range,
            iter: None,
        }
    }
}

impl Iterator for ActionIterator {
    type Item = AppliedAction;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If the end of the enumeration was reached, we can exit.
            if self.current_index >= self.current_range.end {
                return None;
            }

            // Ensure the current member can act
            if !self.current.members[self.current_index].can_act() {
                self.current_index += 1;
                continue;
            }

            if self.iter.is_none() {
                let member = self.current.members[self.current_index].clone();
                let target_range = 0..self.opponent.members.len();
                self.iter = Some(ActionTargetIterator::new(member, target_range));
            }

            match self.iter.as_mut().map_or(None, |i| i.next()) {
                None => {
                    // The iterator was exhausted, so we continue with the next member.
                    self.current_index += 1;
                    self.iter = None;
                }
                Some((action, target_index)) => {
                    let opponent = &self.opponent.members[target_index];

                    // TODO: Rework action generation - should only generate applicable actions to begin with.
                    if !opponent.is_applicable(&action) {
                        continue;
                    }

                    let source = Participant {
                        party_id: self.current.id,
                        member_id: self.current.members[self.current_index].id,
                    };

                    let target = Participant {
                        party_id: self.opponent.id,
                        member_id: opponent.id,
                    };

                    return Some(AppliedAction {
                        action,
                        source,
                        target,
                    });
                }
            }
        }
    }
}

impl ActionTargetIterator {
    pub fn new(member: PartyMember, enemies: Range<usize>) -> Self {
        let actions = member.actions();
        let enemy_index = enemies.start;
        Self {
            iter: Some(actions),
            enemies,
            enemy_index,
            action: None,
        }
    }
}

impl Iterator for ActionTargetIterator {
    type Item = (Action, usize);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(actions) = &mut self.iter {
            // Reset to the first enemy if needed and require a new action.
            if self.enemy_index >= self.enemies.end {
                self.enemy_index = self.enemies.start;
                self.action = None;
            }

            // Generate a new action if needed.
            if self.action.is_none() {
                self.action = actions.next();
            }

            // If there is an action, apply it to the current enemy.
            if let Some(action) = self.action.as_ref() {
                let index = self.enemy_index;
                self.enemy_index += 1;
                return Some((action.clone(), index));
            }

            // At this point no action was generated, i.e. the generator
            // is exhausted. Discard the entire generator.
            self.iter = None;
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::SimpleAttackAction;
    use crate::weapon::{Fists, Stick, Weapon};

    #[test]
    fn action_target_iterator_works() {
        let mut iter = ActionTargetIterator::new(
            PartyMember {
                id: 0,
                health: 25.0,
                damage_taken: 0.0,
                weapon: Weapon::Stick(Stick { damage: 10.0 }),
            },
            0..10,
        );

        for t in 0..10 {
            assert_eq!(
                iter.next(),
                Some((
                    Action::SimpleAttack(SimpleAttackAction {
                        weapon: Some(Weapon::Stick(Stick { damage: 10.0 })),
                        damage: 10.0
                    }),
                    t
                ))
            );
        }

        assert_eq!(iter.next(), None);
    }

    /// Same test as [`action_target_iterator_works`], but this one uses a different index range.
    /// This test ensures the iterator does not start or end at default indices.
    #[test]
    fn action_target_iterator_sliced() {
        let mut iter = ActionTargetIterator::new(
            PartyMember {
                id: 0,
                health: 25.0,
                damage_taken: 0.0,
                weapon: Weapon::Stick(Stick { damage: 10.0 }),
            },
            10..20,
        );

        for t in 10..20 {
            assert_eq!(
                iter.next(),
                Some((
                    Action::SimpleAttack(SimpleAttackAction {
                        weapon: Some(Weapon::Stick(Stick { damage: 10.0 })),
                        damage: 10.0
                    }),
                    t
                ))
            );
        }

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn attack_iterator_works() {
        let heroes = Party {
            id: 0,
            members: vec![
                PartyMember {
                    id: 0,
                    health: 25.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Stick(Stick { damage: 10.0 }),
                },
                PartyMember {
                    id: 1,
                    health: 25.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Fists(Fists { damage: 5.0 }),
                },
            ],
        };

        let villains = Party {
            id: 1,
            members: vec![
                PartyMember {
                    id: 0,
                    health: 25.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Stick(Stick { damage: 10.0 }),
                },
                PartyMember {
                    id: 1,
                    health: 25.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Stick(Stick { damage: 10.0 }),
                },
            ],
        };

        let mut iter = ActionIterator::new(heroes, villains);

        // First player attacks first opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction {
                action: Action::SimpleAttack(SimpleAttackAction {
                    weapon: Some(Weapon::Stick(Stick { damage: 10.0 })),
                    damage: 10.0
                }),
                source: Participant {
                    party_id: 0,
                    member_id: 0
                },
                target: Participant {
                    party_id: 1,
                    member_id: 0
                }
            })
        );

        // First player attacks second opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction {
                action: Action::SimpleAttack(SimpleAttackAction {
                    weapon: Some(Weapon::Stick(Stick { damage: 10.0 })),
                    damage: 10.0
                }),
                source: Participant {
                    party_id: 0,
                    member_id: 0
                },
                target: Participant {
                    party_id: 1,
                    member_id: 1
                }
            })
        );

        // Second player attacks first opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction {
                action: Action::SimpleAttack(SimpleAttackAction {
                    weapon: Some(Weapon::Fists(Fists { damage: 5.0 })),
                    damage: 5.0
                }),
                source: Participant {
                    party_id: 0,
                    member_id: 1
                },
                target: Participant {
                    party_id: 1,
                    member_id: 0
                }
            })
        );

        // Second player attacks second opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction {
                action: Action::SimpleAttack(SimpleAttackAction {
                    weapon: Some(Weapon::Fists(Fists { damage: 5.0 })),
                    damage: 5.0
                }),
                source: Participant {
                    party_id: 0,
                    member_id: 1
                },
                target: Participant {
                    party_id: 1,
                    member_id: 1
                }
            })
        );

        assert_eq!(iter.next(), None);
    }
}
