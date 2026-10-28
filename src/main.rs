use crate::cli::Command;
use crate::narration::playback::play;
use crate::narration::{NarrationOptions, Narrator};
use crate::roster::Roster;
use crate::scenario::{Fight, Preset};
use crate::solver::{Solver, SolverStrategy, MAX_ROUNDS};
use std::io::{stdout, IsTerminal};
use std::process::ExitCode;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
        Ok(Command::ListScenarios) => {
            for preset in Preset::ALL {
                println!("{:<10} {}", preset.name(), preset.description());
            }
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}\n\n{}", cli::USAGE);
            return ExitCode::from(2);
        }
    };

    let seed = options.seed.unwrap_or_else(clock_seed);
    let roster = options.roster.unwrap_or(Roster::Random);
    let Fight { conflict, names } = scenario::build(options.setup, roster, seed);

    let strategy = SolverStrategy::IterativeDeepening(options.depth);
    let outcome = Solver::play(&conflict, strategy, MAX_ROUNDS);

    let narrator = Narrator::new(
        &names,
        NarrationOptions {
            explain: options.explain,
            seed: Some(seed),
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

/// A seed for runs without `--seed`, printed in the stats line for replay.
fn clock_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64)
}
