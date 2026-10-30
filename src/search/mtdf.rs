//! MTD(f): null-window searches that close in on the root score.

use super::alphabeta::{RootResult, Searcher};
use crate::conflict::Conflict;
use crate::solver::is_maximizing;
use crate::value::Bound;

/// The most null-window searches per call.
const MAX_PASSES: usize = 64;

/// The width of a null window, and how close the bounds must come.
const WIDTH: f32 = 1e-3;

/// Searches `state` up to `depth` actions deep with null windows, starting
/// around `guess`, until the bounds on the root score meet within [`WIDTH`]
/// or [`MAX_PASSES`] searches have run.
///
/// Each search proves the root score lies at or below or at or above what
/// it returns, which lowers the upper or raises the lower bound. The
/// searcher's table carries what each search learned into the next.
///
/// The result is that of the last search that proved the score from the
/// mover's side, the only kind whose chosen action is known to reach it; its
/// scores are mostly bounds. When there is none, when a search proves
/// nothing new (which the villain-defeat shortcut can cause) or when the
/// passes run out, one full-window search on the warm table gives the result.
pub(crate) fn mtdf(
    searcher: &mut Searcher,
    state: &Conflict,
    depth: usize,
    guess: f32,
) -> RootResult {
    let is_max = state
        .next_turn()
        .is_none_or(|turn| is_maximizing(state, &turn));
    let mut lower = f32::NEG_INFINITY;
    let mut upper = f32::INFINITY;
    let mut g = guess;
    let mut proved = None;
    for _ in 0..MAX_PASSES {
        let beta = if g == lower { g + WIDTH } else { g };
        let root = searcher.root(state, depth, beta - WIDTH, beta);
        #[cfg(test)]
        {
            searcher.passes += 1;
        }
        g = root.scored.score.value();
        let bound = root.scored.bound;
        let Some(bounds) = narrow((lower, upper), g, bound) else {
            break;
        };
        (lower, upper) = bounds;
        if proves_for_mover(is_max, bound) {
            proved = Some(root);
        }
        if upper - lower <= WIDTH {
            if let Some(root) = proved {
                return root;
            }
            break;
        }
    }
    #[cfg(test)]
    {
        searcher.fallbacks += 1;
    }
    searcher.root(state, depth, f32::NEG_INFINITY, f32::INFINITY)
}

/// Whether a root search that returned `bound` proved its score from the
/// side of the mover, so its chosen action reaches at least that score for
/// a maximizer, or at most that score for a minimizer.
fn proves_for_mover(is_max: bool, bound: Bound) -> bool {
    match bound {
        Bound::Exact => true,
        Bound::AtLeast => is_max,
        Bound::AtMost => !is_max,
        Bound::Approximate => false,
    }
}

/// The bounds on the root score after a search returned `g` with `bound`,
/// or `None` if the search proved nothing that tightens them.
fn narrow((lower, upper): (f32, f32), g: f32, bound: Bound) -> Option<(f32, f32)> {
    let narrowed = match bound {
        Bound::Exact => (g, g),
        Bound::AtLeast => (lower.max(g), upper),
        Bound::AtMost => (lower, upper.min(g)),
        Bound::Approximate => return None,
    };
    (narrowed != (lower, upper)).then_some(narrowed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::positions::bench_positions;
    use crate::search::SearchKind;

    #[test]
    fn narrow_follows_what_the_search_proved() {
        let inf = f32::INFINITY;
        let table = [
            ((-inf, inf), 2.0, Bound::Exact, Some((2.0, 2.0))),
            ((-inf, inf), 2.0, Bound::AtLeast, Some((2.0, inf))),
            ((-inf, inf), 2.0, Bound::AtMost, Some((-inf, 2.0))),
            ((3.0, inf), 2.0, Bound::AtLeast, None),
            ((-inf, 1.0), 2.0, Bound::AtMost, None),
            ((1.0, 4.0), 2.0, Bound::AtMost, Some((1.0, 2.0))),
            ((1.0, 4.0), 3.0, Bound::AtLeast, Some((3.0, 4.0))),
            ((-inf, 2.0), 2.0, Bound::AtMost, None),
            ((-inf, inf), 2.0, Bound::Approximate, None),
        ];
        for (bounds, g, bound, expected) in table {
            assert_eq!(
                narrow(bounds, g, bound),
                expected,
                "{bounds:?} {g} {bound:?}"
            );
        }
    }

    #[test]
    fn only_bounds_from_the_movers_side_prove_the_move() {
        use Bound::*;
        let table = [
            (true, Exact, true),
            (true, AtLeast, true),
            (true, AtMost, false),
            (true, Approximate, false),
            (false, Exact, true),
            (false, AtLeast, false),
            (false, AtMost, true),
            (false, Approximate, false),
        ];
        for (is_max, bound, expected) in table {
            assert_eq!(
                proves_for_mover(is_max, bound),
                expected,
                "{is_max} {bound:?}"
            );
        }
    }

    /// Guards the pass cap against regressions; the loop enforces it itself.
    #[test]
    fn mtdf_stops_after_pass_cap() {
        for p in bench_positions(6) {
            let mut searcher = Searcher::new(SearchKind::Mtdf);
            mtdf(&mut searcher, &p.conflict, 6, 0.0);
            assert!(
                searcher.passes <= MAX_PASSES,
                "{}: {} passes",
                p.label,
                searcher.passes
            );
        }
    }
}
