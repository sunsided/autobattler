//! Builds parties and names from a seed.

use crate::party::Party;
use crate::party_member::PartyMember;
use crate::rng::XorShift64;
use crate::weapon::{Fists, Stick, Weapon};

/// How member stats are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Roster {
    /// Every stat is rolled within fixed ranges.
    Random,
    /// Every member is one of the side's archetypes.
    Archetypes,
}

/// Which side of the fight a party is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Heroes,
    Villains,
}

/// A fixed set of stats.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Archetype {
    health: f32,
    fists: bool,
    damage: f32,
    speed: u32,
}

/// Brawler, Fencer, Scout.
const HERO_ARCHETYPES: [Archetype; 3] = [
    Archetype {
        health: 20.0,
        fists: true,
        damage: 10.0,
        speed: 6,
    },
    Archetype {
        health: 16.0,
        fists: false,
        damage: 8.0,
        speed: 7,
    },
    Archetype {
        health: 12.0,
        fists: false,
        damage: 5.0,
        speed: 9,
    },
];

/// Thug, Brute, Lurker.
const VILLAIN_ARCHETYPES: [Archetype; 3] = [
    Archetype {
        health: 15.0,
        fists: false,
        damage: 6.0,
        speed: 4,
    },
    Archetype {
        health: 25.0,
        fists: true,
        damage: 15.0,
        speed: 2,
    },
    Archetype {
        health: 10.0,
        fists: true,
        damage: 8.0,
        speed: 8,
    },
];

const HERO_PREFIXES: [&str; 20] = [
    "Al", "Bra", "Cor", "Dar", "El", "Fen", "Gar", "Hal", "Ior", "Ker", "Lun", "Mer", "Nor", "Os",
    "Per", "Ran", "Sil", "Tor", "Ul", "Wen",
];
const HERO_SUFFIXES: [&str; 10] = [
    "ric", "dor", "wyn", "mar", "ion", "ald", "eth", "in", "or", "us",
];
const VILLAIN_PREFIXES: [&str; 14] = [
    "Ghor", "Zu", "Mol", "Skar", "Vex", "Kra", "Mor", "Thrak", "Grim", "Uz", "Bal", "Nar", "Xul",
    "Drak",
];
const VILLAIN_SUFFIXES: [&str; 10] = [
    "gash", "ion", "phige", "rok", "zul", "nak", "goth", "mul", "ix", "tha",
];

/// Builds a party of `size` members for `side`.
pub fn party(id: usize, side: Side, size: usize, roster: Roster, rng: &mut XorShift64) -> Party {
    let archetypes = match side {
        Side::Heroes => &HERO_ARCHETYPES,
        Side::Villains => &VILLAIN_ARCHETYPES,
    };
    let members = (0..size)
        .map(|id| {
            let stats = match roster {
                Roster::Random => random_stats(rng),
                Roster::Archetypes => *rng.pick(archetypes),
            };
            member(id, stats)
        })
        .collect();
    Party {
        id,
        members,
        can_retreat: true,
        retreated: false,
    }
}

/// Rolls whole-number stats. The draw order is part of the seed contract.
fn random_stats(rng: &mut XorShift64) -> Archetype {
    let health = rng.range(10, 25) as f32;
    let fists = rng.range(0, 1) == 0;
    let damage = if fists {
        rng.range(5, 20)
    } else {
        rng.range(4, 12)
    } as f32;
    let speed = rng.range(1, 9);
    Archetype {
        health,
        fists,
        damage,
        speed,
    }
}

fn member(id: usize, stats: Archetype) -> PartyMember {
    let weapon = if stats.fists {
        Weapon::Fists(Fists {
            damage: stats.damage,
        })
    } else {
        Weapon::Stick(Stick {
            damage: stats.damage,
        })
    };
    PartyMember {
        id,
        health: stats.health,
        damage_taken: 0.0,
        weapon,
        can_act: true,
        speed: stats.speed,
    }
}

/// Builds display names, `names[0]` for the heroes and `names[1]` for the
/// villains. No two names in the fight are the same.
pub fn names(heroes: usize, villains: usize, rng: &mut XorShift64) -> Vec<Vec<String>> {
    let mut taken = Vec::new();
    let mut side = |count: usize, prefixes: &[&str], suffixes: &[&str]| -> Vec<String> {
        (0..count)
            .map(|_| loop {
                let name = format!("{}{}", rng.pick(prefixes), rng.pick(suffixes));
                if !taken.contains(&name) {
                    taken.push(name.clone());
                    break name;
                }
            })
            .collect()
    };
    let heroes = side(heroes, &HERO_PREFIXES, &HERO_SUFFIXES);
    let villains = side(villains, &VILLAIN_PREFIXES, &VILLAIN_SUFFIXES);
    vec![heroes, villains]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(member: &PartyMember) -> (f32, bool, f32, u32) {
        (
            member.health,
            matches!(member.weapon, Weapon::Fists(_)),
            member.weapon.damage(),
            member.speed,
        )
    }

    #[test]
    fn party_shape() {
        let mut rng = XorShift64::new(1);
        for roster in [Roster::Random, Roster::Archetypes] {
            let party = party(1, Side::Villains, 4, roster, &mut rng);
            assert_eq!(party.id, 1);
            let ids: Vec<usize> = party.members.iter().map(|m| m.id).collect();
            assert_eq!(ids, vec![0, 1, 2, 3]);
            assert!(party.can_retreat);
            assert!(!party.retreated);
            assert!(party
                .members
                .iter()
                .all(|m| m.can_act && m.damage_taken == 0.0));
        }
    }

    #[test]
    fn random_stats_stay_in_range() {
        for seed in 0..200 {
            let mut rng = XorShift64::new(seed);
            for member in party(0, Side::Heroes, 4, Roster::Random, &mut rng).members {
                let (health, fists, damage, speed) = stats(&member);
                assert!((10.0..=25.0).contains(&health));
                assert_eq!(health.fract(), 0.0);
                if fists {
                    assert!((5.0..=20.0).contains(&damage));
                } else {
                    assert!((4.0..=12.0).contains(&damage));
                }
                assert_eq!(damage.fract(), 0.0);
                assert!((1..=9).contains(&speed));
            }
        }
    }

    #[test]
    fn archetype_members_match_their_side() {
        for (side, archetypes) in [
            (Side::Heroes, HERO_ARCHETYPES),
            (Side::Villains, VILLAIN_ARCHETYPES),
        ] {
            let expected: Vec<_> = archetypes
                .iter()
                .map(|a| (a.health, a.fists, a.damage, a.speed))
                .collect();
            let mut rng = XorShift64::new(3);
            for member in party(0, side, 4, Roster::Archetypes, &mut rng).members {
                assert!(expected.contains(&stats(&member)));
            }
        }
    }

    #[test]
    fn same_seed_same_party() {
        let build = || party(0, Side::Heroes, 3, Roster::Random, &mut XorShift64::new(9));
        assert_eq!(format!("{:?}", build()), format!("{:?}", build()));
    }

    /// Pins the roster: changing it changes every seeded fight.
    #[test]
    fn random_hero_for_seed_42_is_stable() {
        let mut rng = XorShift64::new(42);
        let party = party(0, Side::Heroes, 1, Roster::Random, &mut rng);
        assert_eq!(stats(&party.members[0]), (18.0, true, 15.0, 3));
    }

    #[test]
    fn names_are_unique_and_counted() {
        for seed in 0..200 {
            let names = names(4, 4, &mut XorShift64::new(seed));
            assert_eq!(names[0].len(), 4);
            assert_eq!(names[1].len(), 4);
            let mut all: Vec<&String> = names.iter().flatten().collect();
            all.sort();
            all.dedup();
            assert_eq!(all.len(), 8);
        }
    }

    #[test]
    fn names_follow_the_seed() {
        assert_eq!(
            names(2, 3, &mut XorShift64::new(5)),
            names(2, 3, &mut XorShift64::new(5))
        );
    }
}
