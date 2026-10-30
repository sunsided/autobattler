//! MTD(f): null-window searches that close in on the root score.

use super::alphabeta::{RootResult, Searcher};
use crate::conflict::Conflict;

/// The most null-window searches per call.
const MAX_PASSES: usize = 64;

/// The width of a null window, and how close the bounds must come.
const WIDTH: f32 = 1e-3;

/// Searches `state` up to `depth` actions deep with null windows, starting
/// around `guess`, until the bounds on the root score meet within [`WIDTH`]
/// or [`MAX_PASSES`] searches have run.
///
/// Each search tells whether the root score lies below or above its window,
/// which lowers the upper or raises the lower bound. The searcher's table
/// carries what each search learned into the next. The result is that of
/// the last search, so its scores are mostly bounds.
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
        if g < beta {
            upper = g;
        } else {
            lower = g;
        }
        if upper - lower <= WIDTH || passes >= MAX_PASSES {
            return root;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::positions::bench_positions;
    use crate::search::SearchKind;

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
