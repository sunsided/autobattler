use std::cmp::Ordering;
use std::fmt::{Debug, Display, Formatter};
use std::ops::{Deref, DerefMut};

/// Describes a cutoff event.
pub enum Cutoff {
    /// No value was cut off.
    None,
    /// An alpha cutoff was observed.
    Alpha,
    /// A beta cutoff was observed.
    Beta,
}

/// A terminal state.
#[derive(Debug, Copy, Clone)]
pub enum TerminalState {
    /// The maximizing player wins.
    Win(f32),
    /// The maximizing player loses.
    Defeat(f32),
    /// No clear decision can be made.
    Heuristic(f32),
}

/// A value triplet in Alpha-Beta pruning.
#[derive(Clone, PartialEq, PartialOrd)]
pub struct Value {
    /// The lowest value that can be reached by the player.
    ///
    /// ## Minimax - Maximizing Player
    /// If a maximizing player observes a value higher than the alpha value,
    /// the alpha value will be increased accordingly.
    pub alpha: f32,
    /// The current value.
    pub value: TerminalState,
    /// The highest value that can be reached by the player.
    ///
    /// ## Minimax - Maximizing Player
    /// If a maximizing player observes a score higher than the
    /// beta value, the search can be terminated because the value
    /// already exceeds the highest guarantee a minimizing player will make.
    ///
    /// In other words, while other branches may yield higher scores,
    /// the minimizing player would never pick them.
    pub beta: f32,
}

impl Value {
    /// Initializes a new value with default alpha and beta bounds.
    pub const fn new(value: TerminalState) -> Self {
        Self::new_with(f32::NEG_INFINITY, value, f32::INFINITY)
    }

    /// Initializes a new value with provided alpha and beta bounds.
    pub const fn new_with(alpha: f32, value: TerminalState, beta: f32) -> Self {
        Self { alpha, value, beta }
    }

    /// Creates a new instance, overwriting the alpha value.
    pub const fn with_value(&self, value: TerminalState) -> Self {
        Self::new_with(self.alpha, value, self.beta)
    }

    /// Tests whether the current score results in an alpha cutoff.
    pub fn is_alpha_cutoff(&self) -> bool {
        // TODO: We might remove the is_finite() check here.
        self.value.is_finite() && self.value.value() <= self.alpha
    }

    /// Tests whether the current score results in a beta cutoff.
    pub fn is_beta_cutoff(&self) -> bool {
        // TODO: We might remove the is_finite() check here.
        self.value.is_finite() && self.value.value() >= self.beta
    }
}

impl Deref for Value {
    type Target = TerminalState;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl DerefMut for Value {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}, {}, {}]", self.alpha, self.value, self.beta)
    }
}

impl TerminalState {
    pub const fn value(&self) -> f32 {
        match self {
            TerminalState::Win(value) => *value,
            TerminalState::Defeat(value) => *value,
            TerminalState::Heuristic(value) => *value,
        }
    }

    pub fn is_negative(&self) -> bool {
        self.value() < 0.0
    }
}

impl Deref for TerminalState {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        match self {
            TerminalState::Win(value) => value,
            TerminalState::Defeat(value) => value,
            TerminalState::Heuristic(value) => value,
        }
    }
}

impl Display for TerminalState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalState::Win(value) => write!(f, "win({})", value),
            TerminalState::Defeat(value) => write!(f, "defeat({})", value),
            TerminalState::Heuristic(value) => write!(f, "H({})", value),
        }
    }
}

impl PartialEq for TerminalState {
    fn eq(&self, other: &Self) -> bool {
        self.value().eq(&other.value())
    }
}

impl PartialOrd for TerminalState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value().partial_cmp(&other.value())
    }
}

impl PartialEq<f32> for TerminalState {
    fn eq(&self, other: &f32) -> bool {
        self.value().eq(other)
    }
}

impl PartialOrd<f32> for TerminalState {
    fn partial_cmp(&self, other: &f32) -> Option<Ordering> {
        self.value().partial_cmp(other)
    }
}

impl PartialEq<TerminalState> for f32 {
    fn eq(&self, other: &TerminalState) -> bool {
        self.eq(&other.value())
    }
}

impl From<TerminalState> for f32 {
    fn from(value: TerminalState) -> Self {
        value.value()
    }
}
