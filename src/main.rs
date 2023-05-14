use crate::conflict::Conflict;
use crate::party::Party;
use crate::party_member::PartyMember;
use crate::solver::Solver;
use crate::weapon::{Stick, Weapon};

mod action;
mod conflict;
mod party;
mod party_member;
mod solver;
mod weapon;

fn main() {
    let villains = Party {
        id: 0,
        members: vec![PartyMember {
            id: 0,
            health: 25.0,
            damage_taken: 0.0,
            weapon: Weapon::Stick(Stick { damage: 10.0 }),
        }],
    };

    let heroes = Party {
        id: 1,
        members: vec![PartyMember {
            id: 0,
            health: 25.0,
            damage_taken: 0.0,
            weapon: Weapon::Stick(Stick { damage: 10.0 }),
        }],
    };

    let conflict = Conflict {
        turn: 0,
        initiator: heroes,
        opponent: villains,
    };

    let outcome = Solver::engage(conflict);

    if outcome.win {
        println!("Initiating party wins")
    } else {
        println!("Initiating party is defeated")
    }

    println!("Actions:");
    for (turn, event) in outcome.timeline.into_iter().enumerate() {
        println!(
            "turn={} party={} action={:?}",
            turn,
            if turn & 1 == 0 { "I" } else { "E" },
            event.action
        );
        println!("State:\n{:?}", event.state);
    }
}
