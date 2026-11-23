//! The position key: a 128-bit hash of everything that decides how a conflict plays out,
//! which game-tree-search's transposition table stores positions under.

use crate::conflict::{Conflict, Parting};
use crate::party::Party;
use crate::party_member::{Morale, PartyMember};
use crate::weapon::Weapon;
use std::hash::{Hash, Hasher};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conflict::Parting;
    use crate::roster::Roster;
    use crate::scenario::{self, Preset, Setup};

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
}
