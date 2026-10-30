//! `mnnn search`: live searches.
use super::{Ctx, SearchCommand, SpecArgs};
use crate::ingest::time::{fmt_ts, parse_ts};
use crate::model::{Duration, PriceMetric, Symbol};
use crate::store::{LiveSearch, NewAction, SearchSpec, SearchTarget, Store};
use crate::tui::sidebar::SortBy;
use anyhow::{Context, anyhow};

/// Record a workspace action from the CLI.
pub(super) fn record(store: &Store, kind: &str, summary: String, params: serde_json::Value) {
    store.note_action(&NewAction::new(kind, "cli", summary).params(params));
}

impl SpecArgs {
    /// The search `base` changed by the options given.
    pub fn apply(&self, ctx: &Ctx, store: &Store, mut base: SearchSpec) -> anyhow::Result<SearchSpec> {
        if self.clear {
            base.filters = Default::default();
            base.query.clear();
        }
        if self.from_view {
            let p = ctx.prefs()?;
            base.filters = p.tui.sidebar.filters.clone();
            base.sort = p.tui.sidebar.sort;
            base.desc = p.tui.sidebar.desc;
        }
        if self.journals {
            base.target = SearchTarget::Journals;
        }
        if self.series {
            base.target = SearchTarget::Series;
        }
        if self.records || self.model.is_some() || !self.conditions.is_empty() {
            base.target = SearchTarget::Records;
        }
        if let Some(m) = &self.model {
            base.model = if m.eq_ignore_ascii_case("all") { None } else { Some(store.find_model(m)?.id) };
        }
        if !self.conditions.is_empty() {
            base.conditions = crate::designer::parse_where(&self.conditions)?;
        }
        if let Some(q) = &self.query {
            base.query = q.trim().to_string();
        }
        let f = &mut base.filters;
        if !self.sources.is_empty() {
            f.sources = self.sources.iter().map(|s| s.trim().to_string()).collect();
        }
        if !self.symbols.is_empty() {
            f.symbols = self
                .symbols
                .iter()
                .map(|s| Ok(store.canonical_symbol(&Symbol::parse(s)?)?.display()))
                .collect::<crate::Result<_>>()
                .context("--symbol")?;
        }
        if !self.periods.is_empty() {
            f.durations = self.periods.iter().map(|d| Ok(Duration::parse(d)?.to_string())).collect::<crate::Result<_>>().context("--period")?;
        }
        if !self.years.is_empty() {
            f.years = self.years.iter().copied().collect();
        }
        let ts = |s: &str, what: &str| parse_ts(s).ok_or_else(|| anyhow!("invalid {what}: `{s}` is not a date or time"));
        if let Some(a) = &self.from {
            f.from = Some(ts(a, "--from")?);
        }
        if let Some(b) = &self.to {
            f.to = Some(ts(b, "--to")?);
        }
        if let Some(m) = &self.price {
            let m = m.trim().to_lowercase();
            f.price.metric = PriceMetric::ALL
                .into_iter()
                .find(|x| x.label() == m || x.label().replace(' ', "") == m || (m == "ohlc" && *x == PriceMetric::OhlcAvg))
                .ok_or_else(|| anyhow!("invalid --price: `{m}` (open, high, low, close, typical, median, ohlc, vwap)"))?;
        }
        if self.min.is_some() {
            f.price.min = self.min;
        }
        if self.max.is_some() {
            f.price.max = self.max;
        }
        if let Some(s) = &self.sort {
            base.sort = if s == "name" { SortBy::Name } else { SortBy::Year };
        }
        if self.desc {
            base.desc = true;
        }
        if self.asc {
            base.desc = false;
        }
        Ok(base)
    }
}

fn print_results(store: &Store, spec: &SearchSpec, json: bool) -> anyhow::Result<()> {
    let found = crate::live::run(store, spec)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&found)?);
    } else {
        for f in &found {
            println!("{}", f.line());
        }
    }
    Ok(())
}

fn params(s: &LiveSearch) -> serde_json::Value {
    serde_json::json!({ "search": s.id, "name": s.name, "spec": s.spec })
}

pub fn run(ctx: &Ctx, cmd: SearchCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    match cmd {
        SearchCommand::List { json } => {
            let all = store.live_searches()?;
            if json {
                let v: Vec<_> = all.iter().map(|s| serde_json::json!({ "search": s, "matches": crate::live::count(&store, &s.spec) })).collect();
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                for s in &all {
                    println!("{}\t{}\t{}\t{}", s.short_id(), s.name, crate::live::matches(crate::live::count(&store, &s.spec)), s.spec.describe());
                }
                if all.is_empty() {
                    ctx.io.info("no live searches yet; save one with `mnnn search save NAME --query TEXT` (or B in the TUI)");
                }
            }
        }
        SearchCommand::Show { search, json } => {
            let s = store.find_live_search(&search)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&s)?);
            } else {
                println!("id:      {}", s.id);
                println!("name:    {}", s.name);
                println!("search:  {}", s.spec.describe());
                println!("sort:    {:?}{}", s.spec.sort, if s.spec.desc { " desc" } else { "" });
                if let Some(f) = &s.forked_from {
                    let name = store.live_search(f)?.map(|o| o.name).unwrap_or_else(|| "(removed)".into());
                    println!("fork of: {name} ({})", crate::store::projects::short_of(f));
                }
                println!("author:  {}", crate::history::author_label(&store, s.author_id.as_deref()));
                println!("created: {}", fmt_ts(s.created_at));
                println!("updated: {}", fmt_ts(s.updated_at));
                println!("matches: {} now", crate::live::count(&store, &s.spec));
                let ps: Vec<String> = store.projects_with(crate::store::ItemKind::LiveSearch, &s.id)?.into_iter().map(|p| p.name).collect();
                if !ps.is_empty() {
                    println!("projects: {}", ps.join(", "));
                }
            }
        }
        SearchCommand::Run { search, json } => {
            let s = store.find_live_search(&search)?;
            print_results(&store, &s.spec, json)?;
        }
        SearchCommand::Find { spec, save, json } => {
            let sp = spec.apply(ctx, &store, SearchSpec::default())?;
            print_results(&store, &sp, json)?;
            if let Some(name) = save {
                let s = store.save_live_search(&name, &sp, None)?;
                record(&store, "search.save", format!("save live search {}", s.name), params(&s));
                ctx.io.note(format!("saved live search {} ({})", s.name, s.short_id()));
            }
        }
        SearchCommand::Save { name, spec } => {
            let sp = spec.apply(ctx, &store, SearchSpec::default())?;
            let s = store.save_live_search(&name, &sp, None)?;
            record(&store, "search.save", format!("save live search {}", s.name), params(&s));
            ctx.io.info(format!(
                "saved live search {} ({}): {}, {} now",
                s.name,
                s.short_id(),
                s.spec.describe(),
                crate::live::matches(crate::live::count(&store, &s.spec))
            ));
        }
        SearchCommand::Fork { search, name, spec } => {
            let o = store.find_live_search(&search)?;
            let sp = spec.apply(ctx, &store, o.spec.clone())?;
            let s = store.fork_live_search(&o.id, name.as_deref(), Some(&sp))?;
            record(&store, "search.fork", format!("fork live search {} → {}", o.name, s.name), params(&s));
            ctx.io.info(format!("forked {} → {} ({}): {}", o.name, s.name, s.short_id(), s.spec.describe()));
        }
        SearchCommand::Edit { search, name, spec } => {
            let o = store.find_live_search(&search)?;
            let sp = spec.apply(ctx, &store, o.spec.clone())?;
            let s = store.update_live_search(&o.id, name.as_deref(), Some(&sp))?;
            record(&store, "search.edit", format!("edit live search {}", s.name), params(&s));
            ctx.io.info(format!("updated live search {} ({}): {}", s.name, s.short_id(), s.spec.describe()));
        }
        SearchCommand::Remove { search } => {
            let s = store.find_live_search(&search)?;
            if !ctx.io.confirm(&format!("remove live search `{}`? It also leaves every project.", s.name))? {
                return Err(crate::Error::Aborted.into());
            }
            store.remove_live_search(&s.id)?;
            record(&store, "search.remove", format!("remove live search {}", s.name), params(&s));
            ctx.io.info(format!("removed live search {}", s.name));
        }
    }
    Ok(())
}
