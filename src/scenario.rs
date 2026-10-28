//! The encounters to choose from.

use crate::conflict::Conflict;
use crate::party::Party;
use crate::party_member::{Morale, PartyMember};
use crate::rng::XorShift64;
use crate::roster::{self, Roster, Side};
use crate::weapon::{Fists, Stick, Weapon};

/// A named encounter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Ambush,
    Duel,
    Skirmish,
    Brawl,
}

impl Preset {
    /// Every preset, in the order `--list-scenarios` shows them.
    pub const ALL: [Preset; 4] = [
        Preset::Ambush,
        Preset::Duel,
        Preset::Skirmish,
        Preset::Brawl,
    ];

    /// The name used on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Ambush => "ambush",
            Preset::Duel => "duel",
            Preset::Skirmish => "skirmish",
            Preset::Brawl => "brawl",
        }
    }

    /// Looks a preset up by its command-line name.
    pub fn from_name(name: &str) -> Option<Preset> {
        Self::ALL.into_iter().find(|preset| preset.name() == name)
    }

    /// One line for `--list-scenarios`.
    pub fn description(self) -> &'static str {
        match self {
            Preset::Ambush => "1 hero vs 2 villains with fixed stats (default)",
            Preset::Duel => "1 hero vs 1 villain",
            Preset::Skirmish => "2 heroes vs 3 villains",
            Preset::Brawl => "3 heroes vs 4 villains",
        }
    }

    /// Party sizes for generated presets; `None` for the fixed ambush.
    fn sizes(self) -> Option<(usize, usize)> {
        match self {
            Preset::Ambush => None,
            Preset::Duel => Some((1, 1)),
            Preset::Skirmish => Some((2, 3)),
            Preset::Brawl => Some((3, 4)),
        }
    }
}

/// Which encounter to fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setup {
    /// A named preset.
    Preset(Preset),
    /// Generated parties of the given sizes.
    Custom { heroes: usize, villains: usize },
}

impl Setup {
    /// The scenario name shown to the player: the preset name or `custom {h}v{v}`.
    pub fn label(self) -> String {
        match self {
            Setup::Preset(preset) => preset.name().to_string(),
            Setup::Custom { heroes, villains } => format!("custom {heroes}v{villains}"),
        }
    }
}

/// A ready-to-solve encounter with display names.
pub struct Fight {
    pub conflict: Conflict,
    /// `names[party_id][member_id]`.
    pub names: Vec<Vec<String>>,
}

/// Builds the encounter. The same arguments always build the same fight.
///
/// Draws from the seed in a fixed order: hero stats, villain stats, names.
pub fn build(setup: Setup, roster: Roster, seed: u64) -> Fight {
    let mut rng = XorShift64::new(seed);
    let sizes = match setup {
        Setup::Preset(preset) => preset.sizes(),
        Setup::Custom { heroes, villains } => Some((heroes, villains)),
    };
    let conflict = match sizes {
        None => default_conflict(),
        Some((heroes, villains)) => {
            let heroes = roster::party(0, Side::Heroes, heroes, roster, &mut rng);
            let villains = roster::party(1, Side::Villains, villains, roster, &mut rng);
            Conflict::new(heroes, villains)
        }
    };
    let names = roster::names(conflict.initiator.len(), conflict.opponent.len(), &mut rng);
    Fight { conflict, names }
}

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
            morale: Morale::Tactical,
            fled: false,
        }],
        can_retreat: true,
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
                morale: Morale::Tactical,
                fled: false,
            },
            PartyMember {
                id: 1,
                health: 10.0,
                damage_taken: 0.0,
                weapon: Weapon::Fists(Fists { damage: 20.0 }),
                can_act: true,
                speed: 5,
                morale: Morale::Tactical,
                fled: false,
            },
        ],
        can_retreat: true,
    };

    Conflict::new(heroes, villains)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes(fight: &Fight) -> (usize, usize) {
        (
            fight.conflict.initiator.len(),
            fight.conflict.opponent.len(),
        )
    }

    #[test]
    fn label_names_presets_and_custom_sizes() {
        assert_eq!(Setup::Preset(Preset::Ambush).label(), "ambush");
        assert_eq!(Setup::Preset(Preset::Brawl).label(), "brawl");
        let custom = Setup::Custom {
            heroes: 2,
            villains: 4,
        };
        assert_eq!(custom.label(), "custom 2v4");
    }

    #[test]
    fn names_round_trip() {
        for preset in Preset::ALL {
            assert_eq!(Preset::from_name(preset.name()), Some(preset));
        }
        assert_eq!(Preset::from_name("picnic"), None);
    }

    #[test]
    fn ambush_keeps_the_fixed_stats() {
        let fight = build(Setup::Preset(Preset::Ambush), Roster::Random, 1);
        assert_eq!(
            format!("{:?}", fight.conflict),
            format!("{:?}", default_conflict())
        );
        assert_eq!(fight.names[0].len(), 1);
        assert_eq!(fight.names[1].len(), 2);
    }

    #[test]
    fn presets_and_custom_sizes() {
        let cases = [
            (Setup::Preset(Preset::Duel), (1, 1)),
            (Setup::Preset(Preset::Skirmish), (2, 3)),
            (Setup::Preset(Preset::Brawl), (3, 4)),
            (
                Setup::Custom {
                    heroes: 4,
                    villains: 2,
                },
                (4, 2),
            ),
        ];
        for (setup, expected) in cases {
            let fight = build(setup, Roster::Archetypes, 7);
            assert_eq!(sizes(&fight), expected);
            assert_eq!((fight.names[0].len(), fight.names[1].len()), expected);
        }
    }

    #[test]
    fn same_seed_same_fight() {
        let fight = || build(Setup::Preset(Preset::Brawl), Roster::Random, 42);
        let (a, b) = (fight(), fight());
        assert_eq!(format!("{:?}", a.conflict), format!("{:?}", b.conflict));
        assert_eq!(a.names, b.names);
    }
}
