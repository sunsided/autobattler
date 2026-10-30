use crate::cli::{Command, UiMode};
use crate::narration::playback::play;
use crate::narration::{BeatKind, NarrationOptions, Narrator};
use crate::roster::Roster;
use crate::scenario::{Fight, Preset};
use crate::search::Search;
use crate::solver::MAX_ROUNDS;
use crate::tui::menu::Menu;
use crate::tui::TuiError;
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
mod search;
// The old engine only serves tests until it is removed.
#[cfg_attr(not(test), allow(dead_code))]
mod solver;
mod tui;
mod utility_value;
// The old engine's value bookkeeping goes with it.
#[cfg_attr(not(test), allow(dead_code))]
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
    let search = Search {
        kind: options.search,
        depth: options.depth,
    };
    let mode = cli::ui_mode(options.plain, options.fast, stdout().is_terminal());
    if mode == UiMode::Tui {
        // The TUI draws its own colors; keep escape codes out of the beats.
        colored::control::set_override(false);
        let menu = Menu::new(options.setup, roster, seed);
        let (last, result) = tui::run(menu, search, options.explain);
        // Leaving the alternate screen clears the fight; print the result,
        // seed included, so it stays in the scrollback.
        let print_result = || {
            colored::control::unset_override();
            let Some(fight) = last else { return };
            let options = NarrationOptions {
                explain: options.explain,
                seed: Some(fight.seed),
            };
            let closing =
                Narrator::new(&fight.names, options).narrate(&fight.conflict, &fight.outcome);
            for beat in closing
                .iter()
                .filter(|b| matches!(b.kind, BeatKind::Outro | BeatKind::Stats))
            {
                for line in &beat.lines {
                    println!("{line}");
                }
            }
        };
        match result {
            Ok(()) => {
                print_result();
                return ExitCode::SUCCESS;
            }
            Err(TuiError::Terminal(error)) => {
                eprintln!("terminal error: {error}");
                print_result();
                return ExitCode::FAILURE;
            }
            Err(TuiError::Setup(error)) => {
                eprintln!("TUI unavailable ({error}); showing the text log.");
                colored::control::unset_override();
            }
        }
    }

    let Fight { conflict, names } = scenario::build(options.setup, roster, seed);
    let outcome = search::play(&conflict, search, MAX_ROUNDS);
    let narration = NarrationOptions {
        explain: options.explain,
        seed: Some(seed),
    };
    let beats = Narrator::new(&names, narration).narrate(&conflict, &outcome);
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
