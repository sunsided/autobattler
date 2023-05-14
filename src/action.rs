use crate::weapon::Weapon;

/// An applied action.
#[derive(Debug, Clone)]
pub struct AppliedAction {
    /// The action.
    pub action: Action,
    /// The target of the action.
    pub target: ActionTarget,
}

/// The target of an action.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ActionTarget {
    /// The ID of the targeted party.
    pub party_id: usize,
    /// The ID of the targeted member in the party.
    pub member_id: usize,
}

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
