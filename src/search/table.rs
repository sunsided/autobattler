//! The transposition table: scores of positions already searched.

use crate::conflict::Conflict;
use crate::party::Party;
use crate::party_member::Morale;
use crate::value::{Bound, TerminalState};
use crate::weapon::Weapon;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Mixed into the second hash pass so the two halves of a key differ.
const SALT: u64 = 0x9e37_79b9_7f4a_7c15;

/// A 128-bit hash of everything that decides how `state` plays out.
pub fn position_key(state: &Conflict) -> (u64, u64) {
    let mut first = DefaultHasher::new();
    hash_position(state, &mut first);
    let mut second = DefaultHasher::new();
    SALT.hash(&mut second);
    hash_position(state, &mut second);
    (first.finish(), second.finish())
}

fn hash_position(state: &Conflict, hasher: &mut impl Hasher) {
    state.round.hash(hasher);
    state.slot.hash(hasher);
    match &state.parting {
        None => 0u8.hash(hasher),
        Some(parting) => {
            1u8.hash(hasher);
            parting.fleeing_party.hash(hasher);
            parting.fleeing.hash(hasher);
            parting.resume_slot.hash(hasher);
            parting.next.hash(hasher);
        }
    }
    hash_party(&state.initiator, hasher);
    hash_party(&state.opponent, hasher);
}

fn hash_party(party: &Party, hasher: &mut impl Hasher) {
    party.id.hash(hasher);
    party.can_retreat.hash(hasher);
    party.members.len().hash(hasher);
    for member in &party.members {
        member.id.hash(hasher);
        member.health.to_bits().hash(hasher);
        member.damage_taken.to_bits().hash(hasher);
        member.fled.hash(hasher);
        member.can_act.hash(hasher);
        member.speed.hash(hasher);
        match member.morale {
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
        match &member.weapon {
            Weapon::Fists(_) => 0u8.hash(hasher),
            Weapon::Stick(_) => 1u8.hash(hasher),
        }
        member.weapon.damage().to_bits().hash(hasher);
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
    entries: HashMap<(u64, u64), Entry>,
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
            entries: HashMap::new(),
            cap: Self::CAP,
        }
    }

    /// An empty table that holds up to `cap` entries.
    #[cfg(test)]
    pub fn with_cap(cap: usize) -> Table {
        Table {
            entries: HashMap::new(),
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
