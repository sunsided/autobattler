use std::fmt::{Debug, Formatter};

/// A weapon to be used by someone.
#[derive(Clone)]
pub enum Weapon {
    /// What is brown and sticky?
    Stick(Stick),
}

/// A simple stick. Not very effective.
#[derive(Debug, Clone)]
pub struct Stick {
    /// The amount of damage dealt on a successful hit.
    pub damage: f32,
}

impl Debug for Weapon {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Weapon::Stick(_) => write!(f, "a stick"),
        }
    }
}
