use crate::action::Action;
use crate::conflict::Conflict;
use crate::party::{Participant, Party};
use crate::party_member::PartyMember;
use crate::solver::{OutcomeType, Solver};
use crate::weapon::{Stick, Weapon};
use colored::{ColoredString, Colorize};
use rnglib::{Language, RNG};

mod action;
mod conflict;
mod party;
mod party_member;
mod solver;
mod weapon;

fn main() {
    let heroes = Party {
        id: 0,
        members: vec![PartyMember {
            id: 0,
            health: 25.0,
            damage_taken: 0.0,
            weapon: Weapon::Stick(Stick { damage: 10.0 }),
        }],
    };

    let rng = RNG::try_from(&Language::Fantasy).unwrap();
    let hero_names = rng.generate_names(heroes.len(), false);

    let villains = Party {
        id: 1,
        members: vec![PartyMember {
            id: 0,
            health: 25.0,
            damage_taken: 0.0,
            weapon: Weapon::Stick(Stick { damage: 10.0 }),
        }],
    };

    let rng = RNG::try_from(&Language::Demonic).unwrap();
    let villain_names = rng.generate_names(villains.len(), false);

    let names = vec![hero_names, villain_names];

    let conflict = Conflict {
        turn: 0,
        initiator: heroes,
        opponent: villains,
    };

    let outcome = Solver::engage(&conflict);

    match outcome.outcome {
        OutcomeType::Win(score) => println!(
            "{} {} with a score of {}.",
            "TL;DR:".bright_white(),
            "The initiating party wins".green(),
            score
        ),
        OutcomeType::Lose(score) => println!(
            "{} {} with a score of {}.",
            "TL;DR:".bright_white(),
            "The initiating party is defeated".red(),
            score
        ),
        OutcomeType::Unknown => println!(
            "{} {}.",
            "TL;DR:".bright_white(),
            "Anything could happen".white()
        ),
    }

    println!("\n{}", "On the attacking side:".bright_white());
    for member in &conflict.initiator.members {
        let name = &names[conflict.initiator.id][member.id];
        println!("- {}, with {} health", name.blue(), member.health);
    }

    println!("\n{}", "On the defending side:".bright_white());
    for member in &conflict.opponent.members {
        let name = &names[conflict.opponent.id][member.id];
        println!("- {}, with {} health", name.purple(), member.health);
    }

    let initiator_party = conflict.initiator.id;
    for event in outcome.timeline {
        println!("\nTurn {}:", format!("{}", event.turn).bright_white());

        match event.action.action {
            Action::SimpleAttack(attack) => {
                println!(
                    "  {} whacks {} with {}, dealing {} damage",
                    color_participant(initiator_party, &names, &event.action.source),
                    color_participant(initiator_party, &names, &event.action.target),
                    format!("{:?}", attack).yellow(),
                    attack.damage
                );
            }
        }

        let target = event.state.targeted_member(&event.action.target);
        if target.is_dead() {
            println!(
                "   ⇒ {} has {}",
                color_participant(initiator_party, &names, &event.action.target,),
                "given up on being alive".red()
            );
        } else {
            println!(
                "   ⇒ {} now has {} health",
                color_participant(initiator_party, &names, &event.action.target,),
                target.health
            );
        }
    }
}

fn color_participant(
    initiator_party: usize,
    names: &Vec<Vec<String>>,
    target: &Participant,
) -> ColoredString {
    if target.party_id == initiator_party {
        names[target.party_id][target.member_id].blue()
    } else {
        names[target.party_id][target.member_id].purple()
    }
}
