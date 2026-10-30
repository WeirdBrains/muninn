use clap::Parser;

fn main() {
    // `mnnn … | head` closes the pipe early: end quietly instead of a panic
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info.payload().downcast_ref::<String>().map(String::as_str).unwrap_or("");
        if msg.starts_with("failed printing to stdout") && msg.contains("Broken pipe") {
            std::process::exit(141);
        }
        default(info);
    }));
    let cli = muninn::cli::Cli::parse();
    if let Err(e) = muninn::cli::run(cli) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
