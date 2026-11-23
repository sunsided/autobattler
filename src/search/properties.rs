//! Property and differential tests on random small fights: every search agrees with plain
//! minimax from game-tree-search, and every search kind agrees with the others.

use super::agreement::agree;
use super::engine::{Combat, Position};
use super::{engage, Search, SearchKind};
use crate::conflict::Conflict;
use crate::party::Party;
use crate::party_member::{Morale, PartyMember};
use crate::weapon::{Fists, Stick, Weapon};
use game_tree_search::{minimax, Algorithm, MapTable, Searcher};
use proptest::prelude::*;

/// A random combatant: health 5–30, a weapon hitting for 3–15, speed 1–9, and a morale that
/// either flees when told to or never does.
fn member() -> impl Strategy<Value = (f32, bool, f32, u32, bool)> {
    (
        5.0f32..30.0,
        any::<bool>(),
        3.0f32..15.0,
        1u32..10,
        any::<bool>(),
    )
}

/// A random fight of one or two heroes against one or two villains.
fn conflict() -> impl Strategy<Value = Conflict> {
    let side = || prop::collection::vec(member(), 1..=2);
    (side(), side(), any::<bool>(), any::<bool>()).prop_map(|(heroes, villains, hr, vr)| {
        let party = |id, members: Vec<(f32, bool, f32, u32, bool)>, can_retreat| Party {
            id,
            members: members
                .into_iter()
                .enumerate()
                .map(
                    |(index, (health, fists, damage, speed, tactical))| PartyMember {
                        id: index,
                        health: health.round(),
                        damage_taken: 0.0,
                        weapon: if fists {
                            Weapon::Fists(Fists {
                                damage: damage.round(),
                            })
                        } else {
                            Weapon::Stick(Stick {
                                damage: damage.round(),
                            })
                        },
                        can_act: true,
                        speed,
                        morale: if tactical {
                            Morale::Tactical
                        } else {
                            Morale::Never
                        },
                        fled: false,
                    },
                )
                .collect(),
            can_retreat,
        };
        Conflict::new(party(0, heroes, hr), party(1, villains, vr))
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Differential: every algorithm, with and without a table and with move ordering,
    /// returns a score whose bound admits plain minimax's value at the same depth. The
    /// villains' early stop makes some bounds loose, never wrong.
    #[test]
    fn every_search_admits_the_minimax_value(conflict in conflict(), depth in 1u32..=4) {
        let root = Position::new(conflict);
        let value = minimax(&Combat { ordered: false }, &root, depth).score;
        for ordered in [false, true] {
            let game = Combat { ordered };
            for algorithm in [Algorithm::AlphaBeta, Algorithm::Pvs, Algorithm::Mtdf] {
                let plain = Searcher::new().with_algorithm(algorithm).search(&game, &root, depth);
                prop_assert!(plain.bound.admits(plain.score, value), "{algorithm:?}, ordered {ordered}");
                let tabled = Searcher::with_table(MapTable::new(1 << 12))
                    .with_algorithm(algorithm)
                    .search(&game, &root, depth);
                prop_assert!(tabled.bound.admits(tabled.score, value), "{algorithm:?} with a table");
            }
        }
    }

    /// Every search kind of the CLI reaches the same outcome as `basic`, with overlapping
    /// score ranges, on random fights.
    #[test]
    fn every_search_kind_agrees_with_basic(conflict in conflict(), depth in 1usize..=5) {
        let basic = engage(&conflict, Search { kind: SearchKind::Basic, depth });
        for kind in SearchKind::ALL {
            let other = engage(&conflict, Search { kind, depth });
            prop_assert!(agree(&basic, &other), "{} disagrees with basic", kind.name());
        }
    }
}
