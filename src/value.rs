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

/// How exact a search score is.
///
/// Alpha-beta pruning stops exploring a node as soon as its value can no longer
/// change the decision above it; the value reached by then is only a bound.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Bound {
    /// The node was fully explored (or evaluated); the score is exact.
    Exact,
    /// The exploration was cut short; the true score is at most this value.
    AtMost,
    /// The exploration was cut short; the true score is at least this value.
    AtLeast,
    /// The true score may lie on either side of this value.
    Approximate,
}

/// Combines what is known about a node's children into the bound of its value.
///
/// ## Arguments
/// * `is_maximizing` - Whether the node picks the highest child value.
/// * `cut` - Whether the node stopped expanding at a cutoff, leaving children unexplored.
/// * `best` - The bound of the child that set the node's value.
/// * `widened` - Whether any explored child may be better for the node than its
///   score says (a maximizer saw [`Bound::AtLeast`], a minimizer [`Bound::AtMost`],
///   either saw [`Bound::Approximate`]).
pub fn combine_bound(is_maximizing: bool, cut: bool, best: Bound, widened: bool) -> Bound {
    // The best child pins the side the node optimizes away from; the other side
    // is only safe if no child (explored or not) could beat the score.
    let (lower_ok, upper_ok) = if is_maximizing {
        (
            matches!(best, Bound::Exact | Bound::AtLeast),
            !cut && !widened,
        )
    } else {
        (
            !cut && !widened,
            matches!(best, Bound::Exact | Bound::AtMost),
        )
    };
    match (lower_ok, upper_ok) {
        (true, true) => Bound::Exact,
        (true, false) => Bound::AtLeast,
        (false, true) => Bound::AtMost,
        (false, false) => Bound::Approximate,
    }
}

/// A terminal state.
#[derive(Debug, Copy, Clone)]
pub enum TerminalState {
    /// The maximizing player wins.
    Win(f32),
    /// The maximizing player loses.
    Defeat(f32),
    /// The player remained after the other player retreated.
    Remain(f32),
    /// The player retreated.
    Retreat(f32),
    /// No clear decision can be made.
    Heuristic(f32),
    /// The branch is unexplored and has a default value.
    OpenUnexplored(f32),
}

/// A value triplet in Alpha-Beta pruning.
#[derive(Clone, PartialEq, PartialOrd)]
pub struct Value {
    /// The current value. If a branch is initialized, the value is set
    /// to [`TerminalState::Heuristic`]. This value will always be between
    /// `alpha`, the lowest guaranteed value, and `beta`, the highest
    /// possible value.
    pub value: TerminalState,
    /// The lower bound, i.e. the lowest `value` that can be reached by a
    /// minimizing player.
    ///
    /// ## Maximizing Player
    /// If a maximizing player observes a value higher than the alpha value,
    /// the alpha value will be increased accordingly and the search in the
    /// current minimizing node can be terminated.
    ///
    /// ## Minimizing Player
    /// When a minimizing player finds a new lowest value and the maximizing
    /// player is aware of another minimizing node that produces a higher value.
    /// While other child branches of this minimizing node may produce even
    /// lower values (favorable to the minimizer), the maximizing player would
    /// never pick them since it already knows a better alternative.
    pub alpha: f32,
    /// The upper bound, i.e. the highest `value` that can be reached by a
    /// maximizing player.
    ///
    /// ## Minimizing Player
    /// If a minimizing player observes a value lower than the beta value,
    /// the beta value will be decreased accordingly and the search in the
    /// current maximizing node can be terminated.
    ///
    /// ## Maximizing Player
    /// When a maximizing node finds a highest value and the minimizing
    /// player is aware of another node that produces a lower value.
    /// In other words, while other child branches of the maximizing node
    /// might yield even higher values (favorable to the maximizer), the
    /// minimizing player would never pick them because it already knows
    /// a better alternative.
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
    ///
    /// A beta cutoff happens when a maximizer node finds a value that
    /// is larger than the current beta value. Since the beta value resembles
    /// the highest value the parent minimizer node will allow (since it is
    /// aware of another maximizer node with a lower value), the search
    /// in this branch can be stopped
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
            TerminalState::Remain(value) => *value,
            TerminalState::Retreat(value) => *value,
            TerminalState::Heuristic(value) => *value,
            TerminalState::OpenUnexplored(value) => *value,
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
            TerminalState::Remain(value) => value,
            TerminalState::Retreat(value) => value,
            TerminalState::Heuristic(value) => value,
            TerminalState::OpenUnexplored(value) => value,
        }
    }
}

impl Display for TerminalState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            TerminalState::Win(value) => write!(f, "win({})", value),
            TerminalState::Defeat(value) => write!(f, "defeat({})", value),
            TerminalState::Remain(value) => write!(f, "remain({})", value),
            TerminalState::Retreat(value) => write!(f, "retreat({})", value),
            TerminalState::Heuristic(value) => write!(f, "H({})", value),
            TerminalState::OpenUnexplored(value) => write!(f, "O({})", value),
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

#[cfg(test)]
mod tests {
    use super::*;
    use Bound::*;

    #[test]
    fn combine_bound_covers_all_outcomes() {
        // (is_maximizing, cut, best, widened) => expected
        let cases = [
            // Maximizer: the best child bounds the value from below,
            // a cutoff or a widening child leaves the upper side open.
            (true, false, Exact, false, Exact),
            (true, false, AtLeast, false, Exact),
            (true, false, AtMost, false, AtMost),
            (true, false, Approximate, false, AtMost),
            (true, true, Exact, false, AtLeast),
            (true, false, Exact, true, AtLeast),
            (true, true, AtLeast, true, AtLeast),
            (true, true, AtMost, false, Approximate),
            (true, false, Approximate, true, Approximate),
            // Minimizer: mirrored.
            (false, false, Exact, false, Exact),
            (false, false, AtMost, false, Exact),
            (false, false, AtLeast, false, AtLeast),
            (false, false, Approximate, false, AtLeast),
            (false, true, Exact, false, AtMost),
            (false, false, Exact, true, AtMost),
            (false, true, AtMost, true, AtMost),
            (false, true, AtLeast, false, Approximate),
            (false, false, Approximate, true, Approximate),
        ];
        for (is_maximizing, cut, best, widened, expected) in cases {
            assert_eq!(
                combine_bound(is_maximizing, cut, best, widened),
                expected,
                "max={is_maximizing} cut={cut} best={best:?} widened={widened}"
            );
        }
    }
}
