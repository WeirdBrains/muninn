//! `mnnn export`
use super::{Ctx, ExportArgs, ExportFormat};
use crate::export::{self, Format};

pub fn run(ctx: &Ctx, a: ExportArgs) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    let f = export::filter(
        &store,
        a.source.as_deref(),
        a.symbol.as_deref(),
        a.duration.as_deref(),
        a.from.as_deref(),
        a.to.as_deref(),
    )?;
    let fmt = match a.format {
        ExportFormat::Csv => Format::Csv,
        ExportFormat::Parquet => Format::Parquet,
        ExportFormat::Json => Format::Json,
    };
    let n = export::export(&store, &f, fmt, &a.out)?;
    if a.out.as_os_str() != "-" {
        ctx.io.info(format!("wrote {n} bars to {}", a.out.display()));
    }
    Ok(())
}
