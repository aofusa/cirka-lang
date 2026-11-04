//! `cirka`: the CirkaLang command line driver.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("CirkaLang v{}", env!("CARGO_PKG_VERSION"));
        return;
    }
    eprintln!("usage: cirka <build|run|check> [options] [file.cirka]");
    std::process::exit(2);
}
