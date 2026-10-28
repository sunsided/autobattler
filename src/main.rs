use crate::cli::Command;
use crate::narration::playback::play;
use crate::narration::{NarrationOptions, Narrator};
use crate::solver::{Solver, SolverStrategy};
use rnglib::{Language, RNG};
use std::io::{stdout, IsTerminal};
use std::process::ExitCode;
use std::time::Duration;

mod action;
mod action_iterator;
mod cli;
mod conflict;
mod narration;
mod party;
mod party_member;
mod rng;
mod roster;
mod scenario;
mod solver;
mod utility_value;
mod value;
mod weapon;

fn main() -> ExitCode {
    env_logger::init();

    let options = match cli::parse(std::env::args().skip(1)) {
        Ok(Command::Run(options)) => options,
        Ok(Command::Help) => {
            println!("{}", cli::USAGE);
            return ExitCode::SUCCESS;
        }
        Err(flag) => {
            eprintln!("unknown option: {flag}\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };

    let conflict = scenario::default_conflict();

    let rng = RNG::try_from(&Language::Fantasy).unwrap();
    let hero_names = rng.generate_names(conflict.initiator.len(), false);

    let rng = RNG::try_from(&Language::Demonic).unwrap();
    let villain_names = rng.generate_names(conflict.opponent.len(), false);

    let names = vec![hero_names, villain_names];

    let outcome = Solver::engage(&conflict, SolverStrategy::IterativeDeepening(10));

    let narrator = Narrator::new(
        &names,
        NarrationOptions {
            explain: options.explain,
        },
    );
    let beats = narrator.narrate(&conflict, &outcome);

    let delays = cli::delays_enabled(options.fast, stdout().is_terminal());
    let pause = |duration: Duration| {
        if delays {
            std::thread::sleep(duration);
        }
    };
    if let Err(error) = play(&beats, &mut stdout().lock(), pause) {
        eprintln!("failed to write the combat log: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
