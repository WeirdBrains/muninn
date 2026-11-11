//! Code spaces as collections (`mnnn spaces`), `mnnn migrate`, and plugin info
//! (`mnnn plugins info`). Code units stay on [`super::code`].
use super::data::{open, open_write};
use super::info::paths;
use muninn_core::code::pipeline::GraphExt as _;
use muninn_core::code::{self, CodeEnv, migrate, spaces};
use muninn_core::store::{Space, SpaceKind, Store};
use serde_json::json;

const ORIGIN: &str = "app";

pub struct CollectionInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub kind: String,
    pub enabled: bool,
    pub approved: bool,
    pub main: String,
    pub members: i64,
    pub unreviewed: i64,
    pub pin_broken: i64,
}

pub struct MemberInfo {
    pub code_id: String,
    pub code_name: String,
    pub alias: String,
    pub lang: String,
    pub main: bool,
    pub pinned: bool,
    pub pin_broken: bool,
    pub unreviewed: bool,
}

pub struct MigrateLine {
    pub code: String,
    pub space: String,
    pub kind: String,
    pub renamed: bool,
    pub approved: bool,
    pub why: String,
}

pub struct MigrateSkip {
    pub code: String,
    pub why: String,
}

pub struct MigrateReport {
    pub created: Vec<MigrateLine>,
    pub skipped: Vec<MigrateSkip>,
    /// `name@version` for each rewritten pipeline.
    pub pipelines: Vec<String>,
    pub backup: String,
    pub dry_run: bool,
}

pub struct PluginDetail {
    pub name: String,
    pub kind: String,
    pub lang: String,
    pub main: String,
    pub library: String,
    pub declared: String,
    pub members: i64,
    pub approved: bool,
    pub enabled: bool,
    pub used_by: Vec<String>,
    pub problem: String,
}

pub struct CollectionRun {
    pub status: String,
    pub error: String,
    pub lines: Vec<String>,
}

fn kind_of(k: &str) -> anyhow::Result<SpaceKind> {
    SpaceKind::from_arg(k).map_err(|e| anyhow::anyhow!("{e}"))
}

fn info_of(store: &Store, s: &Space) -> anyhow::Result<CollectionInfo> {
    let ms = spaces::members(store, s)?;
    Ok(CollectionInfo {
        id: s.id.clone(),
        name: s.name.clone(),
        description: s.description.clone(),
        kind: s.kind.as_str().into(),
        enabled: s.enabled,
        approved: spaces::approved(store, s),
        main: ms.iter().find(|m| m.main).map(|m| m.code.name.clone()).unwrap_or_default(),
        members: ms.len() as i64,
        unreviewed: ms.iter().filter(|m| m.unreviewed).count() as i64,
        pin_broken: ms.iter().filter(|m| m.pin_broken).count() as i64,
    })
}

fn member_info(m: &spaces::MemberState) -> MemberInfo {
    MemberInfo {
        code_id: m.code.id.clone(),
        code_name: m.code.name.clone(),
        alias: m.member.alias.clone(),
        lang: m.code.lang.as_str().into(),
        main: m.main,
        pinned: m.member.pin_hash.is_some(),
        pin_broken: m.pin_broken,
        unreviewed: m.unreviewed,
    }
}

fn report_of(r: migrate::Report) -> MigrateReport {
    MigrateReport {
        created: r
            .created
            .into_iter()
            .map(|c| MigrateLine { code: c.code, space: c.space, kind: c.kind.as_str().into(), renamed: c.renamed, approved: c.approved, why: c.why })
            .collect(),
        skipped: r.skipped.into_iter().map(|k| MigrateSkip { code: k.code, why: k.why }).collect(),
        pipelines: r.pipelines.into_iter().map(|(n, v)| format!("{n}@{v}")).collect(),
        backup: r.backup.map(|p| p.display().to_string()).unwrap_or_default(),
        dry_run: r.dry_run,
    }
}

fn used_by(store: &Store, s: &Space) -> anyhow::Result<Vec<String>> {
    let mut v = vec![];
    for p in store.pipelines()? {
        if p.graph.spaces().iter().any(|n| n.eq_ignore_ascii_case(&s.name) || *n == s.id) {
            v.push(format!("pipeline {}", p.name));
        }
    }
    for r in store.rules()? {
        if r.spec.then.iter().any(|t| matches!(t, muninn_core::store::Then::RunSpace { space, .. } if *space == s.id)) {
            v.push(format!("rule {}", r.name));
        }
    }
    for o in store.spaces()? {
        if o.id == s.id {
            continue;
        }
        if let Some(m) = store.code_space(&o.main_code_id)?
            && spaces::imports(&m.source).spaces.iter().any(|n| n.eq_ignore_ascii_case(&s.name))
        {
            v.push(format!("space {} (its main imports it)", o.name));
        }
    }
    Ok(v)
}

/// `mnnn spaces list [--kind K]`.
pub fn collections_list(kind: Option<String>) -> anyhow::Result<Vec<CollectionInfo>> {
    let store = open()?;
    let want = kind.as_deref().map(str::trim).filter(|k| !k.is_empty()).map(kind_of).transpose()?;
    let mut v = vec![];
    for s in store.spaces()?.into_iter().filter(|s| want.is_none_or(|k| s.kind == k)) {
        v.push(info_of(&store, &s)?);
    }
    Ok(v)
}

/// One space's members (★ main, pins, unreviewed).
pub fn collection_members(space: String) -> anyhow::Result<Vec<MemberInfo>> {
    let store = open()?;
    let s = store.find_space(space.trim())?;
    Ok(spaces::members(&store, &s)?.iter().map(member_info).collect())
}

/// `mnnn spaces create NAME --kind K --main CODE`.
pub fn collection_create(name: String, description: String, kind: String, main: String) -> anyhow::Result<CollectionInfo> {
    let store = open_write()?;
    let m = store.find_code_space(main.trim())?;
    let s = spaces::create(&store, ORIGIN, name.trim(), description.trim(), kind_of(kind.trim())?, &m)?;
    info_of(&store, &s)
}

/// `mnnn spaces add SPACE CODE [--as ALIAS] [--pin]`.
pub fn collection_add(space: String, code: String, alias: Option<String>, pin: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = store.find_space(space.trim())?;
    let c = store.find_code_space(code.trim())?;
    let h = if pin { Some(code::trust_hash(&c)?) } else { None };
    let m = store.add_space_member(&s.id, &c, alias.as_deref().map(str::trim).filter(|a| !a.is_empty()), h.as_deref())?;
    spaces::note(&store, ORIGIN, &s, &format!("add {} as {}", c.name, m.alias), json!({ "code": c.id, "alias": m.alias }));
    Ok(format!("added {} to {} as `{}`; the space needs approving again", c.name, s.name, m.alias))
}

/// `mnnn spaces remove-code SPACE CODE`.
pub fn collection_remove_code(space: String, code: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = store.find_space(space.trim())?;
    let c = store.find_code_space(code.trim())?;
    store.remove_space_member(&s.id, &c.id)?;
    spaces::note(&store, ORIGIN, &s, &format!("remove {}", c.name), json!({ "code": c.id }));
    Ok(format!("removed {} from {}", c.name, s.name))
}

/// `mnnn spaces set-main SPACE CODE`.
pub fn collection_set_main(space: String, code: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = store.find_space(space.trim())?;
    let c = store.find_code_space(code.trim())?;
    store.set_space_main(&s.id, &c)?;
    spaces::note(&store, ORIGIN, &s, &format!("main is now {}", c.name), json!({ "main": c.id }));
    Ok(format!("{} is the main of {}; the space needs approving again", c.name, s.name))
}

/// Pin (`on`) or unpin a member. The space needs approving again.
pub fn collection_pin(space: String, code: String, on: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = store.find_space(space.trim())?;
    let c = store.find_code_space(code.trim())?;
    let h = if on { Some(code::trust_hash(&c)?) } else { None };
    store.set_member_pin(&s.id, &c.id, h.as_deref())?;
    spaces::note(&store, ORIGIN, &s, if on { "pin" } else { "unpin" }, json!({ "code": c.id }));
    Ok(format!("{} {} in {}; the space needs approving again", if on { "pinned" } else { "unpinned" }, c.name, s.name))
}

/// Approve the line-up and unreviewed member code (TUI `a` / `y`).
pub fn collection_approve(space: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = store.find_space(space.trim())?;
    if spaces::approved(&store, &s) {
        return Ok(format!("{} is approved already", s.name));
    }
    spaces::approve(&store, ORIGIN, &s, true)?;
    Ok(format!("approved {} (its line-up and its unreviewed code)", s.name))
}

/// Space enable/disable (TUI Space).
pub fn collection_set_enabled(space: String, enabled: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = store.find_space(space.trim())?;
    store.set_space_enabled(&s.id, enabled)?;
    spaces::note(&store, ORIGIN, &s, if enabled { "enabled" } else { "disabled" }, json!({ "enabled": enabled }));
    Ok(format!("{} code space {}", if enabled { "enabled" } else { "disabled" }, s.name))
}

/// `mnnn spaces remove` (its code stays). Refuses while a schedule uses it.
pub fn collection_remove(space: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = store.find_space(space.trim())?;
    let using = store.schedules_using("plugin_id", &s.id)?;
    if !using.is_empty() {
        anyhow::bail!(
            "schedules use plugin {}: {} (remove or retarget them first)",
            s.name,
            using.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ")
        );
    }
    spaces::remove(&store, ORIGIN, &s.id)?;
    Ok(format!("removed code space {} (its code stays)", s.name))
}

/// Run the space's main (TUI `r`). Connectors run only from a schedule.
pub fn collection_run(space: String) -> anyhow::Result<CollectionRun> {
    let store = open_write()?;
    let s = store.find_space(space.trim())?;
    let env = CodeEnv::for_store(&store)?;
    let mut log = vec![];
    let main = store.code_space(&s.main_code_id)?;
    let src = main.as_ref().map(|m| code::pipe::Source::of(m.input.clone()));
    let input = match &src {
        Some(src) => code::pipe::frame(&store, &env, ORIGIN, src, &mut |l| log.push(l.to_string()))?,
        None => None,
    };
    let info = spaces::run(&store, &env, ORIGIN, &s, input, "the app", &mut |l| log.push(l.to_string()))?;
    let mut lines: Vec<String> = log.into_iter().filter(|l| l.starts_with("note:")).collect();
    if let Some(v) = code::run_view(&info) {
        lines.extend(v.text_lines(30));
    }
    Ok(CollectionRun { status: info.status, error: info.error.unwrap_or_default(), lines })
}

/// How many code units are in no space.
pub fn migrate_pending() -> anyhow::Result<i64> {
    Ok(migrate::pending(&open()?)? as i64)
}

/// The one-time offer was already shown (the `migrate-offered` marker).
pub fn migrate_offered(home: Option<String>) -> anyhow::Result<bool> {
    Ok(paths(home)?.home.join("migrate-offered").exists())
}

/// Don't ask again (Later and Don't ask again write the same marker).
pub fn migrate_dismiss(home: Option<String>) -> anyhow::Result<()> {
    let p = paths(home)?;
    std::fs::create_dir_all(&p.home)?;
    std::fs::write(p.home.join("migrate-offered"), format!("{}\n", muninn_core::store::now_ms()))?;
    Ok(())
}

/// `mnnn migrate`. Unclear kinds are skipped unless [kind] is set (the TUI's `y`).
/// A dry run changes nothing.
#[allow(clippy::too_many_arguments)]
pub fn migrate_run(
    home: Option<String>,
    codes: Vec<String>,
    kind: Option<String>,
    dry_run: bool,
    rewrite_pipelines: bool,
    backup: bool,
) -> anyhow::Result<MigrateReport> {
    let store = open_write()?;
    let home = paths(home)?.home;
    let opts = migrate::Options {
        names: codes.into_iter().map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect(),
        kind: kind.as_deref().map(str::trim).filter(|k| !k.is_empty()).map(kind_of).transpose()?,
        dry_run,
        rewrite_pipelines,
        backup,
    };
    let rep = migrate::migrate(&store, &home, ORIGIN, &opts, &mut |_, _| Ok(None))?;
    Ok(report_of(rep))
}

/// `mnnn plugins info NAME`.
pub fn plugin_info(name: String) -> anyhow::Result<PluginDetail> {
    let store = open()?;
    let s = store.find_space(name.trim())?;
    let main = store.code_space(&s.main_code_id)?.ok_or_else(|| anyhow::anyhow!("{}: no main", s.name))?;
    let approved = spaces::approved(&store, &s);
    let declared = if approved { code::declared_kind(&main).unwrap_or_default() } else { String::new() };
    let problem = spaces::check(&store, &s)
        .and_then(|_| if approved { spaces::check_declared_kind(&store, &s) } else { Ok(()) })
        .err()
        .map(|e| e.to_string())
        .unwrap_or_default();
    let library = if main.lang == muninn_core::store::Lang::Plugin { main.source.trim().to_string() } else { String::new() };
    Ok(PluginDetail {
        name: s.name.clone(),
        kind: s.kind.as_str().into(),
        lang: main.lang.as_str().into(),
        main: main.name,
        library,
        declared,
        members: store.space_members(&s.id)?.len() as i64,
        approved,
        enabled: s.enabled,
        used_by: used_by(&store, &s)?,
        problem,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::{CodeInput, Lang};

    #[test]
    fn collection_members_and_migrate_dry_run() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open_in_memory().unwrap();
        let c = s.create_code_space("alpha", "", Lang::Math, "x = close", CodeInput::default()).unwrap();
        let space = spaces::create(&s, "t", "alpha-space", "about", SpaceKind::Analysis, &c).unwrap();
        let info = info_of(&s, &space).unwrap();
        assert_eq!(info.main, "alpha");
        assert_eq!(info.members, 1);
        assert!(!info.approved);
        let ms = spaces::members(&s, &space).unwrap();
        assert!(ms[0].main);

        let other = s.create_code_space("beta", "", Lang::Math, "y = 1", CodeInput::default()).unwrap();
        s.add_space_member(&space.id, &other, Some("b"), None).unwrap();
        s.create_code_space("gamma", "", Lang::Math, "z = 1", CodeInput::default()).unwrap();
        assert_eq!(spaces::members(&s, &s.find_space("alpha-space").unwrap()).unwrap().len(), 2);

        let dry = migrate::migrate(&s, dir.path(), "t", &migrate::Options { dry_run: true, ..Default::default() }, &mut |_, _| Ok(None)).unwrap();
        assert!(dry.dry_run);
        assert_eq!(s.spaces().unwrap().len(), 1, "dry run changes nothing");
        assert!(dry.created.iter().any(|c| c.space == "gamma"));
    }
}
