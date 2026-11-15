//! `cirka`: the CirkaLang command line driver.

use std::process::ExitCode;

const USAGE: &str =
    "usage: cirka <build|run|check|test> [options] FILE.cirka | DIR (with cirka.toml) | FILE.core
  --dump=rn,tt also writes the resolved program and the elaborated TT
  test [--filter=S] runs the `test` declarations, each in its own process
       cirka bindgen [options] HEADER.h [-o FILE.cirka] (extern declarations for a C header)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("CirkaLang v{}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if args.first().is_some_and(|a| a == "bindgen") {
        return cirka_bindgen::cli::main("cirka", &args[1..], cirka_bindgen::Lang::Cirka);
    }
    cirka_driver::cli::main("cirka", USAGE, &cirka::frontend)
}
