//! `achc`: the aCHC command line driver.

use std::process::ExitCode;

const USAGE: &str =
    "usage: achc <build|run|check> [options] FILE.hs | FILE.chs | DIR (with achc.toml) | FILE.core
       achc bindgen [options] HEADER.h [-o FILE.hs] (Haskell FFI imports for a C header)
  --dump=rn,tc also writes the renamed main module and the typed program
  --time-passes prints how long each stage takes";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("aCHC {}", env!("ACHC_COMMIT"));
        return ExitCode::SUCCESS;
    }
    if args.first().is_some_and(|a| a == "bindgen") {
        return cirka_bindgen::cli::main("achc", &args[1..], cirka_bindgen::Lang::Haskell);
    }
    cirka_driver::cli::main("achc", USAGE, &achc::frontend)
}
