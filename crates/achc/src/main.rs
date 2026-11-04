//! `achc`: the aCHC command line driver.

use std::process::ExitCode;

const USAGE: &str = "usage: achc <build|run|check> [options] FILE (.hs / .chs / .core)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("aCHC {}", env!("ACHC_COMMIT"));
        return ExitCode::SUCCESS;
    }
    // The Haskell frontend is added by later tickets (FEAT-0100..); until
    // then only `.core` input is accepted.
    cirka_driver::cli::main("achc", USAGE, &|_, _, _| None)
}
