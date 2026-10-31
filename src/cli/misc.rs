//! `mnnn theme`, `mnnn tui`, `mnnn help`, `mnnn man`.
use super::{Ctx, ThemeCommand};
use crate::Error;
use crate::history::{ViewState, view_action};
use crate::prefs::Preferences;
use crate::theme::{self, ThemeFile};
use std::path::Path;

/// Record a view change when the store opens without a prompt (see `Ctx::store_if_unlocked`).
fn record_view(ctx: &Ctx, kind: &str, summary: String, before: &ViewState, prefs: &Preferences) {
    let after = ViewState::capture(prefs);
    if after != *before
        && let Some(store) = ctx.store_if_unlocked()
    {
        store.note_action(&view_action(kind, "cli", summary, before, &after));
    }
}

pub fn theme(ctx: &Ctx, cmd: ThemeCommand) -> anyhow::Result<()> {
    theme::materialize(&ctx.paths)?;
    let mut prefs = ctx.prefs()?;
    match cmd {
        ThemeCommand::List => {
            for n in theme::list(&ctx.paths)? {
                let mark = if n == prefs.theme.name { "*" } else { " " };
                let disp = theme::load(&ctx.paths, &n).map(|t| t.display_name()).unwrap_or_else(|e| format!("(invalid: {e})"));
                println!("{mark} {n:<14} {disp}");
            }
        }
        ThemeCommand::Show => {
            let mode = ctx.global.theme_mode.unwrap_or(prefs.theme.mode);
            let v = theme::resolve_variant(mode);
            let (t, warn) = theme::active(&ctx.paths, &prefs.theme.name);
            println!("theme    {} ({})", t.meta.name, t.display_name());
            println!("mode     {mode} -> {}", v.name());
            for w in theme::contrast_warnings(t.variant(v)) {
                println!("warning  {w}");
            }
            if let Some(w) = warn {
                println!("warning  configured theme `{}` is invalid, using default: {w}", prefs.theme.name);
            }
        }
        ThemeCommand::Set { name } => {
            theme::load(&ctx.paths, &name)?;
            let before = ViewState::capture(&prefs);
            prefs.theme.name = name.clone();
            prefs.save(&ctx.paths)?;
            record_view(ctx, "theme", format!("theme {name}"), &before, &prefs);
            ctx.io.info(format!("theme set to {name}"));
        }
        ThemeCommand::Mode { mode } => {
            let before = ViewState::capture(&prefs);
            prefs.theme.mode = mode;
            prefs.save(&ctx.paths)?;
            record_view(ctx, "theme.mode", format!("theme mode {mode}"), &before, &prefs);
            ctx.io.info(format!("theme mode set to {mode}"));
        }
        ThemeCommand::Export { name, out } => {
            let t = theme::load(&ctx.paths, &name)?;
            std::fs::write(&out, t.to_toml()?)?;
            ctx.io.info(format!("wrote {}", out.display()));
        }
        ThemeCommand::Import { file } => {
            let t = ThemeFile::parse(&std::fs::read_to_string(&file)?)?;
            if ctx.paths.theme_dir(&t.meta.name).join("interface.toml").exists()
                && !ctx.io.confirm(&format!("overwrite existing theme `{}`?", t.meta.name))?
            {
                return Err(Error::Aborted.into());
            }
            theme::save(&ctx.paths, &t)?;
            ctx.io.info(format!("imported theme {}", t.meta.name));
        }
        ThemeCommand::Reset { name } => {
            theme::reset(&ctx.paths, &name)?;
            ctx.io.info(format!("{name} reset to the built-in version"));
        }
    }
    Ok(())
}

pub fn tui(ctx: &Ctx) -> anyhow::Result<()> {
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return Err(Error::invalid("terminal", "the TUI needs an interactive terminal").into());
    }
    let dek = ctx.unlock()?;
    let store = crate::store::Store::open(&ctx.paths.db, Some(&dek.0))?;
    let app = crate::tui::App::new(ctx.paths.clone(), store, dek, ctx.prefs()?, ctx.global.theme_mode)?;
    crate::tui::run(app)?;
    Ok(())
}

pub fn help(topic: Option<&str>, search: Option<&str>) -> anyhow::Result<()> {
    if let Some(q) = search {
        for t in crate::help::search(q) {
            println!("{:<20} {}", t.id, t.title);
        }
        return Ok(());
    }
    match topic {
        None => {
            println!("Help topics (mnnn help <topic>):\n");
            for t in crate::help::topics() {
                println!("  {:<20} {}", t.id, t.title);
            }
        }
        Some(id) => {
            let t = crate::help::topic(id)
                .ok_or_else(|| Error::invalid("topic", format!("`{id}` (see `mnnn help` for the list, or `mnnn help --search {id}`)")))?;
            print!("{}", t.body);
        }
    }
    Ok(())
}

pub fn man(out: &Path) -> anyhow::Result<()> {
    let n = crate::cli::man::generate(out)?;
    println!("wrote {n} man pages to {}", out.display());
    Ok(())
}

/// `mnnn share`.
pub fn share(ctx: &Ctx, a: super::args::ShareArgs) -> anyhow::Result<()> {
    use crate::share::{TargetKind, share, targets_for};
    use crate::store::ShareKind;
    let kind =
        ShareKind::parse(&a.kind).ok_or_else(|| anyhow::anyhow!("unknown kind {:?} (model, search, journal, project, report, code)", a.kind))?;
    if a.targets {
        let ts = targets_for(kind);
        if a.json {
            println!("{}", serde_json::to_string_pretty(&ts)?);
        } else {
            for t in ts {
                println!("{:<8} {}", t.as_str(), t.verb());
            }
        }
        return Ok(());
    }
    let (item, target) = (a.item.unwrap_or_default(), a.target.unwrap_or_default());
    let t = TargetKind::parse(&target).ok_or_else(|| anyhow::anyhow!("unknown target {target:?} (project, journal, code, net)"))?;
    let store = ctx.open_store()?;
    if a.run && t != TargetKind::Code {
        return Err(anyhow::anyhow!("--run is for sharing to a code space"));
    }
    let out = share(&store, kind, &item, t, &a.to, &a.note, "cli")?;
    if a.json && !a.run {
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        ctx.io.note(out.summary());
    }
    if a.run {
        super::code::run_space(ctx, &store, &out.made, a.json, 20)?;
    }
    Ok(())
}
