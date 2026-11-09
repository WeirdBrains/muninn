//! Create-then-share for the Flutter app (`mnnn share --create` / `--create-to` and kin).
//! Thin wrappers over `muninn_core::share`; identity for net destinations stays in Rust.
use super::data::open_write;
use super::info::paths;
use muninn_core::net::{identity, local};
use muninn_core::prefs::Preferences;
use muninn_core::share::{self, CreateOpts, NetCreate, TargetKind};
use muninn_core::store::{EntryFormat, ItemKind, Lang, ReviewKind, ShareKind, SpaceKind};

fn parse_share_kind(k: &str) -> anyhow::Result<ShareKind> {
    ShareKind::parse(k).ok_or_else(|| anyhow::anyhow!("unknown kind {k:?} (model, search, journal, project, report, code, space, pipeline)"))
}

fn parse_target(k: &str) -> anyhow::Result<TargetKind> {
    TargetKind::parse(k).ok_or_else(|| anyhow::anyhow!("unknown target {k:?} (project, journal, code, space, net)"))
}

fn parse_item_kind(k: &str) -> anyhow::Result<ItemKind> {
    Ok(match k {
        "search" | "live_search" | "live-search" => ItemKind::LiveSearch,
        "journal" => ItemKind::Journal,
        "report" => ItemKind::Report,
        "model" => ItemKind::Model,
        "code" => ItemKind::Code,
        "space" | "spaces" => ItemKind::Space,
        "pipeline" => ItemKind::Pipeline,
        "rule" | "ruleset" | "rule_set" | "rule-set" => ItemKind::RuleSet,
        "result" | "series" => ItemKind::Result,
        other => anyhow::bail!("unknown kind {other:?}"),
    })
}

fn parse_review_kind(k: &str) -> anyhow::Result<ReviewKind> {
    let k = match k.to_ascii_lowercase().as_str() {
        "live-search" | "searches" => "search".to_string(),
        "entries" => "entry".into(),
        k => k.trim_end_matches('s').to_string(),
    };
    ReviewKind::parse(&k).ok_or_else(|| anyhow::anyhow!("`{k}`: review an entry, journal, project, search or record"))
}

/// Outcome of a create-then-share call for Dart.
pub struct ShareCreated {
    pub created_item_kind: String,
    pub created_item_id: String,
    pub created_item_name: String,
    pub created_to_kind: String,
    pub created_to_id: String,
    pub created_to_name: String,
    pub summary: String,
    pub item_id: String,
    pub item_name: String,
    pub target: String,
    pub to: String,
    pub made: String,
    pub new_share: bool,
}

fn empty_created() -> (String, String, String) {
    (String::new(), String::new(), String::new())
}

fn pack(item: Option<share::Created>, to: Option<share::Created>, shared: share::Shared) -> ShareCreated {
    let (ik, iid, iname) = item.map(|c| (c.kind, c.id, c.name)).unwrap_or_else(empty_created);
    let (tk, tid, tname) = to.map(|c| (c.kind, c.id, c.name)).unwrap_or_else(empty_created);
    ShareCreated {
        created_item_kind: ik,
        created_item_id: iid,
        created_item_name: iname,
        created_to_kind: tk,
        created_to_id: tid,
        created_to_name: tname,
        summary: shared.summary(),
        item_id: shared.item,
        item_name: shared.name,
        target: shared.target.as_str().into(),
        to: shared.to,
        made: shared.made,
        new_share: shared.new,
    }
}

#[allow(clippy::too_many_arguments)]
fn opts_from(
    description: String,
    lang: Option<String>,
    code_from: Option<String>,
    space_kind: Option<String>,
    main: Option<String>,
    format: Option<String>,
    query: Option<String>,
    journal: Option<String>,
    body: Option<String>,
    model: Option<String>,
    peer: Option<String>,
    hours: Option<u64>,
) -> anyhow::Result<CreateOpts> {
    Ok(CreateOpts {
        description,
        lang: lang.as_deref().map(|s| Lang::parse(s).ok_or_else(|| anyhow::anyhow!("unknown language {s:?}"))).transpose()?,
        from: code_from,
        space_kind: space_kind.as_deref().map(|k| SpaceKind::from_arg(k).map_err(|e| anyhow::anyhow!("{e}"))).transpose()?,
        main,
        format: format.as_deref().map(|f| EntryFormat::parse(f).map_err(|e| anyhow::anyhow!("{e}"))).transpose()?,
        query,
        journal,
        body,
        model,
        peer,
        hours,
    })
}

/// Create a shareable item (`mnnn … --create`).
#[allow(clippy::too_many_arguments)]
pub fn share_create_item(
    kind: String,
    name: String,
    description: String,
    lang: Option<String>,
    code_from: Option<String>,
    space_kind: Option<String>,
    main: Option<String>,
    format: Option<String>,
    query: Option<String>,
) -> anyhow::Result<ShareCreated> {
    let store = open_write()?;
    let opts = opts_from(description, lang, code_from, space_kind, main, format, query, None, None, None, None, None)?;
    let c = share::create_item(&store, parse_share_kind(&kind)?, &name, &opts, "app")?;
    Ok(ShareCreated {
        created_item_kind: c.kind.clone(),
        created_item_id: c.id.clone(),
        created_item_name: c.name.clone(),
        created_to_kind: String::new(),
        created_to_id: String::new(),
        created_to_name: String::new(),
        summary: format!("created {} {}", c.kind, c.name),
        item_id: c.id,
        item_name: c.name,
        target: String::new(),
        to: String::new(),
        made: String::new(),
        new_share: true,
    })
}

/// Create-then-share (`mnnn share KIND ITEM TARGET --create [--create-to NAME]`).
#[allow(clippy::too_many_arguments)]
pub fn share_create_then(
    kind: String,
    item: String,
    target: String,
    to: Vec<String>,
    note: String,
    create_item: bool,
    create_to: Option<String>,
    description: String,
    lang: Option<String>,
    code_from: Option<String>,
    space_kind: Option<String>,
    main: Option<String>,
    format: Option<String>,
    query: Option<String>,
    peer: Option<String>,
    hours: Option<u64>,
) -> anyhow::Result<ShareCreated> {
    let store = open_write()?;
    let opts = opts_from(description, lang, code_from, space_kind, main, format, query, None, None, None, peer, hours)?;
    let t = parse_target(&target)?;
    let k = parse_share_kind(&kind)?;
    let (item_c, to_c, shared) = if t == TargetKind::Net && create_to.is_some() {
        let kp = identity::load_or_create(&store)?;
        let p = paths(None)?;
        let prefs = Preferences::load_or_create(&p).unwrap_or_default();
        let relays = prefs.net.relays.clone();
        let addrs = local::offer_addrs(&p.home, &store, None);
        let label = prefs.net.label.clone();
        let net = NetCreate { node: None, kp: &kp, my_label: &label, addrs, relays: &relays };
        share::create_then_share(&store, k, &item, t, &to, &note, "app", create_item, create_to.as_deref(), &opts, Some(&net))?
    } else {
        share::create_then_share(&store, k, &item, t, &to, &note, "app", create_item, create_to.as_deref(), &opts, None)?
    };
    Ok(pack(item_c, to_c, shared))
}

/// Create a review subject then the app can call `review_request` (`--create` on every ReviewKind).
#[allow(clippy::too_many_arguments)]
pub fn share_create_review_subject(
    kind: String,
    name: String,
    description: String,
    journal: Option<String>,
    body: Option<String>,
    model: Option<String>,
    format: Option<String>,
    query: Option<String>,
) -> anyhow::Result<ShareCreated> {
    let store = open_write()?;
    let opts = opts_from(description, None, None, None, None, format, query, journal, body, model, None, None)?;
    let c = share::create_review_subject(&store, parse_review_kind(&kind)?, &name, &opts, "app")?;
    Ok(ShareCreated {
        created_item_kind: c.kind.clone(),
        created_item_id: c.id.clone(),
        created_item_name: c.name.clone(),
        created_to_kind: String::new(),
        created_to_id: String::new(),
        created_to_name: String::new(),
        summary: format!("created {} {}", c.kind, c.name),
        item_id: c.id,
        item_name: c.name,
        target: String::new(),
        to: String::new(),
        made: String::new(),
        new_share: true,
    })
}

/// Create a project item kind (`mnnn project add --create KIND NAME`), including `result`.
pub fn share_create_project_item(kind: String, name: String, description: String) -> anyhow::Result<ShareCreated> {
    let store = open_write()?;
    let opts = CreateOpts { description, ..Default::default() };
    let c = share::create_project_item(&store, parse_item_kind(&kind)?, &name, &opts, "app")?;
    Ok(ShareCreated {
        created_item_kind: c.kind.clone(),
        created_item_id: c.id.clone(),
        created_item_name: c.name.clone(),
        created_to_kind: String::new(),
        created_to_id: String::new(),
        created_to_name: String::new(),
        summary: format!("created {} {}", c.kind, c.name),
        item_id: c.id,
        item_name: c.name,
        target: String::new(),
        to: String::new(),
        made: String::new(),
        new_share: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::Store;

    #[test]
    fn create_then_share_to_new_project_of() {
        let s = Store::open_in_memory().unwrap();
        let opts = CreateOpts::default();
        let (item, to, shared) =
            share::create_then_share(&s, ShareKind::Journal, "Notes", TargetKind::Project, &[], "", "app", true, Some("Alpha"), &opts, None).unwrap();
        let out = pack(item, to, shared);
        assert_eq!(out.created_item_name, "Notes");
        assert_eq!(out.created_to_name, "Alpha");
        assert!(out.new_share);
    }

    #[test]
    fn create_review_entry_of() {
        let s = Store::open_in_memory().unwrap();
        let c = share::create_review_subject(&s, ReviewKind::Entry, "Monday", &CreateOpts::default(), "app").unwrap();
        assert_eq!(c.kind, "entry");
    }

    #[test]
    fn writes_need_unlock() {
        assert!(share_create_item("journal".into(), "X".into(), String::new(), None, None, None, None, None, None).is_err());
        assert!(
            share_create_then(
                "journal".into(),
                "X".into(),
                "project".into(),
                vec![],
                String::new(),
                true,
                Some("P".into()),
                String::new(),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .is_err()
        );
    }
}
