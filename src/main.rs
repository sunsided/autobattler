use crate::action::{Action, AppliedAction};
use crate::party::Participant;
use crate::solver::{OutcomeType, Solver, SolverStrategy};
use colored::{ColoredString, Colorize};
use rnglib::{Language, RNG};

mod action;
mod action_iterator;
mod conflict;
// Wired into main in the next task.
#[allow(dead_code)]
mod narration;
mod party;
mod party_member;
mod scenario;
mod solver;
mod utility_value;
mod value;
mod weapon;

fn main() {
    env_logger::init();

    let conflict = scenario::default_conflict();

    let rng = RNG::try_from(&Language::Fantasy).unwrap();
    let hero_names = rng.generate_names(conflict.initiator.len(), false);

    let rng = RNG::try_from(&Language::Demonic).unwrap();
    let villain_names = rng.generate_names(conflict.opponent.len(), false);

    let names = vec![hero_names, villain_names];

    let outcome = Solver::engage(&conflict, SolverStrategy::IterativeDeepening(10));

    println!(
        "Performed {} evaluations with {} cuts at depth {} in {:?}. The encounter has {} turns.",
        outcome.evaluations,
        outcome.cuts,
        outcome.max_visited_depth,
        outcome.search_duration,
        outcome.len()
    );
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
        OutcomeType::Remain(score) => println!(
            "{} {} with a score of {}.",
            "TL;DR:".bright_white(),
            "The initiating party let the opponent flee".red(),
            score
        ),
        OutcomeType::Retreat(score) => println!(
            "{} {} with a score of {}.",
            "TL;DR:".bright_white(),
            "The initiating party retreated".red(),
            score
        ),
        OutcomeType::Unknown(score) => println!(
            "{} {}, the best hypothesis is a score of {}.",
            "TL;DR:".bright_white(),
            "Anything could happen".white(),
            score
        ),
    }

    println!("\n{}", "On the attacking side:".bright_white());
    for member in &conflict.initiator.members {
        let name = &names[conflict.initiator.id][member.id];
        println!(
            "- {}, with {} health and {}",
            name.blue(),
            member.health,
            format!("{:#?}", member.weapon).yellow()
        );
    }

    println!("\n{}", "On the defending side:".bright_white());
    for member in &conflict.opponent.members {
        let name = &names[conflict.opponent.id][member.id];
        println!(
            "- {}, with {} health and {}",
            name.purple(),
            member.health,
            format!("{:#?}", member.weapon).yellow()
        );
    }

    let initiator_party = conflict.initiator.id;
    for event in outcome.timeline {
        println!(
            "\nRound {} (discovered at step {}):",
            format!("{}", event.round).bright_white(),
            event.depth
        );

        match event.action {
            AppliedAction::Flee => println!("  the party flees"),
            AppliedAction::Targeted(action) => {
                match action.action {
                    Action::SimpleAttack(attack) => {
                        println!(
                            "  {} whacks {} with {}, dealing {} damage",
                            color_participant(initiator_party, &names, &action.source),
                            color_participant(initiator_party, &names, &action.target),
                            format!("{:?}", attack).yellow(),
                            attack.damage
                        );
                    }
                };

                let target = event.state.targeted_member(&action.target);
                if target.is_dead() {
                    println!(
                        "   ⇒ {} has {}",
                        color_participant(initiator_party, &names, &action.target,),
                        "given up on being alive".red()
                    );
                } else {
                    println!(
                        "   ⇒ {} now has {} health",
                        color_participant(initiator_party, &names, &action.target,),
                        target.health
                    );
                }
            }
        }
    }
}

fn color_participant(
    initiator_party: usize,
    names: &[Vec<String>],
    target: &Participant,
) -> ColoredString {
    if target.party_id == initiator_party {
        names[target.party_id][target.member_id].blue()
    } else {
        names[target.party_id][target.member_id].purple()
    }
}
