//! MTD(f): null-window searches that close in on the root score.

use super::alphabeta::{RootResult, Searcher};
use crate::conflict::Conflict;
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
/// searcher's table carries what each search learned into the next. The
/// result is that of the last search, so its scores are mostly bounds. When
/// a search proves nothing new, which the villain-defeat shortcut can cause,
/// one full-window search on the warm table gives the result instead.
pub(crate) fn mtdf(
    searcher: &mut Searcher,
    state: &Conflict,
    depth: usize,
    guess: f32,
) -> RootResult {
    let mut lower = f32::NEG_INFINITY;
    let mut upper = f32::INFINITY;
    let mut g = guess;
    let mut passes = 0;
    loop {
        let beta = if g == lower { g + WIDTH } else { g };
        let root = searcher.root(state, depth, beta - WIDTH, beta);
        passes += 1;
        #[cfg(test)]
        {
            searcher.passes += 1;
        }
        g = root.scored.score.value();
        let Some(bounds) = narrow((lower, upper), g, root.scored.bound) else {
            return searcher.root(state, depth, f32::NEG_INFINITY, f32::INFINITY);
        };
        (lower, upper) = bounds;
        if upper - lower <= WIDTH || passes >= MAX_PASSES {
            return root;
        }
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
