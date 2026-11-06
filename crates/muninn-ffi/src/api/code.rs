//! Code spaces, live pipes and pipelines (`mnnn code` / `mnnn pipe` / `mnnn pipeline`
//! and the TUI Code / Pipes / Pipelines windows). Compiled plugin output and trust
//! hashes stay grounded in Rust; Dart sees names, source text the user wrote, status
//! and logs.
use super::data::{open, open_write};
use muninn_core::code::{self, CodeEnv, pipeline as pl};
use muninn_core::store::{CodeChange, CodeScope, CodeSpace, Lang, NewAction, Store, pipelines::Pipeline};
use serde_json::json;

const ORIGIN: &str = "app";
const KEEP_PIPES: usize = 20;

pub struct CodeSpaceInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub description: String,
    pub lang: String,
    pub source: String,
    pub input: String,
    pub scope: String,
    pub approved: bool,
    pub trust_hash: String,
    pub last_status: String,
    pub last_error: String,
    pub last_at: i64,
    pub last_ms: i64,
    pub last_writes: i64,
}

pub struct CodeParserInfo {
    pub name: String,
    pub about: String,
    pub installed: bool,
}

pub struct CodeRunResult {
    pub status: String,
    pub error: String,
    pub logs: Vec<String>,
    pub lines: Vec<String>,
    pub writes: i64,
    pub ms: i64,
}

pub struct LiveHopInfo {
    pub name: String,
    pub status: String,
    pub rows: i64,
    pub ms: i64,
    pub columns: Vec<String>,
    pub error: String,
    pub state: String,
    pub rows_in: i64,
}

pub struct LivePipeInfo {
    pub token: String,
    pub origin: String,
    pub status: String,
    pub pipeline: String,
    pub hops: Vec<LiveHopInfo>,
    pub saved: Vec<String>,
    pub updated: i64,
}

pub struct PipeRunResult {
    pub hops: Vec<String>,
    pub saved: Vec<String>,
    pub lines: Vec<String>,
}

pub struct PipelineInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub description: String,
    pub version: i64,
    pub draft: bool,
    pub nodes: i64,
    pub sinks: String,
    pub problems: Vec<String>,
    pub node_specs: Vec<String>,
    pub edge_specs: Vec<String>,
    pub graph_lines: Vec<String>,
}

pub struct PipelineRunResult {
    pub ok: bool,
    pub lines: Vec<String>,
    pub landed: Vec<String>,
}

fn parse_lang(s: &str) -> anyhow::Result<Lang> {
    Lang::parse(s).ok_or_else(|| anyhow::anyhow!("language `{s}`: math, polars, rust, python or plugin"))
}

fn hash16(c: &CodeSpace) -> String {
    code::trust_hash(c).map(|h| h.chars().take(16).collect()).unwrap_or_default()
}

fn scope_label(store: &Store, scopes: &std::collections::HashMap<String, CodeScope>, c: &CodeSpace) -> String {
    match scopes.get(&c.id) {
        Some(CodeScope::Organization { org_id }) => {
            format!("org:{}", store.org(org_id).ok().flatten().map(|o| o.slug).unwrap_or_else(|| org_id.clone()))
        }
        Some(other) => other.kind().to_string(),
        None => "user".into(),
    }
}

fn space_info(store: &Store, scopes: &std::collections::HashMap<String, CodeScope>, c: CodeSpace) -> CodeSpaceInfo {
    let last = c.last_run.clone();
    CodeSpaceInfo {
        short_id: c.short_id().to_string(),
        approved: code::approved(&c),
        trust_hash: hash16(&c),
        input: c.input.text(),
        scope: scope_label(store, scopes, &c),
        last_status: last.as_ref().map(|r| r.status.clone()).unwrap_or_default(),
        last_error: last.as_ref().and_then(|r| r.error.clone()).unwrap_or_default(),
        last_at: last.as_ref().map(|r| r.at).unwrap_or(0),
        last_ms: last.as_ref().map(|r| r.ms as i64).unwrap_or(0),
        last_writes: last.as_ref().map(|r| r.writes as i64).unwrap_or(0),
        id: c.id,
        name: c.name,
        description: c.description,
        lang: c.lang.as_str().into(),
        source: c.source,
    }
}

pub(crate) fn code_spaces_of(store: &Store) -> anyhow::Result<Vec<CodeSpaceInfo>> {
    let scopes = store.code_scopes()?;
    Ok(store.code_spaces()?.into_iter().map(|c| space_info(store, &scopes, c)).collect())
}

fn run_result(info: muninn_core::store::RunInfo) -> CodeRunResult {
    let lines = code::run_view(&info).map(|v| v.text_lines(30)).unwrap_or_default();
    CodeRunResult {
        status: info.status,
        error: info.error.unwrap_or_default(),
        logs: info.logs,
        lines,
        writes: info.writes as i64,
        ms: info.ms as i64,
    }
}

fn approve_if_needed(store: &Store, c: &CodeSpace) -> anyhow::Result<()> {
    if !code::approved(c) {
        code::approve(store, ORIGIN, c)?;
    }
    Ok(())
}

fn source_from(store: &Store, lang: Lang, from: Option<&str>, source: Option<&str>) -> anyhow::Result<String> {
    if let Some(s) = source.map(str::trim).filter(|s| !s.is_empty()) {
        return Ok(s.to_string());
    }
    if let Some(f) = from.map(str::trim).filter(|s| !s.is_empty()) {
        if let Ok(c) = store.find_code_space(f) {
            return Ok(c.source);
        }
        if let Some(b) = code::parsers::builtin(f) {
            return Ok(b.source.to_string());
        }
        return Err(anyhow::anyhow!("no code space or built-in `{f}`"));
    }
    Ok(code::template(lang).to_string())
}

/// `mnnn code list` / TUI Code spaces….
pub fn code_spaces() -> anyhow::Result<Vec<CodeSpaceInfo>> {
    code_spaces_of(&open()?)
}

/// `mnnn code create` / TUI `n`. [from] is an existing space or built-in parser.
pub fn code_create(name: String, lang: String, description: String, source: Option<String>, from: Option<String>, input: Option<String>) -> anyhow::Result<CodeSpaceInfo> {
    let store = open_write()?;
    let lang = if lang.trim().is_empty() {
        if let Some(f) = from.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            store.find_code_space(f).map(|c| c.lang).ok().or_else(|| code::parsers::builtin(f).map(|_| Lang::Rust)).ok_or_else(|| anyhow::anyhow!("no code space or built-in `{f}`"))?
        } else {
            return Err(anyhow::anyhow!("pick a language (math, polars, rust, python or plugin)"));
        }
    } else {
        parse_lang(&lang)?
    };
    let src = source_from(&store, lang, from.as_deref(), source.as_deref())?;
    let input = code::parse_input(input.as_deref().unwrap_or("none"))?;
    let c = code::create(&store, ORIGIN, &name, &description, lang, &src, input)?;
    let scopes = store.code_scopes()?;
    Ok(space_info(&store, &scopes, c))
}

/// `mnnn code edit` / TUI editor save. A changed source needs approving again.
pub fn code_edit(space: String, name: Option<String>, description: Option<String>, lang: Option<String>, source: Option<String>, input: Option<String>) -> anyhow::Result<CodeSpaceInfo> {
    let store = open_write()?;
    let c = store.find_code_space(space.trim())?;
    let ch = CodeChange {
        name: name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        description,
        lang: lang.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(parse_lang).transpose()?,
        source,
        input: match input.as_deref().map(str::trim) {
            None => None,
            Some(s) => Some(code::parse_input(s)?),
        },
    };
    let c = code::edit(&store, ORIGIN, &c.id, &ch)?;
    let scopes = store.code_scopes()?;
    Ok(space_info(&store, &scopes, c))
}

/// `mnnn code remove` / TUI `d`.
pub fn code_remove(space: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let c = store.find_code_space(space.trim())?;
    let name = c.name.clone();
    code::remove(&store, ORIGIN, &c.id)?;
    Ok(format!("removed code space {name}"))
}

/// `mnnn code approve`.
pub fn code_approve(space: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let c = store.find_code_space(space.trim())?;
    if code::approved(&c) {
        return Ok(format!("code space {} is approved already", c.name));
    }
    code::approve(&store, ORIGIN, &c)?;
    Ok(format!("approved code space {}", c.name))
}

/// `mnnn code check SPACE`.
pub fn code_check(space: String) -> anyhow::Result<Vec<String>> {
    let store = open()?;
    let c = store.find_code_space(space.trim())?;
    let env = CodeEnv::for_store(&store)?;
    let mut lines = code::check(&env, c.lang, &c.source)?;
    lines.push(format!("approval: {}", if code::approved(&c) { "approved" } else { "not approved" }));
    Ok(lines)
}

/// `mnnn code build`.
pub fn code_build(space: String) -> anyhow::Result<CodeRunResult> {
    let store = open_write()?;
    let c = store.find_code_space(space.trim())?;
    let env = CodeEnv::for_store(&store)?;
    let mut logs = Vec::new();
    let b = code::build_space(&store, &env, ORIGIN, &c, &mut |l| logs.push(l.to_string()))?;
    let status = if b.cached { "already built" } else { "built" };
    Ok(CodeRunResult {
        status: status.into(),
        error: String::new(),
        logs,
        lines: vec![format!("{status} code space {} ({})", c.name, b.path.display())],
        writes: 0,
        ms: 0,
    })
}

/// `mnnn code run` / TUI `r`. Approves the current source first (the app asked).
pub fn code_run(space: String) -> anyhow::Result<CodeRunResult> {
    let store = open_write()?;
    let c = store.find_code_space(space.trim())?;
    approve_if_needed(&store, &c)?;
    let c = store.code_space(&c.id)?.ok_or_else(|| anyhow::anyhow!("code space {} vanished", c.name))?;
    let env = CodeEnv::for_store(&store)?;
    let mut extra = Vec::new();
    let mut info = code::run(&store, &env, ORIGIN, &c, &mut |l| extra.push(l.to_string()))?;
    if !extra.is_empty() {
        extra.append(&mut info.logs);
        info.logs = extra;
    }
    Ok(run_result(info))
}

/// `mnnn code parsers`.
pub fn code_parsers() -> anyhow::Result<Vec<CodeParserInfo>> {
    let store = open()?;
    Ok(code::parsers::BUILTINS
        .iter()
        .map(|b| CodeParserInfo { name: b.name.into(), about: b.about.into(), installed: store.find_code_space(b.name).is_ok() })
        .collect())
}

/// `mnnn code parsers --install` (one name, or every built-in when [name] is empty).
pub fn code_parsers_install(name: Option<String>) -> anyhow::Result<String> {
    let store = open_write()?;
    let want = name.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let mut done = Vec::new();
    for b in code::parsers::BUILTINS {
        if let Some(n) = want
            && !b.name.eq_ignore_ascii_case(n)
        {
            continue;
        }
        let (c, new) = code::parsers::install(&store, ORIGIN, b)?;
        done.push(if new {
            format!("{} installed (approve it on first run)", c.name)
        } else {
            format!("{} already installed", c.name)
        });
    }
    if done.is_empty() {
        return Err(anyhow::anyhow!("no built-in `{}` (see Code ▸ Parsers)", want.unwrap_or_default()));
    }
    Ok(done.join("; "))
}

fn strs(v: &serde_json::Value, k: &str) -> Vec<String> {
    v[k].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default()
}

fn live_pipes_of(store: &Store) -> anyhow::Result<Vec<LivePipeInfo>> {
    let mut acts = Vec::new();
    for k in ["pipe.start", "pipe.hop", "pipe.run"] {
        acts.extend(store.actions_of_kind(k)?);
    }
    acts.sort_by_key(|a| a.id);
    let mut list: Vec<LivePipeInfo> = Vec::new();
    for a in acts {
        let token = a.params["token"].as_str().unwrap_or_default().to_string();
        if token.is_empty() {
            continue;
        }
        let pos = match list.iter().position(|p| p.token == token) {
            Some(i) => i,
            None => {
                list.insert(0, LivePipeInfo {
                    token: token.clone(),
                    origin: a.origin.clone(),
                    status: "running".into(),
                    pipeline: String::new(),
                    hops: vec![],
                    saved: vec![],
                    updated: a.created_at,
                });
                list.truncate(KEEP_PIPES);
                0
            }
        };
        let p = &mut list[pos];
        p.updated = a.created_at;
        match a.kind.as_str() {
            "pipe.start" => {
                p.pipeline = a.params["pipeline"].as_str().unwrap_or_default().to_string();
                p.hops = strs(&a.params, "plan").into_iter().map(|n| LiveHopInfo {
                    name: n,
                    status: "waiting".into(),
                    rows: 0,
                    ms: 0,
                    columns: vec![],
                    error: String::new(),
                    state: String::new(),
                    rows_in: 0,
                }).collect();
            }
            "pipe.hop" => {
                let i = a.params["i"].as_u64().unwrap_or(0) as usize;
                while p.hops.len() <= i {
                    p.hops.push(LiveHopInfo {
                        name: String::new(),
                        status: "waiting".into(),
                        rows: 0,
                        ms: 0,
                        columns: vec![],
                        error: String::new(),
                        state: String::new(),
                        rows_in: 0,
                    });
                }
                let h = &mut p.hops[i];
                let hop = a.params["hop"].as_str().unwrap_or_default();
                if h.name.is_empty() || !hop.is_empty() {
                    h.name = hop.to_string();
                }
                h.status = a.params["status"].as_str().unwrap_or("ok").to_string();
                h.rows = a.params["rows"].as_u64().unwrap_or(0) as i64;
                h.ms = a.params["ms"].as_u64().unwrap_or(0) as i64;
                h.columns = strs(&a.params, "columns");
                h.error = a.params["error"].as_str().unwrap_or_default().to_string();
                h.state = a.params["state"].as_str().unwrap_or_default().to_string();
                h.rows_in = a.params["rows_in"].as_u64().unwrap_or(0) as i64;
                if h.status == "error" {
                    p.status = "error".into();
                }
            }
            _ => {
                if p.status != "error" {
                    p.status = "done".into();
                }
                p.saved = strs(&a.params, "saved");
                for h in p.hops.iter_mut().filter(|h| h.status == "waiting") {
                    h.status = "ok".into();
                }
            }
        }
    }
    Ok(list)
}

/// TUI Pipes…: live and recent pipes from history (`mnnn pipe` or a pipeline run).
pub fn pipes_live() -> anyhow::Result<Vec<LivePipeInfo>> {
    live_pipes_of(&open()?)
}

/// `mnnn pipe SOURCE STEP… [--to TARGET]`. Standard input (`-`) stays in the CLI.
pub fn pipe_run(source: String, steps: Vec<String>, to: Vec<String>) -> anyhow::Result<PipeRunResult> {
    let store = open_write()?;
    let src = code::pipe::Source::parse(source.trim())?;
    if matches!(src, code::pipe::Source::Stdin) {
        return Err(anyhow::anyhow!("standard input (`-`) stays in the CLI (`mnnn pipe - …`)"));
    }
    let steps = steps.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(code::pipe::Step::parse).collect::<muninn_core::error::Result<Vec<_>>>()?;
    let to = to.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(code::pipe::Sink::parse).collect::<muninn_core::error::Result<Vec<_>>>()?;
    let mut names: Vec<String> = steps.iter().filter_map(|s| if let code::pipe::Step::Code(n) = s { Some(code::split_options(n).0) } else { None }).collect();
    if let code::pipe::Source::Code(n) = &src {
        names.push(n.clone());
    }
    for n in &names {
        let c = store.find_code_space(n)?;
        approve_if_needed(&store, &c)?;
    }
    let env = CodeEnv::for_store(&store)?;
    let mut extra = Vec::new();
    let p = code::pipe::run(&store, &env, ORIGIN, &src, &steps, &to, &mut |l| extra.push(l.to_string()))?;
    let mut lines = extra;
    for (c, info) in &p.runs {
        if p.runs.len() > 1 {
            lines.push(format!("-- {} --", c.name));
        }
        lines.extend(info.logs.iter().cloned());
        if let Some(v) = code::run_view(info) {
            lines.extend(v.text_lines(30));
        }
        if let Some(e) = &info.error {
            lines.push(format!("{}: {e}", c.name));
        }
    }
    if p.runs.is_empty()
        && let Some(f) = &p.frame
    {
        lines.extend(code::pipe::markdown(f, 30).lines().map(str::to_string));
    }
    Ok(PipeRunResult { hops: p.hops, saved: p.saved, lines })
}

fn node_spec(n: &pl::Node) -> String {
    let mut s = format!("{}={}", n.id, n.kind.text());
    for t in &n.tees {
        s.push('+');
        s.push_str(t);
    }
    s
}

fn edge_spec(e: &pl::Edge) -> String {
    match &e.name {
        Some(n) => format!("{}>{}:{n}", e.from, e.to),
        None => format!("{}>{}", e.from, e.to),
    }
}

fn graph_lines(store: &Store, g: &pl::Graph) -> Vec<String> {
    let order = g.order().unwrap_or_else(|_| g.nodes.iter().map(|n| n.id.clone()).collect());
    let mut v = Vec::new();
    for id in order {
        let Some(n) = g.node(&id) else { continue };
        let downs: Vec<String> = g
            .downs(&id)
            .iter()
            .map(|e| match &e.name {
                Some(x) => format!("{} (as {x})", e.to),
                None => e.to.clone(),
            })
            .collect();
        let tees = if n.tees.is_empty() { String::new() } else { format!(" + {}", n.tees.join(" + ")) };
        let to = if downs.is_empty() { String::new() } else { format!(" → {}", downs.join(", ")) };
        v.push(format!("{id} = {}{tees}{to}", n.kind.text()));
    }
    for p in g.problems(Some(store)) {
        v.push(format!("! {p}"));
    }
    v
}

fn pipeline_info(store: &Store, p: Pipeline) -> PipelineInfo {
    let problems = p.graph.problems(Some(store));
    let node_specs = p.graph.nodes.iter().map(node_spec).collect();
    let edge_specs = p.graph.edges.iter().map(edge_spec).collect();
    let lines = graph_lines(store, &p.graph);
    PipelineInfo {
        short_id: muninn_core::store::projects::short_of(&p.id).to_string(),
        sinks: p.graph.sinks().join(", "),
        nodes: p.graph.nodes.len() as i64,
        version: p.version,
        draft: p.draft,
        problems,
        node_specs,
        edge_specs,
        graph_lines: lines,
        id: p.id,
        name: p.name,
        description: p.description,
    }
}

pub(crate) fn pipelines_of(store: &Store) -> anyhow::Result<Vec<PipelineInfo>> {
    Ok(store.pipelines()?.into_iter().map(|p| pipeline_info(store, p)).collect())
}

/// `mnnn pipeline list` / TUI Pipelines….
pub fn pipelines() -> anyhow::Result<Vec<PipelineInfo>> {
    pipelines_of(&open()?)
}

/// `mnnn pipeline create` / TUI `n`. Nodes are `ID=KIND`; edges are `FROM>TO`.
pub fn pipeline_create(name: String, description: String, nodes: Vec<String>, edges: Vec<String>) -> anyhow::Result<PipelineInfo> {
    let store = open_write()?;
    let nodes: Vec<String> = nodes.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let edges: Vec<String> = edges.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    let g = pl::Graph::parse(&nodes, &edges)?;
    let p = store.create_pipeline(&name, &description, &g)?;
    store.note_action(&NewAction::new("pipeline.create", ORIGIN, format!("pipeline {} created", p.name)).params(json!({ "pipeline": p.id, "graph": p.graph })));
    Ok(pipeline_info(&store, p))
}

/// `mnnn pipeline edit`.
pub fn pipeline_edit(
    pipeline: String,
    name: Option<String>,
    description: Option<String>,
    nodes: Vec<String>,
    edges: Vec<String>,
    remove_nodes: Vec<String>,
    remove_edges: Vec<String>,
) -> anyhow::Result<PipelineInfo> {
    let store = open_write()?;
    let p = store.find_pipeline(pipeline.trim())?;
    let mut g = p.graph.clone();
    for r in remove_nodes.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if !g.remove_node(r) {
            return Err(anyhow::anyhow!("no node `{r}` in {}", p.name));
        }
    }
    for r in remove_edges.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let (a, b) = r.split_once('>').ok_or_else(|| anyhow::anyhow!("`{r}`: an edge is FROM>TO"))?;
        g.edges.retain(|e| !(e.from == a.trim() && e.to == b.trim()));
    }
    for n in nodes.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        g.add_node(n)?;
    }
    for e in edges.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        g.add_edges(e)?;
    }
    let changed = (g != p.graph).then_some(&g);
    let q = store.update_pipeline(&p.id, name.as_deref().map(str::trim).filter(|s| !s.is_empty()), description.as_deref(), changed)?;
    store.note_action(
        &NewAction::new("pipeline.edit", ORIGIN, format!("pipeline {} edited (v{})", q.name, q.version)).params(json!({ "pipeline": q.id, "graph": q.graph })),
    );
    Ok(pipeline_info(&store, q))
}

/// `mnnn pipeline remove` / TUI `d`.
pub fn pipeline_remove(pipeline: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let p = store.find_pipeline(pipeline.trim())?;
    let name = p.name.clone();
    store.remove_pipeline(&p.id)?;
    store.note_action(&NewAction::new("pipeline.remove", ORIGIN, format!("pipeline {name} removed")).params(json!({ "pipeline": p.id })));
    Ok(format!("removed pipeline {name}"))
}

/// `mnnn pipeline approve` for a draft fetched from a peer.
pub fn pipeline_approve(pipeline: String) -> anyhow::Result<String> {
    let store = open_write()?;
    let p = store.find_pipeline(pipeline.trim())?;
    if !p.draft {
        return Ok(format!("{} is not a draft", p.name));
    }
    Ok(muninn_core::tui::designer::approve(&store, ORIGIN, &p)?)
}

/// `mnnn pipeline run` (no standard input; that stays in the CLI).
pub fn pipeline_run(pipeline: String) -> anyhow::Result<PipelineRunResult> {
    let store = open_write()?;
    let p = store.find_pipeline(pipeline.trim())?;
    if p.draft {
        return Err(anyhow::anyhow!("{} is a draft from a peer: approve it before it runs", p.name));
    }
    for n in p.graph.code_spaces() {
        let c = store.find_code_space(&n)?;
        approve_if_needed(&store, &c)?;
    }
    let env = CodeEnv::for_store(&store)?;
    let mut extra = Vec::new();
    let r = pl::run(&store, &env, ORIGIN, &p, None, &mut |l| extra.push(l.to_string()))?;
    let mut lines = extra;
    for n in &r.nodes {
        let extra = n.error.clone().unwrap_or_else(|| format!("{} → {} rows, {} ms", n.rows_in, n.rows_out, n.ms));
        lines.push(format!("{}  {}  {}  {extra}", n.id, n.node, n.status));
        for s in &n.saved {
            lines.push(format!("  saved {s}"));
        }
    }
    let landed = r.landed.iter().map(|(m, k)| format!("{k} rows → model {m}")).collect();
    Ok(PipelineRunResult { ok: r.ok(), lines, landed })
}

/// `mnnn pipeline export`. Bytes stay in Rust; Dart passes the path.
pub fn pipeline_export(pipeline: String, path: String, templates: bool) -> anyhow::Result<String> {
    let store = open()?;
    let e = pl::export(&store, pipeline.trim(), templates)?;
    std::fs::write(&path, serde_json::to_vec_pretty(&e)?)?;
    store.note_action(
        &NewAction::new("pipeline.export", ORIGIN, format!("pipeline {} exported", e.name)).params(json!({ "pipeline": e.name, "file": path })),
    );
    Ok(format!("wrote {path} ({} code spaces, {} models)", e.code.len(), e.models.len()))
}

/// Plan an import (`mnnn pipeline import --dry-run`). Bytes stay in Rust.
pub fn pipeline_import_plan(path: String, name: Option<String>, merge: bool) -> anyhow::Result<Vec<String>> {
    let store = open()?;
    let e = pl::Export::parse(&std::fs::read(&path)?)?;
    let plan = pl::plan_import(&store, &e, name.as_deref().map(str::trim).filter(|s| !s.is_empty()), merge)?;
    Ok(plan.lines())
}

/// `mnnn pipeline import`.
pub fn pipeline_import(path: String, name: Option<String>, merge: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let e = pl::Export::parse(&std::fs::read(&path)?)?;
    let plan = pl::plan_import(&store, &e, name.as_deref().map(str::trim).filter(|s| !s.is_empty()), merge)?;
    let p = pl::import(&store, &e, &plan)?;
    store.note_action(
        &NewAction::new("pipeline.import", ORIGIN, format!("pipeline {} imported (v{})", p.name, p.version))
            .params(json!({ "pipeline": p.id, "file": path, "plan": plan })),
    );
    Ok(format!("imported pipeline {} v{}; approve its code before it runs", p.name, p.version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::Store;

    #[test]
    fn code_create_edit_remove_without_running() {
        let s = Store::open_in_memory().unwrap();
        assert!(code_spaces_of(&s).unwrap().is_empty());
        let c = code::create(&s, "test", "mean", "avg close", Lang::Math, code::template(Lang::Math), muninn_core::store::CodeInput::default()).unwrap();
        let list = code_spaces_of(&s).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "mean");
        assert_eq!(list[0].lang, "math");
        assert!(!list[0].approved);
        assert!(list[0].source.contains("sma") || !list[0].source.is_empty());
        let n = code::edit(&s, "test", &c.id, &CodeChange { description: Some("updated".into()), ..Default::default() }).unwrap();
        assert_eq!(n.description, "updated");
        code::remove(&s, "test", &c.id).unwrap();
        assert!(code_spaces_of(&s).unwrap().is_empty());
    }

    #[test]
    fn pipeline_create_list_remove_without_running() {
        let s = Store::open_in_memory().unwrap();
        assert!(pipelines_of(&s).unwrap().is_empty());
        let g = pl::Graph::parse(&["src=source:coinbase".into(), "bars=code:to-bars".into(), "out=model:bars".into()], &["src>bars".into(), "bars>out".into()]).unwrap();
        let p = s.create_pipeline("bars-from-ingest", "raw data to bars", &g).unwrap();
        let list = pipelines_of(&s).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "bars-from-ingest");
        assert_eq!(list[0].nodes, 3);
        assert!(!list[0].draft);
        assert!(list[0].node_specs.iter().any(|n| n.starts_with("src=")));
        s.remove_pipeline(&p.id).unwrap();
        assert!(pipelines_of(&s).unwrap().is_empty());
    }

    #[test]
    fn live_pipes_empty_and_parsers_list() {
        let s = Store::open_in_memory().unwrap();
        assert!(live_pipes_of(&s).unwrap().is_empty());
        assert!(code::parsers::BUILTINS.iter().any(|b| b.name == "parse-csv" || b.name.contains("csv") || b.name.contains("to-bars")));
    }

    #[test]
    fn writes_need_unlock() {
        crate::api::data::lock();
        assert!(code_spaces().is_err());
        assert!(pipelines().is_err());
        assert!(pipes_live().is_err());
        assert!(code_create("x".into(), "math".into(), "".into(), None, None, None).is_err());
        assert!(pipeline_create("x".into(), "".into(), vec![], vec![]).is_err());
    }
}
