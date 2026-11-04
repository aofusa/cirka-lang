//! `cirka`: the CirkaLang command line driver.

use std::process::ExitCode;

const USAGE: &str = "usage: cirka <build|run|check> [options] FILE (.cirka / .core)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("CirkaLang v{}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    // The CirkaLang frontend is added by later tickets (FEAT-0200..); until
    // then only `.core` input is accepted.
    cirka_driver::cli::main("cirka", USAGE, &|_, _, _| None)
}
