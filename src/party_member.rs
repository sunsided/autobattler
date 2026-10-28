use crate::action::{Action, SimpleAttackAction};
use crate::weapon::Weapon;

/// How readily a party member flees the conflict on their own.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)] // Courage and AfterRounds are only built from rosters in later steps.
pub enum Morale {
    /// Only flees when the party retreats as a whole.
    Tactical,
    /// Never flees, not even when the party retreats.
    Never,
    /// Flees once the remaining health fraction drops to or below the given value.
    Courage(f32),
    /// Flees after the given number of rounds.
    AfterRounds(usize),
}

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
    /// Whether the party member can currently act.
    /// A member may not be able to act e.g. if they are paralyzed
    /// or fled from the encounter.
    pub can_act: bool,
    /// The speed of the member. Faster members act earlier in a round.
    pub speed: u32,
    /// How readily the member flees on their own.
    pub morale: Morale,
    /// Whether the member has fled the conflict.
    pub fled: bool,
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

        true
    }

    /// Returns an iterator listing all possible actions the party
    /// member can take.
    pub fn actions(self) -> AttackIterator {
        AttackIterator::new(self)
    }

    /// Determines whether the current member can act.
    pub fn can_act(&self) -> bool {
        self.can_act && !self.is_dead() && !self.fled
    }

    /// Returns `true` if the member is still present in the conflict,
    /// i.e. neither dead nor fled.
    pub fn on_field(&self) -> bool {
        !self.is_dead() && !self.fled
    }

    #[allow(dead_code)] // Used by the flight logic in later steps.
    /// The health the member started with.
    pub fn max_health(&self) -> f32 {
        self.health + self.damage_taken
    }

    #[allow(dead_code)] // Used by the flight logic in later steps.
    /// Returns `true` if the member's morale breaks in the given round.
    pub fn must_flee(&self, round: usize) -> bool {
        match self.morale {
            Morale::Tactical | Morale::Never => false,
            Morale::Courage(threshold) => self.health / self.max_health() <= threshold,
            Morale::AfterRounds(rounds) => round > rounds,
        }
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
            1 => {
                let action = Action::SimpleAttack(SimpleAttackAction {
                    weapon: None,
                    damage: 1.0,
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
            can_act: true,
            speed: 1,
            morale: Morale::Tactical,
            fled: false,
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
            can_act: true,
            speed: 1,
            morale: Morale::Tactical,
            fled: false,
        };

        let mut iter = member.clone().actions();
        assert_eq!(
            iter.next(),
            Some(Action::SimpleAttack(SimpleAttackAction {
                weapon: Some(member.weapon),
                damage: 0.0
            }))
        );
        assert_eq!(
            iter.next(),
            Some(Action::SimpleAttack(SimpleAttackAction {
                weapon: None,
                damage: 1.0
            }))
        );
        assert_eq!(iter.next(), None);
    }

    fn member_with(health: f32, morale: Morale) -> PartyMember {
        PartyMember {
            id: 0,
            health,
            damage_taken: 0.0,
            weapon: Weapon::Stick(Stick { damage: 0.0 }),
            can_act: true,
            speed: 1,
            morale,
            fled: false,
        }
    }

    #[test]
    fn must_flee_by_morale() {
        for round in [1, 5, 100] {
            assert!(!member_with(20.0, Morale::Tactical).must_flee(round));
            assert!(!member_with(20.0, Morale::Never).must_flee(round));
        }

        let mut m = member_with(1.0, Morale::Courage(0.0));
        m.damage_taken = 19.0;
        assert!(!m.must_flee(1));
        assert!(member_with(20.0, Morale::Courage(1.0)).must_flee(1));

        let mut m = member_with(20.0, Morale::Courage(0.4));
        assert!(!m.must_flee(1));
        m.handle_action(&Action::SimpleAttack(SimpleAttackAction {
            weapon: None,
            damage: 12.0,
        }));
        assert_eq!(m.health, 8.0);
        assert!(m.must_flee(1));
        m.health = 9.0;
        assert!(!m.must_flee(1));

        let m = member_with(20.0, Morale::AfterRounds(2));
        assert!(!m.must_flee(1));
        assert!(!m.must_flee(2));
        assert!(m.must_flee(3));
    }

    #[test]
    fn max_health_is_health_plus_damage() {
        let mut m = member_with(20.0, Morale::Tactical);
        m.handle_action(&Action::SimpleAttack(SimpleAttackAction {
            weapon: None,
            damage: 7.0,
        }));
        assert_eq!(m.max_health(), 20.0);
    }

    #[test]
    fn fled_members_are_off_the_field() {
        let mut m = member_with(20.0, Morale::Tactical);
        assert!(m.on_field() && m.can_act());
        m.fled = true;
        assert!(!m.on_field());
        assert!(!m.can_act());
    }
}
