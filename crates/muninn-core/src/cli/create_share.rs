//! CLI helpers for create-then-share (`--create` / `--create-to`).
use super::args::CreateShareOpts;
use crate::share::CreateOpts;
use crate::store::{EntryFormat, Lang, SpaceKind};

pub fn opts(a: &CreateShareOpts) -> anyhow::Result<CreateOpts> {
    Ok(CreateOpts {
        description: a.description.clone(),
        lang: a.lang.as_deref().map(parse_lang).transpose()?,
        from: a.code_from.clone(),
        space_kind: a.space_kind.as_deref().map(|k| SpaceKind::from_arg(k).map_err(|e| anyhow::anyhow!("{e}"))).transpose()?,
        main: a.main.clone(),
        format: a.format.as_deref().map(|f| EntryFormat::parse(f).map_err(|e| anyhow::anyhow!("{e}"))).transpose()?,
        query: a.query.clone(),
    })
}

fn parse_lang(s: &str) -> anyhow::Result<Lang> {
    Lang::parse(s).ok_or_else(|| anyhow::anyhow!("unknown language {s:?}"))
}
