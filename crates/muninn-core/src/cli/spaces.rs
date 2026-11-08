//! `mnnn spaces` (collections of code) and `mnnn migrate`. Thin over
//! [`crate::code::spaces`] and [`crate::code::migrate`].
use super::{Ctx, MigrateArgs, SpacesCommand};
use crate::code::{self, migrate, spaces};
use crate::store::{CodeSpace, Space, SpaceKind, Store};
use anyhow::anyhow;
use serde_json::json;

fn row(store: &Store, s: &Space) -> serde_json::Value {
    let ms = spaces::members(store, s).unwrap_or_default();
    json!({
        "id": s.id, "name": s.name, "kind": s.kind, "enabled": s.enabled,
        "approved": spaces::approved(store, s),
        "main": ms.iter().find(|m| m.main).map(|m| m.code.name.clone()),
        "members": ms.iter().map(|m| json!({ "code": m.code.name, "alias": m.member.alias, "main": m.main, "pin": m.member.pin_hash, "pin_broken": m.pin_broken, "unreviewed": m.unreviewed })).collect::<Vec<_>>(),
    })
}

fn flags(store: &Store, s: &Space) -> String {
    let ms = spaces::members(store, s).unwrap_or_default();
    let mut f = vec![];
    if !s.enabled {
        f.push("disabled".to_string());
    }
    let un = ms.iter().filter(|m| m.unreviewed).count();
    if un > 0 {
        f.push(format!("{un} unreviewed"));
    }
    let pb = ms.iter().filter(|m| m.pin_broken).count();
    if pb > 0 {
        f.push(format!("{pb} pin changed"));
    }
    f.join(", ")
}

/// Approve a space's line-up if it isn't (asks; -y yes), offering its unreviewed code too.
pub(crate) fn ensure_approved(ctx: &Ctx, store: &Store, s: &Space) -> anyhow::Result<()> {
    if spaces::approved(store, s) {
        return Ok(());
    }
    spaces::check(store, s)?;
    eprintln!(
        "code space {} ({}) is {}; it runs native code with your rights:",
        s.name,
        s.kind,
        if s.approved_hash.is_some() { "changed" } else { "new" }
    );
    let ms = spaces::members(store, s)?;
    for m in &ms {
        eprintln!(
            "  {} {} ({}){}{}",
            if m.main { "main" } else { "    " },
            m.member.alias,
            m.code.name,
            if m.member.pin_hash.is_some() { " pinned" } else { "" },
            if m.unreviewed { " · unreviewed" } else { "" }
        );
    }
    let main = store.code_space(&s.main_code_id)?.ok_or_else(|| anyhow!("no main"))?;
    let imp = spaces::imports(&main.source);
    if !imp.spaces.is_empty() {
        eprintln!("  calls spaces: {}", imp.spaces.join(", "));
    }
    eprintln!("review the code with `mnnn code show NAME`");
    if !ctx.io.confirm(&format!("approve code space `{}` (its line-up and its unreviewed code)?", s.name))? {
        return Err(crate::Error::Aborted.into());
    }
    spaces::approve(store, "cli", s, true)?;
    Ok(())
}

fn member_of(store: &Store, s: &Space, code: &str) -> anyhow::Result<CodeSpace> {
    let ms = spaces::members(store, s)?;
    if let Some(m) = ms.iter().find(|m| m.member.alias.eq_ignore_ascii_case(code)) {
        return Ok(m.code.clone());
    }
    let c = store.find_code_space(code)?;
    if !ms.iter().any(|m| m.code.id == c.id) {
        return Err(anyhow!("{} is not a member of {}", c.name, s.name));
    }
    Ok(c)
}

pub fn run(ctx: &Ctx, cmd: SpacesCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    if let Some(m) = migrate::offer_once(&store, &ctx.paths.home) {
        ctx.io.note(m);
    }
    match cmd {
        SpacesCommand::List { kind, json } => {
            let want = kind.map(|k| k.kind());
            let v: Vec<Space> = store.spaces()?.into_iter().filter(|s| want.is_none_or(|k| s.kind == k)).collect();
            if json {
                println!("{}", serde_json::to_string_pretty(&v.iter().map(|s| row(&store, s)).collect::<Vec<_>>())?);
            } else if v.is_empty() {
                ctx.io.info("no code spaces yet; `mnnn spaces create NAME --kind K --main CODE`, or `mnnn migrate` to make them from your code");
            } else {
                for s in &v {
                    let main = store.code_space(&s.main_code_id)?.map(|c| c.name).unwrap_or_default();
                    println!(
                        "{}\t{}\t{}\tmain {}\t{} member(s)\t{}\t{}",
                        s.short_id(),
                        s.name,
                        s.kind,
                        main,
                        store.space_members(&s.id)?.len(),
                        if spaces::approved(&store, s) { "approved" } else { "not approved" },
                        flags(&store, s)
                    );
                }
            }
        }
        SpacesCommand::Show { space, json } => {
            let s = store.find_space(&space)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&row(&store, &s))?);
                return Ok(());
            }
            println!("id:        {}", s.id);
            println!("name:      {}", s.name);
            if !s.description.is_empty() {
                println!("about:     {}", s.description);
            }
            println!("kind:      {}", s.kind);
            println!("enabled:   {}", if s.enabled { "yes" } else { "no" });
            println!("approved:  {}", if spaces::approved(&store, &s) { "yes (its line-up)" } else { "no: `mnnn spaces approve`" });
            println!("members:");
            for m in spaces::members(&store, &s)? {
                println!(
                    "  {} {:<16} {} ({}){}{}{}",
                    if m.main { "★" } else { " " },
                    m.member.alias,
                    m.code.name,
                    m.code.lang.as_str(),
                    match &m.member.pin_hash {
                        Some(h) => format!(" · pinned {}", &h[..12.min(h.len())]),
                        None => " · follows".into(),
                    },
                    if m.pin_broken { " · CHANGED since pinned (`mnnn spaces pin`)" } else { "" },
                    if m.unreviewed { " · edited, unreviewed" } else { "" }
                );
            }
            let main = store.code_space(&s.main_code_id)?.ok_or_else(|| anyhow!("no main"))?;
            let imp = spaces::imports(&main.source);
            if !imp.code.is_empty() || !imp.spaces.is_empty() {
                println!(
                    "imports:   {}",
                    imp.code.iter().cloned().chain(imp.spaces.iter().map(|x| format!("space:{x}"))).collect::<Vec<_>>().join(", ")
                );
            }
            if let Err(e) = spaces::check(&store, &s) {
                println!("problem:   {e}");
            }
        }
        SpacesCommand::Create { name, kind, main, add, description } => {
            let m = store.find_code_space(&main)?;
            let s = spaces::create(&store, "cli", &name, &description, kind.kind(), &m)?;
            for a in &add {
                let (c, alias) = a.split_once('=').map(|(c, a)| (c, Some(a))).unwrap_or((a.as_str(), None));
                let c = store.find_code_space(c)?;
                store.add_space_member(&s.id, &c, alias, None)?;
            }
            if !add.is_empty() {
                spaces::note(&store, "cli", &s, "members added", json!({ "add": add }));
            }
            ctx.io.info(format!("created code space {} ({}, main {}); approve it with `mnnn spaces approve {}`", s.name, s.kind, m.name, s.name));
        }
        SpacesCommand::Add { space, code, alias, pin } => {
            let s = store.find_space(&space)?;
            let c = store.find_code_space(&code)?;
            let h = if pin { Some(code::trust_hash(&c)?) } else { None };
            let m = store.add_space_member(&s.id, &c, alias.as_deref(), h.as_deref())?;
            spaces::note(&store, "cli", &s, &format!("add {} as {}", c.name, m.alias), json!({ "code": c.id, "alias": m.alias, "pin": h }));
            ctx.io.info(format!(
                "added {} to {} as `{}`{}; the space needs approving again",
                c.name,
                s.name,
                m.alias,
                if pin { " (pinned)" } else { "" }
            ));
        }
        SpacesCommand::RemoveCode { space, code } => {
            let s = store.find_space(&space)?;
            let c = member_of(&store, &s, &code)?;
            store.remove_space_member(&s.id, &c.id)?;
            spaces::note(&store, "cli", &s, &format!("remove {}", c.name), json!({ "code": c.id }));
            ctx.io.info(format!("removed {} from {}", c.name, s.name));
        }
        SpacesCommand::SetMain { space, code } => {
            let s = store.find_space(&space)?;
            let c = store.find_code_space(&code)?;
            store.set_space_main(&s.id, &c)?;
            spaces::note(&store, "cli", &s, &format!("main is now {}", c.name), json!({ "main": c.id }));
            ctx.io.info(format!("{} is the main of {}; the space needs approving again", c.name, s.name));
        }
        SpacesCommand::Pin { space, code } => {
            let s = store.find_space(&space)?;
            let c = member_of(&store, &s, &code)?;
            let h = code::trust_hash(&c)?;
            store.set_member_pin(&s.id, &c.id, Some(&h))?;
            spaces::note(&store, "cli", &s, &format!("pin {}", c.name), json!({ "code": c.id, "pin": h }));
            ctx.io.info(format!("pinned {} in {} at {}; the space needs approving again", c.name, s.name, &h[..12]));
        }
        SpacesCommand::Unpin { space, code } => {
            let s = store.find_space(&space)?;
            let c = member_of(&store, &s, &code)?;
            store.set_member_pin(&s.id, &c.id, None)?;
            spaces::note(&store, "cli", &s, &format!("unpin {}", c.name), json!({ "code": c.id }));
            ctx.io.info(format!("{} follows its code again in {}; the space needs approving again", c.name, s.name));
        }
        SpacesCommand::Approve { space } => {
            let s = store.find_space(&space)?;
            if spaces::approved(&store, &s) {
                ctx.io.info(format!("code space {} is approved already", s.name));
            } else {
                ensure_approved(ctx, &store, &s)?;
                ctx.io.info(format!("approved code space {}", s.name));
            }
        }
        SpacesCommand::Run { space, input, json, rows, connector } if connector.connector_dry_run => {
            let s = store.find_space(&space)?;
            if s.kind != crate::store::SpaceKind::IngestConnector {
                anyhow::bail!(
                    "{} is a {} space; --connector-dry-run is for ingest-connector spaces (`mnnn spaces kind {} ingest-connector`)",
                    s.name,
                    s.kind,
                    s.name
                );
            }
            let kv = |v: &str| -> anyhow::Result<(String, String)> {
                v.split_once('=').map(|(k, v)| (k.trim().to_string(), v.to_string())).ok_or_else(|| anyhow::anyhow!("`{v}`: give KEY=VALUE"))
            };
            let mut opts = serde_json::Map::new();
            for o in &connector.options {
                let (k, v) = kv(o)?;
                opts.insert(k, json!(v));
            }
            let mut d = code::connector::DryRun::new(&s.name, connector.source.clone(), connector.pipeline.clone(), connector.ruleset.clone(), opts);
            for o in &connector.state {
                let (k, v) = kv(o)?;
                d.state.insert(k, serde_json::from_str(&v).unwrap_or(json!(v)));
            }
            let src = match &input {
                Some(i) => code::pipe::Source::parse(i)?,
                None => code::pipe::Source::Space(s.name.clone()),
            };
            let steps = if input.is_some() { vec![code::pipe::Step::Space(s.name.clone())] } else { vec![] };
            let (out, d) = code::connector::dry_run(d, || super::code::run_pipe(ctx, &store, &src, &steps, &[], json, rows));
            if json {
                println!("{}", serde_json::to_string(&json!({ "connector_dry_run": d }))?);
            } else {
                ctx.io.info(format!(
                    "connector dry run: {} call(s), {} item(s), {} byte(s) — nothing stored, piped or run",
                    d.calls.len(),
                    d.stored,
                    d.bytes
                ));
                for l in d.summary() {
                    ctx.io.info(format!("  {l}"));
                }
            }
            out?
        }
        SpacesCommand::Run { space, input, json, rows, .. } => {
            let s = store.find_space(&space)?;
            match input {
                Some(src) => super::code::run_pipe(
                    ctx,
                    &store,
                    &code::pipe::Source::parse(&src)?,
                    &[code::pipe::Step::Space(s.name.clone())],
                    &[],
                    json,
                    rows,
                )?,
                None => super::code::run_pipe(ctx, &store, &code::pipe::Source::Space(s.name.clone()), &[], &[], json, rows)?,
            }
        }
        SpacesCommand::Rename { space, name } => {
            let s = store.find_space(&space)?;
            store.rename_space(&s.id, &name)?;
            spaces::note(&store, "cli", &s, &format!("renamed to {name}"), json!({ "name": name }));
            ctx.io.info(format!("renamed {} to {}", s.name, name.trim()));
        }
        SpacesCommand::Kind { space, kind } => {
            let s = store.find_space(&space)?;
            store.set_space_kind(&s.id, kind.kind())?;
            spaces::note(&store, "cli", &s, &format!("kind {}", kind.kind()), json!({ "kind": kind.kind() }));
            ctx.io.info(format!("{} is a {} space now; it needs approving again", s.name, kind.kind()));
        }
        SpacesCommand::Enable { space } => toggle(ctx, &store, &space, true)?,
        SpacesCommand::Disable { space } => toggle(ctx, &store, &space, false)?,
        SpacesCommand::Export { space, out } => {
            let b = crate::code::space_share::export(&store, &space)?;
            let bytes = serde_json::to_vec_pretty(&b)?;
            match out.as_deref().filter(|p| p.as_os_str() != "-") {
                Some(p) => {
                    std::fs::write(p, &bytes)?;
                    ctx.io.info(format!("wrote {} ({} code space(s), {} code)", p.display(), b.spaces.len(), b.code.len()));
                }
                None => println!("{}", String::from_utf8_lossy(&bytes)),
            }
            store.note_action(
                &crate::store::NewAction::new("space.export", "cli", format!("code space {} exported", b.name)).params(json!({ "space": b.name })),
            );
        }
        SpacesCommand::Import { file, name, dry_run } => {
            let b = crate::code::space_share::Bundle::parse(&std::fs::read(&file)?)?;
            let plan = crate::code::space_share::plan(&store, &b, name.as_deref())?;
            for l in plan.lines() {
                println!("{l}");
            }
            if dry_run {
                return Ok(());
            }
            if !ctx.io.confirm("import it?")? {
                return Err(crate::Error::Aborted.into());
            }
            let s = crate::code::space_share::import(&store, &b, &plan)?;
            store.note_action(
                &crate::store::NewAction::new("space.import", "cli", format!("code space {} imported", s.name))
                    .params(json!({ "space": s.id, "plan": plan })),
            );
            ctx.io.info(format!("imported code space {}; review and approve it (`mnnn spaces approve {}`) before it runs", s.name, s.name));
        }
        SpacesCommand::Remove { space } => {
            let s = store.find_space(&space)?;
            if !ctx.io.confirm(&format!("remove code space `{}`? (its code stays)", s.name))? {
                return Err(crate::Error::Aborted.into());
            }
            spaces::remove(&store, "cli", &s.id)?;
            ctx.io.info(format!("removed code space {}", s.name));
        }
    }
    Ok(())
}

fn toggle(ctx: &Ctx, store: &Store, space: &str, on: bool) -> anyhow::Result<()> {
    let s = store.find_space(space)?;
    store.set_space_enabled(&s.id, on)?;
    spaces::note(store, "cli", &s, if on { "enabled" } else { "disabled" }, json!({ "enabled": on }));
    ctx.io.info(format!("{} code space {}", if on { "enabled" } else { "disabled" }, s.name));
    Ok(())
}

/// `mnnn migrate`.
/// `mnnn status`: the home, the database, counts, and the reminder to migrate.
pub fn status(ctx: &Ctx, json: bool) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    let pending = migrate::pending(&store)?;
    let code = store.code_spaces()?.len();
    let spaces = store.spaces()?;
    let unapproved = spaces.iter().filter(|s| !crate::code::spaces::approved(&store, s)).count();
    let bars = store.count_bars()?;
    let todo: Vec<String> = [
        (pending > 0).then(|| {
            format!("{pending} code not in any code space: `mnnn migrate --dry-run` shows what `mnnn migrate` would do (pipelines run spaces; code: steps are deprecated)")
        }),
        (unapproved > 0).then(|| format!("{unapproved} code space(s) not approved: `mnnn spaces list`")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "home": ctx.paths.home,
                "database": ctx.paths.db,
                "schema": store.schema_version()?,
                "bars": bars,
                "code": code,
                "spaces": spaces.len(),
                "unmigrated_code": pending,
                "unapproved_spaces": unapproved,
                "todo": todo,
            }))?
        );
        return Ok(());
    }
    println!("muninn      {}", env!("CARGO_PKG_VERSION"));
    println!("home        {}", ctx.paths.home.display());
    println!("database    {} (schema {})", ctx.paths.db.display(), store.schema_version()?);
    println!("bars        {bars}");
    println!("code        {code} ({pending} not in a code space)");
    println!("code spaces {} ({unapproved} not approved)", spaces.len());
    for t in &todo {
        println!("to do: {t}");
    }
    Ok(())
}

pub fn migrate(ctx: &Ctx, a: MigrateArgs) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    let _ = migrate::offer_once(&store, &ctx.paths.home);
    let opts = migrate::Options {
        names: a.codes,
        kind: a.kind.map(|k| k.kind()),
        dry_run: a.dry_run,
        rewrite_pipelines: !a.no_rewrite_pipelines,
        backup: !a.no_backup,
    };
    let io = ctx.io;
    let mut ask = |c: &CodeSpace, i: &migrate::Inference| -> crate::Result<Option<SpaceKind>> {
        if io.yes || !io.interactive {
            return Ok(None);
        }
        let kinds = SpaceKind::ALL.map(|k| k.as_str()).join(", ");
        let ans =
            io.prompt("kind", &format!("{}: {}. Its space's kind ({kinds}; empty skips)", c.name, i.why), Some(i.suggest.as_str()), "give --kind")?;
        if ans.trim().is_empty() || ans.trim() == "skip" {
            return Ok(None);
        }
        SpaceKind::from_arg(&ans).map(Some)
    };
    let rep = migrate::migrate(&store, &ctx.paths.home, "cli", &opts, &mut ask)?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&rep)?);
        return Ok(());
    }
    if rep.is_empty() {
        ctx.io.info("nothing to migrate: every code is in a code space");
        return Ok(());
    }
    let created: Vec<_> = rep.created.iter().filter(|c| !c.renamed).collect();
    println!("created ({}):", created.len());
    for c in &created {
        println!("  {} ({}){} · {}", c.space, c.kind, if c.approved { " · approved" } else { "" }, c.why);
    }
    let renamed = rep.renamed();
    println!("renamed ({}):", renamed.len());
    for c in &renamed {
        println!("  {} → {} ({}){} · {}", c.code, c.space, c.kind, if c.approved { " · approved" } else { "" }, c.why);
    }
    println!("skipped ({}):", rep.skipped.len());
    for k in &rep.skipped {
        println!("  {}: {}", k.code, k.why);
    }
    println!("pipelines rewritten ({}):", rep.pipelines.len());
    for (p, v) in &rep.pipelines {
        println!("  {p} (now version {v})");
    }
    if let Some(b) = &rep.backup {
        ctx.io.note(format!("backup: {}", b.display()));
    }
    if rep.dry_run {
        ctx.io.note("dry run: nothing changed");
    }
    Ok(())
}
