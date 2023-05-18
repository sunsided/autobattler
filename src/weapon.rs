use std::fmt::{Debug, Formatter};

/// A weapon to be used by someone.
#[derive(Clone, PartialEq)]
pub enum Weapon {
    /// Fists it is.
    Fists(Fists),
    /// What is brown and sticky?
    Stick(Stick),
}

impl Weapon {
    /// Gets the damage of the weapon.
    pub fn damage(&self) -> f32 {
        match self {
            Weapon::Stick(ref w) => w.damage,
            Weapon::Fists(ref w) => w.damage,
        }
    }
}

/// Fists. Not very effective.
#[derive(Debug, Clone, PartialEq)]
pub struct Fists {
    /// The amount of damage dealt on a successful hit.
    pub damage: f32,
}

/// A simple stick. Not very effective.
#[derive(Debug, Clone, PartialEq)]
pub struct Stick {
    /// The amount of damage dealt on a successful hit.
    pub damage: f32,
}

impl Debug for Weapon {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Weapon::Stick(item) => write!(
                f,
                "a stick{}",
                if f.alternate() {
                    format!(" ({} damage)", item.damage)
                } else {
                    String::default()
                }
            ),
            Weapon::Fists(item) => write!(
                f,
                "their fists{}",
                if f.alternate() {
                    format!(" ({} damage)", item.damage)
                } else {
                    String::default()
                }
            ),
        }
    }
}
