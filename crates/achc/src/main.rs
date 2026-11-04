//! `achc`: the aCHC command line driver.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("aCHC {}", env!("ACHC_COMMIT"));
        return;
    }
    eprintln!("usage: achc <build|run|check> [options] [Main.hs]");
    std::process::exit(2);
}
