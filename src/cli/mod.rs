//! CLI layer: thin handlers over the library.
mod args;
pub use args::*;

use crate::interact::Interact;
use crate::paths::Paths;
use crate::prefs::Preferences;

/// Everything a handler needs.
pub struct Ctx {
    pub global: GlobalArgs,
    pub paths: Paths,
    pub io: Interact,
}

impl Ctx {
    pub fn new(global: GlobalArgs) -> anyhow::Result<Self> {
        let paths = Paths::resolve(global.home.clone(), global.db.clone())?;
        paths.ensure()?;
        let io = Interact::new(global.non_interactive, global.yes, global.quiet);
        Ok(Self { global, paths, io })
    }

    pub fn prefs(&self) -> anyhow::Result<Preferences> {
        Ok(Preferences::load_or_create(&self.paths)?)
    }
}

fn init_tracing(g: &GlobalArgs) {
    let level = if g.quiet {
        "error"
    } else {
        match g.verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    };
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("MUNINN_LOG").unwrap_or_else(|_| level.into()))
        .with_writer(std::io::stderr)
        .try_init();
}

pub fn run(cli: Cli) -> anyhow::Result<()> {
    init_tracing(&cli.global);
    let ctx = Ctx::new(cli.global)?;
    ctx.prefs()?;
    match cli.command {
        Command::Ingest(_) => Err(crate::Error::NotImplemented("ingest").into()),
        Command::Export(_) => Err(crate::Error::NotImplemented("export").into()),
        Command::Sources(_) => Err(crate::Error::NotImplemented("sources").into()),
        Command::Symbols(_) => Err(crate::Error::NotImplemented("symbols").into()),
        Command::Keys(_) => Err(crate::Error::NotImplemented("keys").into()),
        Command::Backup(_) => Err(crate::Error::NotImplemented("backup").into()),
        Command::Theme(_) => Err(crate::Error::NotImplemented("theme").into()),
        Command::Tui => Err(crate::Error::NotImplemented("tui").into()),
    }
}
