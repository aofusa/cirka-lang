//! `achc`: the aCHC command line driver.

use std::process::ExitCode;

const USAGE: &str =
    "usage: achc <build|run|check> [options] FILE.hs | FILE.chs | DIR (with achc.toml) | FILE.core
  --dump=rn,tc also writes the renamed main module and the typed program";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("aCHC {}", env!("ACHC_COMMIT"));
        return ExitCode::SUCCESS;
    }
    cirka_driver::cli::main("achc", USAGE, &achc::frontend)
}
