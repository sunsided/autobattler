//! Command-line options.

/// How to run the fight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Prints the whole log at once instead of pacing it.
    pub fast: bool,
    /// Adds tactician notes explaining each choice.
    pub explain: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            fast: false,
            explain: true,
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
}

/// The usage text, one line per flag.
pub const USAGE: &str = "\
Usage: autobattler [OPTIONS]

Options:
  --fast        print the whole log at once instead of playing it back live
  --no-explain  leave out the tactician notes
  --help        print this help";

/// Parses the arguments, without the program name.
///
/// Returns the first unknown argument as the error. `--help` anywhere wins.
pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Command, String> {
    let mut options = Options::default();
    let mut unknown = None;
    for arg in args {
        match arg.as_str() {
            "--help" => return Ok(Command::Help),
            "--fast" => options.fast = true,
            "--no-explain" => options.explain = false,
            _ => {
                unknown.get_or_insert(arg);
            }
        }
    }
    match unknown {
        Some(arg) => Err(arg),
        None => Ok(Command::Run(options)),
    }
}

/// Live pacing only makes sense when a person watches a terminal.
pub fn delays_enabled(fast: bool, stdout_is_terminal: bool) -> bool {
    !fast && stdout_is_terminal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn defaults() {
        assert!(matches!(
            parse(vec![]),
            Ok(Command::Run(Options {
                fast: false,
                explain: true
            }))
        ));
    }

    #[test]
    fn flags() {
        assert!(matches!(
            parse(args(&["--fast", "--no-explain"])),
            Ok(Command::Run(Options {
                fast: true,
                explain: false
            }))
        ));
    }

    #[test]
    fn help() {
        assert!(matches!(
            parse(args(&["--fast", "--help"])),
            Ok(Command::Help)
        ));
    }

    #[test]
    fn unknown_flag() {
        assert_eq!(parse(args(&["--slow"])).err(), Some("--slow".to_string()));
    }

    #[test]
    fn delays_only_on_terminal_without_fast() {
        assert!(delays_enabled(false, true));
        assert!(!delays_enabled(true, true));
        assert!(!delays_enabled(false, false));
    }
}
