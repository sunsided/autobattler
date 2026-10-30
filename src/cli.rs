//! Command-line options.

use crate::roster::Roster;
use crate::scenario::{Preset, Setup};
use crate::search::SearchKind;

/// The largest party `--heroes` and `--villains` accept.
pub const MAX_PARTY: usize = 4;
/// The deepest search `--depth` accepts.
pub const MAX_DEPTH: usize = 40;

/// How to run the fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Prints the whole log at once instead of pacing it.
    pub fast: bool,
    /// Adds tactician notes explaining each choice.
    pub explain: bool,
    /// Prints the text log instead of the TUI.
    pub plain: bool,
    /// Which encounter to fight.
    pub setup: Setup,
    /// How generated members get their stats; `None` when not given.
    pub roster: Option<Roster>,
    /// The seed for stats and names; `None` picks one from the clock.
    pub seed: Option<u64>,
    /// The search depth in combatant actions.
    pub depth: usize,
    /// Which search finds the moves.
    pub search: SearchKind,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            fast: false,
            explain: true,
            plain: false,
            setup: Setup::Preset(Preset::Ambush),
            roster: None,
            seed: None,
            depth: 10,
            search: SearchKind::Basic,
        }
    }
}

/// What the command line asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Runs the fight with the given options.
    Run(Options),
    /// Prints the usage.
    Help,
    /// Prints the presets.
    ListScenarios,
}

/// The usage text, one line per flag.
pub const USAGE: &str = "\
Usage: autobattler [OPTIONS]

Options:
  --scenario NAME        fight a preset (see --list-scenarios); default: ambush
  --heroes N             fight N generated heroes (1-4, default 2)
  --villains N           against N generated villains (1-4, default 3)
  --roster KIND          random (default) or archetypes; for generated parties
  --seed N               seed for stats and names; default: from the clock
  --depth N              search depth in actions (1-40, default 10; slow above ~16)
  --search KIND          search: basic, pv, ordered, tt, mtdf (default basic)
  --list-scenarios       list the presets
  --fast                 print the whole text log at once (no TUI, no pauses)
  --no-explain           leave out the tactician notes
  --plain                print the text log instead of the TUI
  --help                 print this help";

/// Parses the arguments, without the program name.
///
/// Values follow their flag as the next argument or after `=`. Returns the
/// first problem as the error. `--help` anywhere wins.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut options = Options::default();
    let mut error = None;
    let mut list = false;
    let mut scenario = None;
    let mut heroes = None;
    let mut villains = None;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (flag, inline) = match arg.split_once('=') {
            Some((flag, value)) => (flag.to_string(), Some(value.to_string())),
            None => (arg.clone(), None),
        };
        let mut value = || inline.clone().or_else(|| args.next());
        let result = match flag.as_str() {
            "--help" => return Ok(Command::Help),
            "--list-scenarios" => {
                list = true;
                Ok(())
            }
            "--fast" => {
                options.fast = true;
                Ok(())
            }
            "--no-explain" => {
                options.explain = false;
                Ok(())
            }
            "--plain" => {
                options.plain = true;
                Ok(())
            }
            "--scenario" => required(&flag, value()).and_then(|name| {
                Preset::from_name(&name)
                    .map(|preset| scenario = Some(preset))
                    .ok_or_else(|| format!("unknown scenario: {name} (see --list-scenarios)"))
            }),
            "--heroes" => number(&flag, value(), 1, MAX_PARTY).map(|n| heroes = Some(n)),
            "--villains" => number(&flag, value(), 1, MAX_PARTY).map(|n| villains = Some(n)),
            "--roster" => required(&flag, value()).and_then(|kind| {
                let roster = match kind.as_str() {
                    "random" => Roster::Random,
                    "archetypes" => Roster::Archetypes,
                    _ => {
                        return Err(format!(
                            "unknown roster: {kind} (expected random or archetypes)"
                        ))
                    }
                };
                options.roster = Some(roster);
                Ok(())
            }),
            "--seed" => required(&flag, value()).and_then(|text| {
                text.parse()
                    .map(|seed| options.seed = Some(seed))
                    .map_err(|_| format!("invalid value for --seed: {text}"))
            }),
            "--depth" => number(&flag, value(), 1, MAX_DEPTH).map(|n| options.depth = n),
            "--search" => required(&flag, value()).and_then(|name| {
                SearchKind::from_name(&name)
                    .map(|kind| options.search = kind)
                    .ok_or_else(|| {
                        format!("--search must be one of basic, pv, ordered, tt, mtdf, got {name}")
                    })
            }),
            _ => Err(format!("unknown option: {arg}")),
        };
        if let Err(message) = result {
            error.get_or_insert(message);
        }
    }

    if let Some(message) = error {
        return Err(message);
    }
    if list {
        return Ok(Command::ListScenarios);
    }

    let custom = heroes.is_some() || villains.is_some();
    options.setup = match (scenario, custom) {
        (Some(_), true) => {
            return Err("--scenario cannot be combined with --heroes or --villains".to_string())
        }
        (Some(preset), false) => Setup::Preset(preset),
        (None, true) => Setup::Custom {
            heroes: heroes.unwrap_or(2),
            villains: villains.unwrap_or(3),
        },
        (None, false) => Setup::Preset(Preset::Ambush),
    };
    if options.roster.is_some() && options.setup == Setup::Preset(Preset::Ambush) {
        return Err(
            "the ambush has fixed stats; use --roster with another scenario or --heroes/--villains"
                .to_string(),
        );
    }
    Ok(Command::Run(options))
}

fn required(flag: &str, value: Option<String>) -> Result<String, String> {
    value.ok_or_else(|| format!("{flag} needs a value"))
}

fn number(flag: &str, value: Option<String>, low: usize, high: usize) -> Result<usize, String> {
    let text = required(flag, value)?;
    match text.parse() {
        Ok(n) if (low..=high).contains(&n) => Ok(n),
        _ => Err(format!(
            "{flag} must be a number from {low} to {high}, got {text}"
        )),
    }
}

/// Live pacing only makes sense when a person watches a terminal.
pub fn delays_enabled(fast: bool, stdout_is_terminal: bool) -> bool {
    !fast && stdout_is_terminal
}

/// How the fight is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    /// The interactive terminal view.
    Tui,
    /// The plain text log.
    Text,
}

/// Picks the TUI only for an interactive terminal without `--plain` or `--fast`.
pub fn ui_mode(plain: bool, fast: bool, stdout_is_terminal: bool) -> UiMode {
    if stdout_is_terminal && !plain && !fast {
        UiMode::Tui
    } else {
        UiMode::Text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    fn run(list: &[&str]) -> Options {
        match parse(args(list)) {
            Ok(Command::Run(options)) => options,
            other => panic!("expected Run, got {other:?}"),
        }
    }

    fn error(list: &[&str]) -> String {
        parse(args(list)).expect_err("expected an error")
    }

    #[test]
    fn defaults() {
        assert_eq!(run(&[]), Options::default());
        assert_eq!(Options::default().setup, Setup::Preset(Preset::Ambush));
        assert_eq!(Options::default().depth, 10);
    }

    #[test]
    fn flags() {
        let options = run(&["--fast", "--no-explain"]);
        assert!(options.fast);
        assert!(!options.explain);
    }

    #[test]
    fn plain_flag() {
        assert!(!run(&[]).plain);
        assert!(run(&["--plain"]).plain);
    }

    #[test]
    fn ui_mode_truth_table() {
        for plain in [false, true] {
            for fast in [false, true] {
                for terminal in [false, true] {
                    let expected = if !plain && !fast && terminal {
                        UiMode::Tui
                    } else {
                        UiMode::Text
                    };
                    assert_eq!(
                        ui_mode(plain, fast, terminal),
                        expected,
                        "plain={plain} fast={fast} terminal={terminal}"
                    );
                }
            }
        }
    }

    #[test]
    fn help() {
        assert_eq!(parse(args(&["--fast", "--help"])), Ok(Command::Help));
        assert_eq!(parse(args(&["--bogus", "--help"])), Ok(Command::Help));
    }

    #[test]
    fn list_scenarios() {
        assert_eq!(
            parse(args(&["--list-scenarios"])),
            Ok(Command::ListScenarios)
        );
    }

    #[test]
    fn unknown_flag() {
        assert_eq!(error(&["--slow"]), "unknown option: --slow");
    }

    #[test]
    fn scenario() {
        assert_eq!(
            run(&["--scenario", "brawl"]).setup,
            Setup::Preset(Preset::Brawl)
        );
        assert_eq!(run(&["--scenario=duel"]).setup, Setup::Preset(Preset::Duel));
        assert_eq!(
            error(&["--scenario", "picnic"]),
            "unknown scenario: picnic (see --list-scenarios)"
        );
        assert_eq!(error(&["--scenario"]), "--scenario needs a value");
    }

    #[test]
    fn sizes_default_the_missing_side() {
        assert_eq!(
            run(&["--heroes", "4"]).setup,
            Setup::Custom {
                heroes: 4,
                villains: 3
            }
        );
        assert_eq!(
            run(&["--villains=1"]).setup,
            Setup::Custom {
                heroes: 2,
                villains: 1
            }
        );
    }

    #[test]
    fn sizes_are_bounded() {
        assert_eq!(
            error(&["--heroes", "5"]),
            "--heroes must be a number from 1 to 4, got 5"
        );
        assert_eq!(
            error(&["--villains", "0"]),
            "--villains must be a number from 1 to 4, got 0"
        );
        assert_eq!(
            error(&["--heroes", "many"]),
            "--heroes must be a number from 1 to 4, got many"
        );
    }

    #[test]
    fn scenario_and_sizes_conflict() {
        assert_eq!(
            error(&["--scenario", "duel", "--heroes", "2"]),
            "--scenario cannot be combined with --heroes or --villains"
        );
    }

    #[test]
    fn roster() {
        assert_eq!(
            run(&["--scenario", "duel", "--roster", "archetypes"]).roster,
            Some(Roster::Archetypes)
        );
        assert_eq!(
            run(&["--heroes", "1", "--roster=random"]).roster,
            Some(Roster::Random)
        );
        assert_eq!(
            error(&["--heroes", "1", "--roster", "elves"]),
            "unknown roster: elves (expected random or archetypes)"
        );
    }

    #[test]
    fn roster_does_not_apply_to_the_ambush() {
        let message =
            "the ambush has fixed stats; use --roster with another scenario or --heroes/--villains";
        assert_eq!(error(&["--roster", "random"]), message);
        assert_eq!(
            error(&["--scenario", "ambush", "--roster", "random"]),
            message
        );
    }

    #[test]
    fn seed() {
        assert_eq!(run(&["--seed", "42"]).seed, Some(42));
        assert_eq!(run(&["--seed=18446744073709551615"]).seed, Some(u64::MAX));
        assert_eq!(error(&["--seed", "-1"]), "invalid value for --seed: -1");
        assert_eq!(error(&["--seed"]), "--seed needs a value");
    }

    #[test]
    fn depth() {
        assert_eq!(run(&["--depth", "40"]).depth, 40);
        assert_eq!(
            error(&["--depth", "41"]),
            "--depth must be a number from 1 to 40, got 41"
        );
    }

    #[test]
    fn search() {
        assert_eq!(run(&[]).search, SearchKind::Basic);
        assert_eq!(run(&["--search", "tt"]).search, SearchKind::Tt);
        assert_eq!(run(&["--search=mtdf"]).search, SearchKind::Mtdf);
    }

    #[test]
    fn search_rejects_unknown() {
        assert_eq!(
            error(&["--search", "fast"]),
            "--search must be one of basic, pv, ordered, tt, mtdf, got fast"
        );
    }

    #[test]
    fn first_error_wins() {
        assert_eq!(
            error(&["--heroes", "9", "--bogus"]),
            "--heroes must be a number from 1 to 4, got 9"
        );
    }

    #[test]
    fn delays_only_on_terminal_without_fast() {
        assert!(delays_enabled(false, true));
        assert!(!delays_enabled(true, true));
        assert!(!delays_enabled(false, false));
    }
}
