//! `mnnn backup ...`
use super::{BackupCommand, Ctx};
use crate::backup;

pub fn run(ctx: &Ctx, cmd: BackupCommand) -> anyhow::Result<()> {
    match cmd {
        BackupCommand::Create { out, encrypt } => {
            let dek = ctx.unlock()?;
            let store = crate::store::Store::open(&ctx.paths.db, Some(&dek.0))?;
            let pw = if encrypt { Some(ctx.export_password("Backup password")?) } else { None };
            let p = backup::create(&ctx.paths, &store, &dek, &out, pw.as_ref())?;
            ctx.io.info(format!("wrote {}", p.display()));
        }
        BackupCommand::Restore { from } => {
            let r = backup::restore(&ctx.paths, &from, &ctx.io, || {
                ctx.export_password("Backup password").map_err(|e| crate::Error::Other(e.to_string()))
            })?;
            ctx.io.info(format!("restored {} files from {}", r.files, r.archive.display()));
            if !r.themes_skipped.is_empty() {
                ctx.io.warn(format!("kept existing themes (pass -y to overwrite): {}", r.themes_skipped.join(", ")));
            }
        }
    }
    Ok(())
}
