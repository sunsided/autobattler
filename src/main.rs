use crate::cli::{Command, UiMode};
use crate::narration::playback::play;
use crate::narration::{BeatKind, NarrationOptions, Narrator};
use crate::roster::Roster;
use crate::scenario::{Fight, Preset, Setup};
use crate::solver::{Solver, SolverStrategy, MAX_ROUNDS};
use crate::tui::view::FightView;
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
mod solver;
mod tui;
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

    let mode = cli::ui_mode(options.plain, options.fast, stdout().is_terminal());
    let tui = mode == UiMode::Tui;
    if tui {
        // The TUI draws its own colors; keep escape codes out of the beats.
        colored::control::set_override(false);
        if conflict.initiator.len() + conflict.opponent.len() > 4 {
            eprintln!("thinking…");
        }
    }

    let strategy = SolverStrategy::IterativeDeepening(options.depth);
    let outcome = Solver::play(&conflict, strategy, MAX_ROUNDS);

    let narrate = |explain: bool| {
        Narrator::new(
            &names,
            NarrationOptions {
                explain,
                seed: Some(seed),
            },
        )
        .narrate(&conflict, &outcome)
    };

    if tui {
        let beats = narrate(true);
        let scenario = match options.setup {
            Setup::Preset(preset) => preset.name().to_string(),
            Setup::Custom { heroes, villains } => format!("custom {heroes}v{villains}"),
        };
        let fight = FightView {
            start: &conflict,
            outcome: &outcome,
            names: &names,
            beats: &beats,
            scenario,
            seed,
        };
        // Leaving the alternate screen clears the fight; print the result,
        // seed included, so it stays in the scrollback.
        let print_result = || {
            colored::control::unset_override();
            let closing = narrate(options.explain);
            for beat in closing
                .iter()
                .filter(|b| matches!(b.kind, BeatKind::Outro | BeatKind::Stats))
            {
                for line in &beat.lines {
                    println!("{line}");
                }
            }
        };
        match tui::run(fight, options.explain) {
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

    let beats = narrate(options.explain);
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
