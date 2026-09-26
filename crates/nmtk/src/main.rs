//! nmtk — run it, break it, learn it.
//!
//! Everything happens on this machine. nmtk opens no sockets and reads nothing but its own
//! settings file.

use std::io::IsTerminal;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
nmtk — run proof of work, ledger models, transformers and zero-knowledge proofs on your own machine.

Usage:
  nmtk            open the program (needs a terminal of at least 80x24)
  nmtk --help     show this text          (-h)
  nmtk --version  show the version        (-V)

Settings are kept in $XDG_CONFIG_HOME/nmtk/settings.toml, or ~/.config/nmtk/settings.toml.
NO_COLOR, when set, starts nmtk with colour off until you choose otherwise under s.

Everything runs locally. nmtk never touches the network.
";

/// The short form, for a mistake: what went wrong is the first line, and the fix the second.
const SHORT_USAGE: &str = "usage: nmtk [--help | --version]";

/// The exit code for a command line nmtk cannot read, as every Unix tool has it.
const USAGE_ERROR: u8 = 2;

/// What the command line asks for.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Run,
    Help,
    Version,
    /// Something nmtk does not understand, kept so it can be named back to the reader.
    Unknown(String),
}

/// Reads the arguments after the program's name.
///
/// One flag is all nmtk takes. A second argument is a mistake rather than something to ignore:
/// `nmtk --version extra` that prints the version and says nothing about `extra` hides a typo.
fn parse(mut args: impl Iterator<Item = std::ffi::OsString>) -> Command {
    let Some(first) = args.next() else { return Command::Run };
    let command = match first.to_str() {
        Some("--help" | "-h") => Command::Help,
        Some("--version" | "-V") => Command::Version,
        _ => return Command::Unknown(first.to_string_lossy().into_owned()),
    };
    match args.next() {
        Some(extra) => Command::Unknown(extra.to_string_lossy().into_owned()),
        None => command,
    }
}

fn main() -> ExitCode {
    match parse(std::env::args_os().skip(1)) {
        Command::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("nmtk {VERSION}");
            ExitCode::SUCCESS
        }
        Command::Unknown(argument) => {
            eprintln!("nmtk: unknown argument `{argument}`\n{SHORT_USAGE}\nTry `nmtk --help`.");
            ExitCode::from(USAGE_ERROR)
        }
        Command::Run => run(),
    }
}

/// Opens the program, when there is a terminal to open it on.
fn run() -> ExitCode {
    // Without this check a pipe or a redirect got a screen's worth of escape codes, or an error
    // about "no such device or address" that says nothing about what to do. nmtk is a program
    // for a person at a terminal, and says so.
    if !std::io::stdout().is_terminal() {
        eprintln!(
            "nmtk: standard output is not a terminal.\n\
             nmtk draws a full-screen program and needs to be run directly in a terminal,\n\
             not through a pipe or with its output redirected. Try `nmtk --help`."
        );
        return ExitCode::FAILURE;
    }
    match nmtk_tui::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
            // Not success: whatever asked nmtk to stop should be able to tell that it did not
            // finish on its own.
            eprintln!("nmtk: {error}; the terminal was restored and every run was stopped.");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("nmtk: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Command {
        parse(list.iter().map(std::ffi::OsString::from))
    }

    #[test]
    fn no_arguments_opens_the_program() {
        assert_eq!(args(&[]), Command::Run);
    }

    #[test]
    fn both_spellings_of_each_flag_work() {
        assert_eq!(args(&["--help"]), Command::Help);
        assert_eq!(args(&["-h"]), Command::Help);
        assert_eq!(args(&["--version"]), Command::Version);
        assert_eq!(args(&["-V"]), Command::Version);
    }

    #[test]
    fn anything_else_is_named_back() {
        assert_eq!(args(&["--verbose"]), Command::Unknown("--verbose".into()));
        assert_eq!(args(&["-v"]), Command::Unknown("-v".into()));
        assert_eq!(args(&["--version", "extra"]), Command::Unknown("extra".into()));
    }

    #[test]
    fn the_help_names_every_flag_it_accepts() {
        for flag in ["--help", "-h", "--version", "-V"] {
            assert!(USAGE.contains(flag), "{flag} is not in the help");
        }
    }
}
