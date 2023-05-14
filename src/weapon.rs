/// A weapon to be used by someone.
#[derive(Debug, Clone)]
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
