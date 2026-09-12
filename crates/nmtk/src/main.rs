//! nmtk — run it, break it, learn it.
//!
//! Everything happens on this machine. nmtk opens no sockets and reads nothing but its own
//! settings file.

use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
nmtk — run proof of work, ledger models, transformers and zero-knowledge proofs on your own machine.

Usage:
  nmtk            open the program
  nmtk --help     show this text
  nmtk --version  show the version

Everything runs locally. nmtk never touches the network.
";

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--help" | "-h") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some("--version" | "-V") => {
            println!("nmtk {VERSION}");
            ExitCode::SUCCESS
        }
        Some(unknown) => {
            eprintln!("nmtk: unknown option {unknown}\n\n{USAGE}");
            ExitCode::FAILURE
        }
        None => match nmtk_tui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("nmtk: {error}");
                ExitCode::FAILURE
            }
        },
    }
}
