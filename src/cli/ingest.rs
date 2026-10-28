//! `mnnn ingest ...`
use super::{CommonIngestArgs, Ctx, IngestCommand};
use crate::ingest::{self, IngestOptions, IngestReport};

pub fn options(ctx: &Ctx, c: &CommonIngestArgs) -> anyhow::Result<IngestOptions> {
    Ok(IngestOptions {
        source: c.source.clone(),
        symbol: c.symbol.clone(),
        duration: c.duration.clone(),
        map: c.map.clone(),
        rejects: c.rejects.clone(),
        force: c.force,
        drop_zero_volume: ctx.prefs()?.ingest.drop_zero_volume,
    })
}

pub fn report(ctx: &Ctx, reports: &[IngestReport]) {
    for r in reports {
        ctx.io.info(r.summary());
        for w in &r.warnings {
            ctx.io.warn(w);
        }
    }
}

pub fn run(ctx: &Ctx, cmd: IngestCommand) -> anyhow::Result<()> {
    let mut store = ctx.open_store()?;
    let reports = match cmd {
        IngestCommand::Csv(a) => {
            let opts = options(ctx, &a.common)?;
            let csv = ingest::csv::CsvOptions {
                delimiter: a.delimiter,
                quote: a.quote,
                escape: a.escape,
                header: !a.no_header,
                skip_rows: a.skip_rows,
            };
            ingest::ingest_csv(&mut store, ctx.io, opts, &a.path, &csv)?
        }
        IngestCommand::Json(_) => return Err(crate::Error::NotImplemented("ingest json").into()),
        IngestCommand::Service(_) => return Err(crate::Error::NotImplemented("ingest service").into()),
    };
    report(ctx, &reports);
    Ok(())
}
