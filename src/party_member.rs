use crate::action::{Action, SimpleAttackAction};
use crate::weapon::Weapon;

/// A party member.
#[derive(Debug, Clone)]
pub struct PartyMember {
    /// The ID of the party member. Must be uniformly increasing and unique within the party.
    pub id: usize,
    /// The amount of health. If health reaches zero, the member is dead.
    pub health: f32,
    /// The total amount of damage taken over the course of a conflict.
    pub damage_taken: f32,
    /// The weapon of choice.
    pub weapon: Weapon,
}

impl PartyMember {
    /// Returns `true` if the party member is dead.
    pub fn is_dead(&self) -> bool {
        self.health <= 0f32
    }

    /// Handles an action.
    ///
    /// ## Returns
    /// `true` if the action could be applied; `false` otherwise.
    pub fn handle_action(&mut self, action: &Action) -> bool {
        match action {
            Action::SimpleAttack(attack) => self.handle_simple_attack(attack),
        }
    }

    /// Handles a simple attack.
    ///
    /// ## Arguments
    /// * `damage` - The amount of damage inflicted on the party member.
    ///
    /// ## Returns
    /// `true` if the action could be applied; `false` otherwise. An attack will not be applied
    /// if the member is already dead.
    fn handle_simple_attack(&mut self, attack: &SimpleAttackAction) -> bool {
        if self.is_dead() {
            return false;
        }

        // We ensure that the health is never negative.
        self.health = (self.health - attack.damage).max(0.0);

        // We just sum up the damage taken regardless of whether
        // it would actually "fit" the health, i.e. a 100 damage on 10
        // health would still be counted 100 instead of 10.
        // This has no real reason apart from being much cooler to look at.
        self.damage_taken += attack.damage;

        return true;
    }

    /// Returns an iterator listing all possible actions the party
    /// member can take.
    pub fn actions(self) -> AttackIterator {
        AttackIterator::new(self)
    }

    /// Determines whether the current member can act..
    pub fn can_act(&self) -> bool {
        !self.is_dead()
    }

    /// Determines whether the action is applicable to this member.
    pub fn is_applicable(&self, _action: &Action) -> bool {
        !self.is_dead()
    }
}

/// An iterator for attack actions, i.e. actions targeting a single opponent.
#[derive(Debug, Clone)]
pub struct AttackIterator {
    member: PartyMember,
    index: usize,
}

impl AttackIterator {
    /// Creates a new iterator for the party member.
    fn new(member: PartyMember) -> Self {
        Self { member, index: 0 }
    }
}

/// Implements the [`AttackIterator`] as a state machine.
impl Iterator for AttackIterator {
    type Item = Action;

    fn next(&mut self) -> Option<Self::Item> {
        let state = self.index;
        match state {
            0 => {
                let damage = self.member.weapon.damage();
                let action = Action::SimpleAttack(SimpleAttackAction {
                    weapon: Some(self.member.weapon.clone()),
                    damage,
                });

                self.index += 1;
                Some(action)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon::Stick;

    #[test]
    fn a_lot_of_damage() {
        let mut member = PartyMember {
            id: 0,
            health: 100.0,
            damage_taken: 0.0,
            weapon: Weapon::Stick(Stick { damage: 0.0 }),
        };

        // Apply more damage than the subject has health.
        let damage_dealt = member.health + 100.0;
        let attack = SimpleAttackAction {
            weapon: None,
            damage: damage_dealt,
        };
        member.handle_action(&Action::SimpleAttack(attack));

        assert!(member.is_dead());

        // Health is not negative.
        assert_eq!(member.health, 0.0);
        assert_eq!(member.damage_taken, damage_dealt);
    }

    #[test]
    fn attack_iterator() {
        let member = PartyMember {
            id: 0,
            health: 100.0,
            damage_taken: 0.0,
            weapon: Weapon::Stick(Stick { damage: 0.0 }),
        };

        let mut iter = member.clone().actions();
        assert_eq!(
            iter.next(),
            Some(Action::SimpleAttack(SimpleAttackAction {
                weapon: Some(member.weapon),
                damage: 0.0
            }))
        );
        assert_eq!(iter.next(), None);
    }
}
