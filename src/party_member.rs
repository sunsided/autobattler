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
    pub fn actions(&self) -> impl IntoIterator<Item = Action> {
        let mut actions = Vec::default();
        self.add_attack_actions(&mut actions);
        actions.into_iter()
    }

    /// Registers all possible attack actions.
    fn add_attack_actions(&self, actions: &mut Vec<Action>) {
        let damage = match self.weapon {
            Weapon::Stick(ref stick) => stick.damage,
            Weapon::Fists(ref fists) => fists.damage,
        };

        actions.push(Action::SimpleAttack(SimpleAttackAction {
            weapon: Some(self.weapon.clone()),
            damage,
        }));
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
}
