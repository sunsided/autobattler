//! The fixed positions the search kinds are compared on.

use super::{play, Search, SearchKind};
use crate::conflict::Conflict;
use crate::roster::Roster;
use crate::scenario::{self, Preset, Setup};
use crate::solver::MAX_ROUNDS;
use std::iter;

/// The most positions taken from a single fight.
const MAX_PER_FIGHT: usize = 50;

/// A position to search, with a label naming where it came from.
pub struct BenchPosition {
    /// The scenario, seed and action index, e.g. `skirmish seed 1 @ 6`.
    pub label: String,
    /// The position.
    pub conflict: Conflict,
}

/// The bench positions: the start of every preset (seed 1) and of custom
/// 2v2, 3v3 and 4v4 fights (seeds 1 to 3), then every third position along
/// the fight `basic` plays at `depth`, at most fifty per fight.
///
/// The same depth always gives the same positions.
pub fn bench_positions(depth: usize) -> Vec<BenchPosition> {
    let presets = Preset::ALL
        .into_iter()
        .map(|preset| (Setup::Preset(preset), 1));
    let customs = (2..=4).flat_map(|n| {
        (1..=3).map(move |seed| {
            let setup = Setup::Custom {
                heroes: n,
                villains: n,
            };
            (setup, seed)
        })
    });

    let search = Search {
        kind: SearchKind::Basic,
        depth,
    };
    let mut positions = Vec::new();
    for (setup, seed) in presets.chain(customs) {
        let start = scenario::build(setup, Roster::Random, seed).conflict;
        let played = play(&start, search, MAX_ROUNDS);
        let states = iter::once(start).chain(played.timeline.into_iter().map(|e| e.state));
        positions.extend(states.enumerate().step_by(3).take(MAX_PER_FIGHT).map(
            |(index, conflict)| BenchPosition {
                label: format!("{} seed {seed} @ {index}", setup.label()),
                conflict,
            },
        ));
    }
    positions
}
