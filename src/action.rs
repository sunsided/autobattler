use crate::weapon::Weapon;

/// An action to be taken.
#[derive(Debug, Clone)]
pub enum Action {
    /// Performs a simple attack.
    SimpleAttack(SimpleAttackAction),
}

/// A simple attack.
#[derive(Debug, Clone)]
pub struct SimpleAttackAction {
    /// The weapon used for the attack.
    pub weapon: Option<Weapon>,
    /// The damage inflicted on the selected target.
    pub damage: f32,
}
