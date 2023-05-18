use crate::party::Participant;
use crate::weapon::Weapon;
use std::fmt::{Debug, Formatter};

/// An applied action.
#[derive(Debug, Clone, PartialEq)]
pub struct AppliedAction {
    /// The action.
    pub action: Action,
    /// The source of the action.
    pub source: Participant,
    /// The target of the action.
    pub target: Participant,
}

/// An action to be taken.
#[derive(Clone, PartialEq)]
pub enum Action {
    /// Performs a simple attack.
    SimpleAttack(SimpleAttackAction),
}

/// A simple attack.
#[derive(Clone, PartialEq)]
pub struct SimpleAttackAction {
    /// The weapon used for the attack.
    pub weapon: Option<Weapon>,
    /// The damage inflicted on the selected target.
    pub damage: f32,
}

impl Debug for Action {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Action::SimpleAttack(attack) => write!(f, "attack with {:?}", attack),
        }
    }
}

impl Debug for SimpleAttackAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.weapon {
            None => write!(f, "fists"),
            Some(ref weapon) => write!(f, "{:?}", weapon),
        }
    }
}
