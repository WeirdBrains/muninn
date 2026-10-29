//! `mnnn export`
use super::{Ctx, ExportArgs, ExportFormat};
use crate::export::{self, Format};
use crate::render::DocFormat;

pub fn run(ctx: &Ctx, a: ExportArgs) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    let f = export::filter(&store, a.source.as_deref(), a.symbol.as_deref(), a.duration.as_deref(), a.from.as_deref(), a.to.as_deref())?;
    let fmt = match a.format {
        ExportFormat::Csv => Format::Csv,
        ExportFormat::Parquet => Format::Parquet,
        ExportFormat::Json => Format::Json,
        ExportFormat::Html => return document(ctx, &store, &f, DocFormat::Html, &a),
        ExportFormat::Latex => return document(ctx, &store, &f, DocFormat::Latex, &a),
        ExportFormat::Pdf => return document(ctx, &store, &f, DocFormat::Pdf, &a),
        ExportFormat::Docx => return document(ctx, &store, &f, DocFormat::Docx, &a),
        ExportFormat::Text => return document(ctx, &store, &f, DocFormat::Text, &a),
    };
    let n = export::export_recorded(&store, &f, fmt, &a.out, "cli")?;
    if a.out.as_os_str() != "-" {
        ctx.io.info(format!("wrote {n} bars to {}", a.out.display()));
    }
    Ok(())
}

/// Bars as a document (a summary and every bar per series).
fn document(ctx: &Ctx, store: &crate::store::Store, f: &crate::store::BarFilter, fmt: DocFormat, a: &ExportArgs) -> anyhow::Result<()> {
    let doc = export::docs::bars(store, f)?;
    let n = store.count_filtered(f)?;
    if a.out.as_os_str() == "-" {
        if fmt != DocFormat::Text && fmt != DocFormat::Html && fmt != DocFormat::Latex {
            anyhow::bail!("--out - (stdout) is for text, html and latex");
        }
        print!("{}", String::from_utf8_lossy(&crate::render::render(&doc, fmt)?));
    } else {
        crate::render::write_files(&doc, fmt, &a.out)?;
        ctx.io.info(format!("wrote {n} bars to {}", a.out.display()));
    }
    let a2 = crate::store::NewAction::new("export", "cli", format!("export {n} bars to {} ({})", a.out.display(), fmt.ext()))
        .params(serde_json::json!({ "out": a.out.display().to_string(), "format": fmt.ext(), "bars": n, "from": f.from, "to": f.to }));
    store.note_action(&a2);
    Ok(())
}
