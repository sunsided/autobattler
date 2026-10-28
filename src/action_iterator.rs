use crate::action::{Action, AppliedAction, TargetedAction};
use crate::party::{Participant, Party};
use crate::party_member::{AttackIterator, PartyMember};
use std::ops::Range;

/// The actions offered in a turn.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnOptions {
    /// Whether attacks are offered.
    pub attacks: bool,
    /// The IDs of the enemies that may be attacked, while alive.
    /// [`None`] means every enemy on the field.
    pub targets: Option<Vec<usize>>,
    /// Whether the actor may flee alone.
    pub flee: bool,
    /// Whether the actor may order the party to retreat.
    pub retreat: bool,
}

/// An action iterator.
///
/// The iterator produces all permutations of party member attack actions
/// targeting each opponent. Actions are generated for the first party member,
/// with the first action applied to each individual opponent, then the second
/// action applied to each opponent, etc. If all actions are exhausted for all opponents,
/// the next party member is selected and the process repeats. After the attacks,
/// [`Flee`](AppliedAction::Flee) and then [`Retreat`](AppliedAction::Retreat) follow,
/// each only if the [`TurnOptions`] offer it.
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
    /// The actions on offer.
    options: TurnOptions,
    /// Determines whether the flee action was already considered.
    tried_flee: bool,
    /// Determines whether the retreat action was already considered.
    tried_retreat: bool,
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
    /// targeting all enemy party members, with every option on.
    #[cfg(test)]
    pub fn new(current: Party, opponent: Party) -> Self {
        let range = 0..current.len();
        let options = TurnOptions {
            attacks: true,
            targets: None,
            flee: true,
            retreat: true,
        };
        Self::new_in(current, opponent, range, options)
    }

    /// Creates a new iterator selecting only a range of current party members,
    /// offering the actions allowed by `options`.
    pub fn new_in(
        current: Party,
        opponent: Party,
        current_range: Range<usize>,
        options: TurnOptions,
    ) -> Self {
        // Without attacks, the member enumeration is skipped entirely.
        let current_index = if options.attacks {
            current_range.start
        } else {
            current_range.end
        };
        Self {
            current,
            opponent,
            current_index,
            current_range,
            options,
            iter: None,
            tried_flee: false,
            tried_retreat: false,
        }
    }

    /// Determines whether `member` of the opponent may be attacked.
    fn is_target(&self, member: &PartyMember) -> bool {
        match &self.options.targets {
            None => member.on_field(),
            Some(ids) => ids.contains(&member.id) && !member.is_dead(),
        }
    }
}

impl Iterator for ActionIterator {
    type Item = AppliedAction;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If the end of the enumeration was reached, the ways out follow.
            if self.current_index >= self.current_range.end {
                if !self.tried_flee {
                    self.tried_flee = true;
                    if self.options.flee {
                        return Some(AppliedAction::Flee);
                    }
                }

                if !self.tried_retreat {
                    self.tried_retreat = true;
                    if self.options.retreat {
                        return Some(AppliedAction::Retreat);
                    }
                }

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

            match self.iter.as_mut().and_then(|i| i.next()) {
                None => {
                    // The iterator was exhausted, so we continue with the next member.
                    self.current_index += 1;
                    self.iter = None;
                }
                Some((action, target_index)) => {
                    let opponent = &self.opponent.members[target_index];

                    // TODO: Rework action generation - should only generate applicable actions to begin with.
                    if !self.is_target(opponent) || !opponent.is_applicable(&action) {
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

                    return Some(AppliedAction::Targeted(TargetedAction {
                        action,
                        source,
                        target,
                    }));
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
    use crate::party_member::Morale;
    use crate::weapon::{Fists, Stick, Weapon};

    #[test]
    fn action_target_iterator_works() {
        let mut iter = ActionTargetIterator::new(
            PartyMember {
                id: 0,
                health: 25.0,
                damage_taken: 0.0,
                weapon: Weapon::Stick(Stick { damage: 10.0 }),
                can_act: true,
                speed: 1,
                morale: Morale::Tactical,
                fled: false,
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

        for t in 0..10 {
            assert_eq!(
                iter.next(),
                Some((
                    Action::SimpleAttack(SimpleAttackAction {
                        weapon: None,
                        damage: 1.0
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
                can_act: true,
                speed: 1,
                morale: Morale::Tactical,
                fled: false,
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

        for t in 10..20 {
            assert_eq!(
                iter.next(),
                Some((
                    Action::SimpleAttack(SimpleAttackAction {
                        weapon: None,
                        damage: 1.0
                    }),
                    t
                ))
            );
        }

        assert_eq!(iter.next(), None);
    }

    fn fighter(id: usize, fled: bool) -> PartyMember {
        PartyMember {
            id,
            health: 25.0,
            damage_taken: 0.0,
            weapon: Weapon::Stick(Stick { damage: 10.0 }),
            can_act: true,
            speed: 1,
            morale: Morale::Tactical,
            fled,
        }
    }

    fn party(id: usize, members: Vec<PartyMember>) -> Party {
        Party {
            id,
            members,
            can_retreat: true,
        }
    }

    fn targets(actions: &[AppliedAction]) -> Vec<usize> {
        actions
            .iter()
            .filter_map(|a| match a {
                AppliedAction::Targeted(t) => Some(t.target.member_id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn options_order_attacks_then_flee_then_retreat() {
        let actions: Vec<_> = ActionIterator::new(
            party(0, vec![fighter(0, false)]),
            party(1, vec![fighter(0, false)]),
        )
        .collect();
        // Stick and stare against the one villain, then both ways out.
        assert_eq!(actions.len(), 4);
        assert!(matches!(actions[0], AppliedAction::Targeted(_)));
        assert!(matches!(actions[1], AppliedAction::Targeted(_)));
        assert_eq!(actions[2], AppliedAction::Flee);
        assert_eq!(actions[3], AppliedAction::Retreat);
    }

    #[test]
    fn options_can_offer_flight_alone() {
        let options = TurnOptions {
            attacks: false,
            targets: None,
            flee: true,
            retreat: false,
        };
        let actions: Vec<_> = ActionIterator::new_in(
            party(0, vec![fighter(0, false)]),
            party(1, vec![fighter(0, false)]),
            0..1,
            options,
        )
        .collect();
        assert_eq!(actions, vec![AppliedAction::Flee]);
    }

    #[test]
    fn parting_targets_only_fleeing_members() {
        let options = TurnOptions {
            attacks: true,
            targets: Some(vec![1]),
            flee: false,
            retreat: false,
        };
        // Member 1 has fled; parting blows may still reach it.
        let actions: Vec<_> = ActionIterator::new_in(
            party(0, vec![fighter(0, false)]),
            party(1, vec![fighter(0, false), fighter(1, true)]),
            0..1,
            options,
        )
        .collect();
        assert_eq!(targets(&actions), vec![1, 1]);
        assert_eq!(actions.len(), 2);
    }

    #[test]
    fn fled_members_are_not_targeted() {
        let actions: Vec<_> = ActionIterator::new(
            party(0, vec![fighter(0, false)]),
            party(1, vec![fighter(0, true), fighter(1, false)]),
        )
        .collect();
        assert_eq!(targets(&actions), vec![1, 1]);
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
                    can_act: true,
                    speed: 1,
                    morale: Morale::Tactical,
                    fled: false,
                },
                PartyMember {
                    id: 1,
                    health: 25.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Fists(Fists { damage: 5.0 }),
                    can_act: true,
                    speed: 1,
                    morale: Morale::Tactical,
                    fled: false,
                },
            ],
            can_retreat: false,
        };

        let villains = Party {
            id: 1,
            members: vec![
                PartyMember {
                    id: 0,
                    health: 25.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Stick(Stick { damage: 10.0 }),
                    can_act: true,
                    speed: 1,
                    morale: Morale::Tactical,
                    fled: false,
                },
                PartyMember {
                    id: 1,
                    health: 25.0,
                    damage_taken: 0.0,
                    weapon: Weapon::Stick(Stick { damage: 10.0 }),
                    can_act: true,
                    speed: 1,
                    morale: Morale::Tactical,
                    fled: false,
                },
            ],
            can_retreat: false,
        };

        let mut iter = ActionIterator::new(heroes, villains);

        // First player attacks first opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction::Targeted(TargetedAction {
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
            }))
        );

        // First player attacks second opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction::Targeted(TargetedAction {
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
            }))
        );

        // First player attacks first opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction::Targeted(TargetedAction {
                action: Action::SimpleAttack(SimpleAttackAction {
                    weapon: None,
                    damage: 1.0
                }),
                source: Participant {
                    party_id: 0,
                    member_id: 0
                },
                target: Participant {
                    party_id: 1,
                    member_id: 0
                }
            }))
        );

        // First player attacks second opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction::Targeted(TargetedAction {
                action: Action::SimpleAttack(SimpleAttackAction {
                    weapon: None,
                    damage: 1.0
                }),
                source: Participant {
                    party_id: 0,
                    member_id: 0
                },
                target: Participant {
                    party_id: 1,
                    member_id: 1
                }
            }))
        );

        // Second player attacks first opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction::Targeted(TargetedAction {
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
            }))
        );

        // Second player attacks second opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction::Targeted(TargetedAction {
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
            }))
        );

        // Second player attacks first opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction::Targeted(TargetedAction {
                action: Action::SimpleAttack(SimpleAttackAction {
                    weapon: None,
                    damage: 1.0
                }),
                source: Participant {
                    party_id: 0,
                    member_id: 1
                },
                target: Participant {
                    party_id: 1,
                    member_id: 0
                }
            }))
        );

        // Second player attacks second opponent.
        assert_eq!(
            iter.next(),
            Some(AppliedAction::Targeted(TargetedAction {
                action: Action::SimpleAttack(SimpleAttackAction {
                    weapon: None,
                    damage: 1.0
                }),
                source: Participant {
                    party_id: 0,
                    member_id: 1
                },
                target: Participant {
                    party_id: 1,
                    member_id: 1
                }
            }))
        );

        // The all-on test options offer both ways out after the attacks.
        assert_eq!(iter.next(), Some(AppliedAction::Flee));
        assert_eq!(iter.next(), Some(AppliedAction::Retreat));
        assert_eq!(iter.next(), None);
    }
}
