use clap::Parser;

fn main() {
    let cli = muninn::cli::Cli::parse();
    if let Err(e) = muninn::cli::run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
