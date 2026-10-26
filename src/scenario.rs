//! The default encounter.

use crate::conflict::Conflict;
use crate::party::Party;
use crate::party_member::PartyMember;
use crate::weapon::{Fists, Stick, Weapon};

/// Builds the default conflict: one hero against two villains.
pub fn default_conflict() -> Conflict {
    let heroes = Party {
        id: 0,
        members: vec![PartyMember {
            id: 0,
            health: 20.0,
            damage_taken: 0.0,
            weapon: Weapon::Fists(Fists { damage: 10.0 }),
            can_act: true,
            speed: 6,
        }],
        can_retreat: true,
        retreated: false,
    };

    let villains = Party {
        id: 1,
        members: vec![
            PartyMember {
                id: 0,
                health: 15.0,
                damage_taken: 0.0,
                weapon: Weapon::Stick(Stick { damage: 5.0 }),
                can_act: true,
                speed: 3,
            },
            PartyMember {
                id: 1,
                health: 10.0,
                damage_taken: 0.0,
                weapon: Weapon::Fists(Fists { damage: 20.0 }),
                can_act: true,
                speed: 5,
            },
        ],
        can_retreat: true,
        retreated: false,
    };

    Conflict::new(heroes, villains)
}
