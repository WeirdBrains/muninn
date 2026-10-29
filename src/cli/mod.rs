//! CLI layer: thin handlers over the library.
mod args;
mod authors;
mod backup;
mod catalog;
mod export;
mod history;
mod ingest;
mod keys;
pub mod man;
mod misc;
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

    /// Unlock the data key with the password-wrapped service key.
    pub fn unlock(&self) -> anyhow::Result<crate::crypto::Key32> {
        crate::crypto::keyring::Envelope::require(&self.paths)?;
        let pw = self.password("Muninn password")?;
        Ok(crate::crypto::keyring::unlock_with_password(&self.paths, &pw)?)
    }

    /// Open the encrypted bar store.
    pub fn open_store(&self) -> anyhow::Result<crate::store::Store> {
        let dek = self.unlock()?;
        Ok(crate::store::Store::open(&self.paths.db, Some(&dek.0))?)
    }

    /// The store, only if it opens without prompting (`MUNINN_PASSWORD` set, keys initialized).
    /// Used to record history for commands that otherwise never unlock (theme set/mode).
    pub fn store_if_unlocked(&self) -> Option<crate::store::Store> {
        std::env::var_os("MUNINN_PASSWORD")?;
        crate::crypto::keyring::Envelope::require(&self.paths).ok()?;
        self.open_store().map_err(|e| tracing::debug!("history not recorded: {e}")).ok()
    }

    /// The Muninn password: `MUNINN_PASSWORD`, else a hidden prompt.
    pub fn password(&self, question: &str) -> anyhow::Result<secrecy::SecretString> {
        if let Ok(p) = std::env::var("MUNINN_PASSWORD") {
            return Ok(p.into());
        }
        Ok(self.io.password(question, "set MUNINN_PASSWORD or run interactively")?.into())
    }

    /// Password for encrypted exports: `MUNINN_EXPORT_PASSWORD`, else `MUNINN_PASSWORD`, else a prompt.
    pub fn export_password(&self, question: &str) -> anyhow::Result<secrecy::SecretString> {
        if let Ok(p) = std::env::var("MUNINN_EXPORT_PASSWORD") {
            return Ok(p.into());
        }
        self.password(question)
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
    // Commands that never touch ~/.muninn.
    match &cli.command {
        Command::Help { topic, search } => return misc::help(topic.as_deref(), search.as_deref()),
        Command::Man { out } => return misc::man(out),
        _ => {}
    }
    let ctx = Ctx::new(cli.global)?;
    ctx.prefs()?;
    match cli.command {
        Command::Ingest(c) => ingest::run(&ctx, c),
        Command::Export(a) => export::run(&ctx, a),
        Command::Sources(c) => catalog::sources(&ctx, c),
        Command::Symbols(c) => catalog::symbols(&ctx, c),
        Command::Keys(c) => keys::run(&ctx, c),
        Command::Backup(c) => backup::run(&ctx, c),
        Command::Theme(c) => misc::theme(&ctx, c),
        Command::Author(c) => authors::run(&ctx, c),
        Command::History(c) => history::run(&ctx, c),
        Command::Tui => misc::tui(&ctx),
        Command::Help { .. } | Command::Man { .. } => Ok(()),
    }
}
