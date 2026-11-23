use game_tree_search::Score;
use std::cmp::Ordering;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

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

impl Eq for TerminalState {}

impl PartialOrd for TerminalState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Orders by value alone, like `==`, with `-0.0` equal to `0.0`: a retreat without damage
/// scores `-0.0`. Values are never NaN.
impl Ord for TerminalState {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.value() + 0.0).total_cmp(&(other.value() + 0.0))
    }
}

/// Lets game-tree-search compare, mirror and step scores; the kind of outcome is kept.
impl Score for TerminalState {
    fn negate(self) -> Self {
        match self {
            TerminalState::Win(value) => TerminalState::Defeat(-value),
            TerminalState::Defeat(value) => TerminalState::Win(-value),
            TerminalState::Remain(value) => TerminalState::Retreat(-value),
            TerminalState::Retreat(value) => TerminalState::Remain(-value),
            TerminalState::Heuristic(value) => TerminalState::Heuristic(-value),
            TerminalState::OpenUnexplored(value) => TerminalState::OpenUnexplored(-value),
        }
    }

    fn next_up(self) -> Self {
        self.with_value(step_up(self.value()))
    }

    fn next_down(self) -> Self {
        self.with_value(-step_up(-self.value()))
    }
}

impl TerminalState {
    /// The same kind of outcome with `value`.
    const fn with_value(self, value: f32) -> Self {
        match self {
            TerminalState::Win(_) => TerminalState::Win(value),
            TerminalState::Defeat(_) => TerminalState::Defeat(value),
            TerminalState::Remain(_) => TerminalState::Remain(value),
            TerminalState::Retreat(_) => TerminalState::Retreat(value),
            TerminalState::Heuristic(_) => TerminalState::Heuristic(value),
            TerminalState::OpenUnexplored(_) => TerminalState::OpenUnexplored(value),
        }
    }
}

/// The next `f32` above `value`, with `-0.0` taken as `0.0`; infinity stays.
fn step_up(value: f32) -> f32 {
    let value = value + 0.0;
    if value == f32::INFINITY {
        value
    } else if value == 0.0 {
        f32::from_bits(1)
    } else if value > 0.0 {
        f32::from_bits(value.to_bits() + 1)
    } else {
        f32::from_bits(value.to_bits() - 1)
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

    /// Scores order by value alone, with `-0.0` equal to `0.0`, as `==` has it.
    #[test]
    fn scores_order_by_value_with_zeros_equal() {
        let retreat = TerminalState::Retreat(-0.0);
        assert_eq!(retreat.cmp(&TerminalState::Heuristic(0.0)), Ordering::Equal);
        assert_eq!(retreat, TerminalState::Heuristic(0.0));
        assert!(TerminalState::Defeat(-1.0) < retreat);
        assert!(TerminalState::Win(2.0) > TerminalState::Heuristic(1.5));
    }

    /// Stepping keeps the kind and moves strictly past the value, through zero too;
    /// mirroring swaps the kind and reverses the order.
    #[test]
    fn steps_are_strict_and_mirrors_reverse() {
        for value in [-2.5, -0.0, 0.0, 1.0, f32::MAX] {
            let score = TerminalState::Heuristic(value);
            assert!(score.next_up() > score, "{value}");
            assert!(score.next_down() < score, "{value}");
            assert!(matches!(score.next_up(), TerminalState::Heuristic(_)));
        }

        let win = TerminalState::Win(5.0);
        assert!(matches!(win.negate(), TerminalState::Defeat(v) if v == -5.0));
        assert!(win.negate().negate() == win && win.negate() < win);
        let inf = TerminalState::Heuristic(f32::INFINITY);
        assert_eq!(inf.next_up(), inf);
    }
}
