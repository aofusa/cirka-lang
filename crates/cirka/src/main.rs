//! `cirka`: the CirkaLang command line driver.

use std::process::ExitCode;

const USAGE: &str =
    "usage: cirka <build|run|check> [options] FILE.cirka | DIR (with cirka.toml) | FILE.core
  --dump=rn,tt also writes the resolved program and the elaborated TT";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("CirkaLang v{}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    cirka_driver::cli::main("cirka", USAGE, &cirka::frontend)
}
