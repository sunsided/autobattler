//! The transposition table: scores of positions already searched.

use crate::conflict::{Conflict, Parting};
use crate::party::Party;
use crate::party_member::{Morale, PartyMember};
use crate::value::{Bound, TerminalState};
use crate::weapon::Weapon;
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};

/// Multipliers of the two halves of a key; odd, so no input bits are lost.
const MULTIPLIERS: (u64, u64) = (0x9e37_79b9_7f4a_7c15, 0xc2b2_ae3d_27d4_eb4f);

/// Starting values of the two halves of a key, so they differ from the first
/// field on.
const SEEDS: (u64, u64) = (0x243f_6a88_85a3_08d3, 0x1319_8a2e_0370_7344);

/// A 128-bit hash of everything that decides how `state` plays out.
pub fn position_key(state: &Conflict) -> (u64, u64) {
    let mut mixer = Mixer {
        first: SEEDS.0,
        second: SEEDS.1,
    };
    hash_position(state, &mut mixer);
    mixer.finish_pair()
}

/// Folds every field into two independent multiply-xorshift accumulators.
struct Mixer {
    first: u64,
    second: u64,
}

impl Mixer {
    fn add(&mut self, value: u64) {
        self.first = (self.first ^ value).wrapping_mul(MULTIPLIERS.0);
        self.first ^= self.first >> 29;
        self.second = (self.second ^ value).wrapping_mul(MULTIPLIERS.1);
        self.second ^= self.second >> 32;
    }

    fn finish_pair(&self) -> (u64, u64) {
        (splitmix(self.first), splitmix(self.second))
    }
}

/// The finalizer of splitmix64: every input bit affects every output bit.
fn splitmix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

impl Hasher for Mixer {
    fn finish(&self) -> u64 {
        splitmix(self.first)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.add(u64::from(byte));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }
}

/// Takes the first `u64` written as the hash: keys are already mixed.
#[derive(Default)]
struct PassThrough {
    hash: u64,
    set: bool,
}

impl Hasher for PassThrough {
    fn finish(&self) -> u64 {
        self.hash
    }

    fn write(&mut self, _bytes: &[u8]) {
        unreachable!("keys are pairs of u64");
    }

    fn write_u64(&mut self, value: u64) {
        if !self.set {
            self.hash = value;
            self.set = true;
        }
    }
}

fn hash_position(state: &Conflict, hasher: &mut impl Hasher) {
    // Exhaustive, so a new field is a compile error until it is hashed.
    let Conflict {
        initiator,
        opponent,
        round,
        slot,
        parting,
    } = state;
    round.hash(hasher);
    slot.hash(hasher);
    match parting {
        None => 0u8.hash(hasher),
        Some(Parting {
            fleeing_party,
            fleeing,
            resume_slot,
            next,
        }) => {
            1u8.hash(hasher);
            fleeing_party.hash(hasher);
            fleeing.hash(hasher);
            resume_slot.hash(hasher);
            next.hash(hasher);
        }
    }
    hash_party(initiator, hasher);
    hash_party(opponent, hasher);
}

fn hash_party(party: &Party, hasher: &mut impl Hasher) {
    let Party {
        id,
        members,
        can_retreat,
    } = party;
    id.hash(hasher);
    can_retreat.hash(hasher);
    members.len().hash(hasher);
    for member in members {
        let PartyMember {
            id,
            health,
            damage_taken,
            weapon,
            can_act,
            speed,
            morale,
            fled,
        } = member;
        id.hash(hasher);
        health.to_bits().hash(hasher);
        damage_taken.to_bits().hash(hasher);
        fled.hash(hasher);
        can_act.hash(hasher);
        speed.hash(hasher);
        match morale {
            Morale::Tactical => 0u8.hash(hasher),
            Morale::Never => 1u8.hash(hasher),
            Morale::Courage(threshold) => {
                2u8.hash(hasher);
                threshold.to_bits().hash(hasher);
            }
            Morale::AfterRounds(rounds) => {
                3u8.hash(hasher);
                rounds.hash(hasher);
            }
        }
        match weapon {
            Weapon::Fists(_) => 0u8.hash(hasher),
            Weapon::Stick(_) => 1u8.hash(hasher),
        }
        weapon.damage().to_bits().hash(hasher);
    }
}

/// What a search found out about a position.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The actions that were left to search below the position.
    pub depth_left: usize,
    /// The score, from the initiator's perspective.
    pub score: TerminalState,
    /// Whether [`score`](Entry::score) is exact or a bound due to pruning.
    pub bound: Bound,
    /// The index of the best action found, if any.
    pub best: Option<usize>,
    /// Whether the search below the position never ran out of depth.
    pub complete: bool,
    /// The fewest actions left at which the search below the position runs
    /// into no depth limit: one more than the deepest ply it went below.
    pub reach: usize,
}

/// Positions already searched, by [`position_key`], up to a fixed number.
#[derive(Debug)]
pub struct Table {
    entries: HashMap<(u64, u64), Entry, BuildHasherDefault<PassThrough>>,
    cap: usize,
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

impl Table {
    /// The most entries a table holds.
    pub const CAP: usize = 1_000_000;

    /// An empty table that holds up to [`CAP`](Table::CAP) entries.
    pub fn new() -> Table {
        Table {
            entries: HashMap::default(),
            cap: Self::CAP,
        }
    }

    /// An empty table that holds up to `cap` entries.
    #[cfg(test)]
    pub fn with_cap(cap: usize) -> Table {
        Table {
            entries: HashMap::default(),
            cap,
        }
    }

    /// The entry stored for `key`, if any.
    pub fn get(&self, key: (u64, u64)) -> Option<&Entry> {
        self.entries.get(&key)
    }

    /// Stores `entry` for `key`, replacing an older one; once the table is
    /// full, positions not in it yet are not stored.
    pub fn put(&mut self, key: (u64, u64), entry: Entry) {
        if self.entries.len() >= self.cap && !self.entries.contains_key(&key) {
            return;
        }
        self.entries.insert(key, entry);
    }

    /// Whether `entry`'s score holds for a search with `depth_left` actions
    /// left: it was searched as deep, or its search never ran out of depth
    /// and still would not.
    pub fn usable(entry: &Entry, depth_left: usize) -> bool {
        entry.depth_left == depth_left || (entry.complete && depth_left >= entry.reach)
    }

    /// The number of entries.
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict::Parting;
    use crate::roster::Roster;
    use crate::scenario::{self, Preset, Setup};

    fn entry(depth_left: usize, complete: bool, reach: usize) -> Entry {
        Entry {
            depth_left,
            score: TerminalState::Heuristic(1.0),
            bound: Bound::Exact,
            best: None,
            complete,
            reach,
        }
    }

    #[test]
    fn keys_differ_by_each_field() {
        let mut base = scenario::build(Setup::Preset(Preset::Ambush), Roster::Random, 1).conflict;
        base.parting = Some(Parting {
            fleeing_party: base.opponent.id,
            fleeing: vec![0],
            resume_slot: 1,
            next: 0,
        });
        let key = position_key(&base);
        assert_eq!(position_key(&base.clone()), key);

        type Change = fn(&mut Conflict);
        let changes: [(&str, Change); 7] = [
            ("health", |c| c.initiator.members[0].health -= 1.0),
            ("damage taken", |c| {
                c.initiator.members[0].damage_taken += 1.0
            }),
            ("fled", |c| c.opponent.members[0].fled = true),
            ("parting next", |c| c.parting.as_mut().unwrap().next += 1),
            ("parting fleeing", |c| {
                c.parting.as_mut().unwrap().fleeing.push(1);
            }),
            ("round", |c| c.round += 1),
            ("slot", |c| c.slot += 1),
        ];
        for (field, change) in changes {
            let mut changed = base.clone();
            change(&mut changed);
            assert_ne!(position_key(&changed), key, "{field}");
        }
    }

    #[test]
    fn table_finds_what_it_stored() {
        let mut table = Table::new();
        for i in 0..100 {
            table.put((splitmix(i), splitmix(!i)), entry(i as usize, false, 0));
        }
        assert_eq!(table.len(), 100);
        for i in 0..100 {
            let found = table.get((splitmix(i), splitmix(!i))).unwrap();
            assert_eq!(found.depth_left, i as usize);
        }
        assert!(table.get((1, 2)).is_none());
    }

    #[test]
    fn usable_rules() {
        assert!(Table::usable(&entry(4, false, 3), 4));
        assert!(!Table::usable(&entry(4, false, 3), 5));
        assert!(Table::usable(&entry(4, true, 3), 3));
        assert!(Table::usable(&entry(4, true, 3), 6));
        assert!(!Table::usable(&entry(4, true, 3), 2));
    }

    #[test]
    fn cap_stops_growth() {
        let mut table = Table::with_cap(3);
        for i in 0..5 {
            table.put((i, i), entry(1, false, 0));
        }
        assert_eq!(table.len(), 3);

        let stored = (0..5).find(|&i| table.get((i, i)).is_some()).unwrap();
        table.put((stored, stored), entry(7, false, 0));
        assert_eq!(table.len(), 3);
        assert_eq!(table.get((stored, stored)).unwrap().depth_left, 7);
    }
}
