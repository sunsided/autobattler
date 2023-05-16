use std::fmt::{Debug, Formatter};
use std::ops::Deref;

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
    pub value: f32,
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
    pub const fn new(value: f32) -> Self {
        Self::new_with(f32::NEG_INFINITY, value, f32::INFINITY)
    }

    /// Initializes a new value with provided alpha and beta bounds.
    pub const fn new_with(alpha: f32, value: f32, beta: f32) -> Self {
        Self { alpha, value, beta }
    }

    /// Creates a new instance, overwriting the alpha value.
    pub const fn with_alpha(&self, alpha: f32) -> Self {
        Self::new_with(alpha, self.value, self.beta)
    }

    /// Creates a new instance, overwriting the alpha value.
    pub const fn with_value(&self, value: f32) -> Self {
        Self::new_with(self.alpha, value, self.beta)
    }

    /// Creates a new instance, overwriting the alpha value.
    pub const fn with_beta(&self, beta: f32) -> Self {
        Self::new_with(self.alpha, self.value, beta)
    }

    pub fn max(&self, other: Value) -> Value {
        todo!("this is wrong");
        let value = self.value.max(other.value);
        Self {
            alpha: self.alpha.max(value),
            value,
            beta: self.beta,
        }
    }

    pub fn min(&self, other: Value) -> Value {
        todo!("this is wrong");
        let value = self.value.min(other.value);
        Self {
            alpha: self.alpha,
            value,
            beta: self.beta.min(value),
        }
    }
}

impl Deref for Value {
    type Target = f32;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl Debug for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}, {}, {}]", self.alpha, self.value, self.beta)
    }
}
