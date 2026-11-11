//! Code spaces, live pipes and pipelines (`mnnn code` / `mnnn pipe` / `mnnn pipeline`
//! and the TUI Code / Pipes / Pipelines windows). Compiled plugin output and trust
//! hashes stay grounded in Rust; Dart sees names, source text the user wrote, status
//! and logs.
use super::data::{open, open_write};
use muninn_core::code::pipeline::GraphExt as _;
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

#[derive(Debug)]
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
pub fn code_create(
    name: String,
    lang: String,
    description: String,
    source: Option<String>,
    from: Option<String>,
    input: Option<String>,
) -> anyhow::Result<CodeSpaceInfo> {
    let store = open_write()?;
    let lang = if lang.trim().is_empty() {
        if let Some(f) = from.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            store
                .find_code_space(f)
                .map(|c| c.lang)
                .ok()
                .or_else(|| code::parsers::builtin(f).map(|_| Lang::Rust))
                .ok_or_else(|| anyhow::anyhow!("no code space or built-in `{f}`"))?
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
pub fn code_edit(
    space: String,
    name: Option<String>,
    description: Option<String>,
    lang: Option<String>,
    source: Option<String>,
    input: Option<String>,
) -> anyhow::Result<CodeSpaceInfo> {
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
        done.push(if new { format!("{} installed (approve it on first run)", c.name) } else { format!("{} already installed", c.name) });
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
                list.insert(
                    0,
                    LivePipeInfo {
                        token: token.clone(),
                        origin: a.origin.clone(),
                        status: "running".into(),
                        pipeline: String::new(),
                        hops: vec![],
                        saved: vec![],
                        updated: a.created_at,
                    },
                );
                list.truncate(KEEP_PIPES);
                0
            }
        };
        let p = &mut list[pos];
        p.updated = a.created_at;
        match a.kind.as_str() {
            "pipe.start" => {
                p.pipeline = a.params["pipeline"].as_str().unwrap_or_default().to_string();
                p.hops = strs(&a.params, "plan")
                    .into_iter()
                    .map(|n| LiveHopInfo {
                        name: n,
                        status: "waiting".into(),
                        rows: 0,
                        ms: 0,
                        columns: vec![],
                        error: String::new(),
                        state: String::new(),
                        rows_in: 0,
                    })
                    .collect();
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

fn nonempty(s: Option<String>) -> Option<String> {
    s.map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

fn stdin_body(stdin: Option<String>, stdin_file: Option<String>) -> anyhow::Result<String> {
    if let Some(path) = nonempty(stdin_file) {
        return std::fs::read_to_string(&path).map_err(|e| anyhow::anyhow!("{path}: {e}"));
    }
    Ok(stdin.unwrap_or_default())
}

fn chunk_frames(text: &str, chunk: usize, keep_header: bool) -> Vec<(String, String)> {
    if chunk == 0 {
        return vec![("stdin".into(), text.to_string())];
    }
    let mut head: Option<String> = None;
    let mut buf: Vec<String> = vec![];
    let mut out = Vec::new();
    let mut n = 0usize;
    let mut flush = |buf: &mut Vec<String>, head: &Option<String>| {
        if buf.is_empty() {
            return;
        }
        n += 1;
        let body = head.iter().cloned().chain(buf.drain(..)).collect::<Vec<_>>().join("\n");
        out.push((format!("stdin#{n}"), body));
    };
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if keep_header && head.is_none() && buf.is_empty() {
            head = Some(line.to_string());
            continue;
        }
        buf.push(line.to_string());
        if buf.len() >= chunk {
            flush(&mut buf, &head);
        }
    }
    flush(&mut buf, &head);
    if out.is_empty() {
        out.push(("stdin".into(), text.to_string()));
    }
    out
}

fn apply_pipe_format(store: &Store, src: &code::pipe::Source, format: Option<&str>, steps: &mut Vec<code::pipe::Step>) -> anyhow::Result<()> {
    let Some(f) = format.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    if !matches!(src, code::pipe::Source::Stdin) {
        return Err(anyhow::anyhow!("--format parses standard input: use it with `-`"));
    }
    let b = code::parsers::for_format(f).ok_or_else(|| anyhow::anyhow!("--format {f}: csv, json or ndjson (or put your own parser step first)"))?;
    let (s, _) = code::parsers::install_space(store, ORIGIN, b)?;
    steps.insert(0, code::pipe::Step::Space(s.name));
    Ok(())
}

fn approve_pipe_spaces(store: &Store, src: &code::pipe::Source, steps: &[code::pipe::Step]) -> anyhow::Result<()> {
    let mut names: Vec<String> =
        steps.iter().filter_map(|s| if let code::pipe::Step::Code(n) = s { Some(code::split_options(n).0) } else { None }).collect();
    if let code::pipe::Source::Code(n) = src {
        names.push(n.clone());
    }
    for n in &names {
        let c = store.find_code_space(n)?;
        approve_if_needed(store, &c)?;
    }
    let mut spaces: Vec<String> =
        steps.iter().filter_map(|s| if let code::pipe::Step::Space(n) = s { Some(code::split_options(n).0) } else { None }).collect();
    if let code::pipe::Source::Space(n) = src {
        spaces.push(code::split_options(n).0);
    }
    for n in &spaces {
        let s = store.find_space(n)?;
        if !code::spaces::approved(store, &s) {
            code::spaces::approve(store, ORIGIN, &s, true)?;
        }
    }
    Ok(())
}

fn pipe_result(
    store: &Store,
    env: &CodeEnv,
    src: &code::pipe::Source,
    steps: &[code::pipe::Step],
    to: &[code::pipe::Sink],
) -> anyhow::Result<PipeRunResult> {
    let mut extra = Vec::new();
    let p = code::pipe::run(store, env, ORIGIN, src, steps, to, &mut |l| extra.push(l.to_string()))?;
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

#[allow(clippy::too_many_arguments)]
fn pipe_run_of(
    store: &Store,
    source: &str,
    steps: &[String],
    to: &[String],
    stdin: Option<String>,
    stdin_file: Option<String>,
    format: Option<String>,
    chunk: u32,
    keep_header: bool,
) -> anyhow::Result<PipeRunResult> {
    let src = code::pipe::Source::parse(source.trim())?;
    let mut steps =
        steps.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(code::pipe::Step::parse).collect::<muninn_core::error::Result<Vec<_>>>()?;
    apply_pipe_format(store, &src, format.as_deref(), &mut steps)?;
    let to = to.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).map(code::pipe::Sink::parse).collect::<muninn_core::error::Result<Vec<_>>>()?;
    approve_pipe_spaces(store, &src, &steps)?;
    let env = CodeEnv::for_store(store)?;
    if !matches!(src, code::pipe::Source::Stdin) {
        return pipe_result(store, &env, &src, &steps, &to);
    }
    let text = stdin_body(stdin, stdin_file)?;
    if text.is_empty() {
        return Err(anyhow::anyhow!("standard input is empty: paste text or choose a file"));
    }
    let keep_header = keep_header || format.as_deref().is_some_and(|f| f.eq_ignore_ascii_case("csv"));
    let frames = chunk_frames(&text, chunk as usize, keep_header);
    let mut acc = PipeRunResult { hops: vec![], saved: vec![], lines: vec![] };
    for (label, body) in &frames {
        let frame_src = code::pipe::Source::Frame(code::parsers::raw_frame(body, label));
        let r = pipe_result(store, &env, &frame_src, &steps, &to)?;
        if frames.len() > 1 {
            acc.lines.push(format!("-- {label} --"));
        }
        acc.lines.extend(r.lines);
        acc.saved.extend(r.saved);
        acc.hops = r.hops;
    }
    Ok(acc)
}

/// `mnnn pipe SOURCE STEP… [--to TARGET] [--format csv|json|ndjson] [--chunk N]`.
/// Source `-` is standard input: paste text or pass a file path (Rust reads the bytes).
#[allow(clippy::too_many_arguments)]
pub fn pipe_run(
    source: String,
    steps: Vec<String>,
    to: Vec<String>,
    stdin: Option<String>,
    stdin_file: Option<String>,
    format: Option<String>,
    chunk: u32,
    keep_header: bool,
) -> anyhow::Result<PipeRunResult> {
    pipe_run_of(&open_write()?, &source, &steps, &to, stdin, stdin_file, format, chunk, keep_header)
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
    store.note_action(
        &NewAction::new("pipeline.create", ORIGIN, format!("pipeline {} created", p.name)).params(json!({ "pipeline": p.id, "graph": p.graph })),
    );
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
        &NewAction::new("pipeline.edit", ORIGIN, format!("pipeline {} edited (v{})", q.name, q.version))
            .params(json!({ "pipeline": q.id, "graph": q.graph })),
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

fn pipeline_run_once(store: &Store, env: &CodeEnv, p: &Pipeline, input: Option<serde_json::Value>) -> anyhow::Result<PipelineRunResult> {
    let mut extra = Vec::new();
    let r = pl::run(store, env, ORIGIN, p, input, &mut |l| extra.push(l.to_string()))?;
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

fn pipeline_run_of(
    store: &Store,
    pipeline: &str,
    stdin: Option<String>,
    stdin_file: Option<String>,
    chunk: u32,
    keep_header: bool,
) -> anyhow::Result<PipelineRunResult> {
    let p = store.find_pipeline(pipeline.trim())?;
    if p.draft {
        return Err(anyhow::anyhow!("{} is a draft from a peer: approve it before it runs", p.name));
    }
    for n in p.graph.code_spaces() {
        let c = store.find_code_space(&n)?;
        approve_if_needed(store, &c)?;
    }
    let env = CodeEnv::for_store(store)?;
    let wants_stdin = p.graph.nodes.iter().any(|n| matches!(&n.kind, pl::NodeKind::Source { source } if source == "-"));
    let text = stdin_body(stdin, stdin_file)?;
    if !wants_stdin {
        return pipeline_run_once(store, &env, &p, None);
    }
    if text.is_empty() {
        return Err(anyhow::anyhow!("{} reads standard input (`-`): paste text or choose a file", p.name));
    }
    let frames = chunk_frames(&text, chunk as usize, keep_header);
    let mut acc = PipelineRunResult { ok: true, lines: vec![], landed: vec![] };
    for (label, body) in &frames {
        let r = pipeline_run_once(store, &env, &p, Some(code::parsers::raw_frame(body, label)))?;
        if frames.len() > 1 {
            acc.lines.push(format!("-- {label} --"));
        }
        acc.lines.extend(r.lines);
        acc.landed.extend(r.landed);
        acc.ok = acc.ok && r.ok;
    }
    Ok(acc)
}

/// `mnnn pipeline run` (optional standard input when a source node is `-`).
pub fn pipeline_run(
    pipeline: String,
    stdin: Option<String>,
    stdin_file: Option<String>,
    chunk: u32,
    keep_header: bool,
) -> anyhow::Result<PipelineRunResult> {
    pipeline_run_of(&open_write()?, &pipeline, stdin, stdin_file, chunk, keep_header)
}

/// TUI designer `t`: also save this node's output to `journal:NAME` or `model:NAME`.
pub fn pipeline_tee(pipeline: String, node: String, sink: String) -> anyhow::Result<PipelineInfo> {
    pipeline_tee_of(&open_write()?, &pipeline, &node, &sink)
}

fn pipeline_tee_of(store: &Store, pipeline: &str, node: &str, sink: &str) -> anyhow::Result<PipelineInfo> {
    let p = store.find_pipeline(pipeline.trim())?;
    let sink = sink.trim();
    code::pipe::Sink::parse(sink)?;
    let node = node.trim();
    let mut g = p.graph.clone();
    let n = g.nodes.iter_mut().find(|n| n.id == node).ok_or_else(|| anyhow::anyhow!("no node `{node}` in {}", p.name))?;
    n.tees.push(sink.to_string());
    let q = store.update_pipeline(&p.id, None, None, Some(&g))?;
    store.note_action(
        &NewAction::new("pipeline.edit", ORIGIN, format!("pipeline {} also saves {} to {sink}", q.name, node))
            .params(json!({ "pipeline": q.id, "graph": q.graph })),
    );
    Ok(pipeline_info(store, q))
}

/// TUI designer `M`: end the node's branch in a model table, making the model when it is new.
pub fn pipeline_end_in_model(pipeline: String, node: String, model: String) -> anyhow::Result<PipelineInfo> {
    pipeline_end_in_model_of(&open_write()?, &pipeline, &node, &model)
}

fn pipeline_end_in_model_of(store: &Store, pipeline: &str, node: &str, model: &str) -> anyhow::Result<PipelineInfo> {
    let p = store.find_pipeline(pipeline.trim())?;
    let model = model.trim();
    if model.is_empty() {
        return Err(anyhow::anyhow!("a model name"));
    }
    let node = node.trim();
    let n = p.graph.node(node).ok_or_else(|| anyhow::anyhow!("no node `{node}` in {}", p.name))?.clone();
    if store.find_model(model).is_err() {
        store.create_model(model, &format!("rows of pipeline {}", p.name), muninn_core::store::Design::default(), true)?;
    }
    let mut id = format!("{}_out", n.id);
    while p.graph.node(&id).is_some() {
        id.push('_');
    }
    let mut g = p.graph.clone();
    g.add_node(&format!("{}={}", id, pl::NodeKind::Sink { model: model.to_string() }.text()))?;
    g.add_edges(&format!("{}>{id}", n.id))?;
    let q = store.update_pipeline(&p.id, None, None, Some(&g))?;
    store.note_action(
        &NewAction::new("pipeline.edit", ORIGIN, format!("pipeline {} ends {} in model {model}", q.name, node))
            .params(json!({ "pipeline": q.id, "graph": q.graph })),
    );
    Ok(pipeline_info(store, q))
}

pub struct PipelineMonitorNode {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub rows_in: i64,
    pub rows_out: i64,
    pub ms: i64,
}

pub struct PipelineMonitor {
    pub pipeline: String,
    pub state: String,
    pub nodes: Vec<PipelineMonitorNode>,
    pub landed: Vec<String>,
}

/// TUI monitor tile: each node's status, rows and what landed in the latest run.
pub fn pipeline_monitor(pipeline: String) -> anyhow::Result<PipelineMonitor> {
    pipeline_monitor_of(&open()?, &pipeline)
}

fn pipeline_monitor_of(store: &Store, pipeline: &str) -> anyhow::Result<PipelineMonitor> {
    let p = store.find_pipeline(pipeline.trim())?;
    let list = live_pipes_of(store)?;
    let run = list.iter().find(|r| r.pipeline == p.name);
    let order = p.graph.order().unwrap_or_else(|_| p.graph.nodes.iter().map(|n| n.id.clone()).collect());
    let mut first_open = true;
    let mut nodes = Vec::new();
    for id in order {
        let Some(n) = p.graph.node(&id) else { continue };
        let mut s = PipelineMonitorNode { id: n.id.clone(), kind: n.kind.text(), status: "idle".into(), rows_in: 0, rows_out: 0, ms: 0 };
        if let Some(r) = run {
            match r.hops.iter().find(|h| h.name.starts_with(&format!("{} (", n.id)) && h.status != "waiting") {
                Some(h) => {
                    s.status = if h.state.is_empty() { h.status.clone() } else { h.state.clone() };
                    s.rows_in = h.rows_in;
                    s.rows_out = h.rows;
                    s.ms = h.ms;
                }
                None if r.status == "running" && first_open => {
                    s.status = "running".into();
                    first_open = false;
                }
                None => {}
            }
        }
        nodes.push(s);
    }
    Ok(PipelineMonitor {
        pipeline: p.name,
        state: run.map(|r| r.status.clone()).unwrap_or_else(|| "not run yet".into()),
        nodes,
        landed: run.map(|r| r.saved.clone()).unwrap_or_default(),
    })
}

pub struct PipeFrameInfo {
    pub key: String,
    pub columns: Vec<String>,
    pub cells: Vec<String>,
    pub row_count: u32,
}

fn frame_cell(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn latest_pipe_frame_of(store: &Store, key: &str) -> anyhow::Result<Option<PipeFrameInfo>> {
    let acts = store.actions_of_kind("pipe.frame")?;
    let mut frame = None;
    for a in acts {
        if a.params.get("key").and_then(|v| v.as_str()) == Some(key) {
            frame = a.params.get("frame").cloned();
        }
    }
    let Some(f) = frame else { return Ok(None) };
    let cols = muninn_core::tui::frametile::columns(&f);
    let n = muninn_core::tui::frametile::rows(&f);
    let mut cells = Vec::with_capacity(cols.len() * n);
    for i in 0..n {
        for c in &cols {
            let col = f.get("columns").and_then(|x| x.get(c)).and_then(|x| x.as_array());
            cells.push(col.and_then(|a| a.get(i)).map(frame_cell).unwrap_or_default());
        }
    }
    Ok(Some(PipeFrameInfo { key: key.to_string(), columns: cols, cells, row_count: n as u32 }))
}

/// Latest `pipe.frame` for a data tile key (`code:SPACE` or `view:PIPELINE/NODE`).
pub fn latest_pipe_frame(key: String) -> anyhow::Result<Option<PipeFrameInfo>> {
    latest_pipe_frame_of(&open()?, &key)
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

/// `mnnn spaces export`: a code space file (main, members, imported spaces, their code).
pub fn space_export(space: String, path: String) -> anyhow::Result<String> {
    let store = open()?;
    let b = code::space_share::export(&store, space.trim())?;
    std::fs::write(&path, serde_json::to_vec_pretty(&b)?)?;
    store.note_action(
        &NewAction::new("space.export", ORIGIN, format!("code space {} exported", b.name)).params(json!({ "space": b.name, "file": path })),
    );
    Ok(format!("wrote {path} ({} code space(s), {} code)", b.spaces.len(), b.code.len()))
}

/// `mnnn spaces import --dry-run`: what importing would do.
pub fn space_import_plan(path: String, name: Option<String>) -> anyhow::Result<Vec<String>> {
    let store = open()?;
    let b = code::space_share::Bundle::parse(&std::fs::read(&path)?)?;
    Ok(code::space_share::plan(&store, &b, name.as_deref())?.lines())
}

/// `mnnn spaces import`: lands unapproved (pins kept; a clash is NAME-imported).
pub fn space_import(path: String, name: Option<String>) -> anyhow::Result<String> {
    let store = open_write()?;
    let b = code::space_share::Bundle::parse(&std::fs::read(&path)?)?;
    let plan = code::space_share::plan(&store, &b, name.as_deref())?;
    let s = code::space_share::import(&store, &b, &plan)?;
    store.note_action(
        &NewAction::new("space.import", ORIGIN, format!("code space {} imported", s.name)).params(json!({ "space": s.id, "file": path })),
    );
    Ok(format!("imported code space {}; approve it before it runs", s.name))
}

/// A plugin: a code space by kind (Plugins ▸ KIND).
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub name: String,
    pub kind: String,
    /// main's language
    pub lang: String,
    pub main: String,
    pub approved: bool,
    pub enabled: bool,
}

fn plugins_of(store: &muninn_core::store::Store, kind: Option<&str>) -> anyhow::Result<Vec<PluginInfo>> {
    let want = match kind.map(str::trim).filter(|k| !k.is_empty()) {
        Some(k) => Some(
            muninn_core::store::SpaceKind::parse(k).ok_or_else(|| anyhow::anyhow!("kind `{k}` (parser, transform, analysis, ingest-connector)"))?,
        ),
        None => None,
    };
    let mut v = vec![];
    for s in store.spaces()?.into_iter().filter(|s| want.is_none_or(|k| s.kind == k)) {
        let main = store.code_space(&s.main_code_id)?;
        v.push(PluginInfo {
            name: s.name.clone(),
            kind: s.kind.as_str().into(),
            lang: main.as_ref().map(|c| c.lang.as_str().to_string()).unwrap_or_default(),
            main: main.map(|c| c.name).unwrap_or_default(),
            approved: code::spaces::approved(store, &s),
            enabled: s.enabled,
        });
    }
    Ok(v)
}

/// `mnnn plugins list [--kind K]`.
pub fn plugins_list(kind: Option<String>) -> anyhow::Result<Vec<PluginInfo>> {
    plugins_of(&open()?, kind.as_deref())
}

/// `mnnn plugins enable|disable NAME`.
pub fn plugin_set_enabled(name: String, enabled: bool) -> anyhow::Result<String> {
    let store = open_write()?;
    let s = store.find_space(name.trim())?;
    store.set_space_enabled(&s.id, enabled)?;
    code::spaces::note(&store, ORIGIN, &s, if enabled { "enabled" } else { "disabled" }, json!({ "enabled": enabled }));
    Ok(format!("{} plugin {}", if enabled { "enabled" } else { "disabled" }, s.name))
}

pub(crate) fn plugin_install_of(store: &Store, what: &str, kind: &str, name: &str) -> anyhow::Result<String> {
    let kind = muninn_core::store::SpaceKind::from_arg(kind.trim())?;
    let what = what.trim();
    if what.is_empty() {
        anyhow::bail!("give a library path or an existing code name");
    }
    let path = std::path::Path::new(what);
    let c = if path.is_file() || what.contains(std::path::MAIN_SEPARATOR) {
        let abs = std::fs::canonicalize(path).map_err(|e| anyhow::anyhow!("{what}: {e}"))?;
        let stem = abs.file_stem().and_then(|s| s.to_str()).unwrap_or("plugin").trim_start_matches("lib").to_string();
        let cname = if name.trim().is_empty() { stem } else { name.trim().to_string() };
        code::create(store, ORIGIN, &cname, "", muninn_core::store::Lang::Plugin, &abs.to_string_lossy(), Default::default())?
    } else {
        store.find_code_space(what)?
    };
    let sname = if name.trim().is_empty() { c.name.clone() } else { name.trim().to_string() };
    let s = code::spaces::create(store, ORIGIN, &sname, "", kind, &c)?;
    store.note_action(
        &NewAction::new("plugins.install", ORIGIN, format!("install plugin {} ({})", s.name, s.kind.as_str()))
            .params(json!({ "space": s.id, "code": c.id, "kind": s.kind.as_str() })),
    );
    Ok(format!("installed plugin {} ({}, main {}); review and approve it", s.name, s.kind.as_str(), c.name))
}

/// `mnnn plugins install PATH|CODE --kind KIND`.
pub fn plugin_install(what: String, kind: String, name: String) -> anyhow::Result<String> {
    plugin_install_of(&open_write()?, &what, &kind, &name)
}

/// One place the last run of a code unit can go (`>` in the TUI Code window).
pub struct PipeTarget {
    pub kind: String,
    pub name: String,
}

pub(crate) fn code_pipe_targets_of(store: &Store, space: &str) -> anyhow::Result<Vec<PipeTarget>> {
    let c = store.find_code_space(space.trim())?;
    let mut out = Vec::new();
    for x in store.code_spaces()? {
        if x.id != c.id {
            out.push(PipeTarget { kind: "code".into(), name: x.name });
        }
    }
    for s in store.spaces()? {
        out.push(PipeTarget { kind: "space".into(), name: s.name });
    }
    for j in store.journals()? {
        out.push(PipeTarget { kind: "journal".into(), name: j.name });
    }
    for m in store.models()? {
        if !m.is_bars() && m.version > 0 {
            out.push(PipeTarget { kind: "model".into(), name: m.name });
        }
    }
    for p in store.projects()? {
        out.push(PipeTarget { kind: "project".into(), name: p.name });
    }
    for o in store.orgs()? {
        out.push(PipeTarget { kind: "org".into(), name: o.slug });
    }
    out.push(PipeTarget { kind: "net".into(), name: "all exchanges".into() });
    Ok(out)
}

/// Targets for piping a code unit's last result onward.
pub fn code_pipe_targets(space: String) -> anyhow::Result<Vec<PipeTarget>> {
    code_pipe_targets_of(&open()?, &space)
}

pub(crate) fn code_pipe_to_of(store: &Store, space: &str, kind: &str, name: &str) -> anyhow::Result<String> {
    let c = store.find_code_space(space.trim())?;
    let info = c.last_run.clone().ok_or_else(|| anyhow::anyhow!("run {} first", c.name))?;
    let frame = code::pipe::output(&c, &info)?;
    let kind = kind.trim();
    let name = name.trim();
    let env = CodeEnv::for_store(store)?;
    if kind == "code" {
        let next = store.find_code_space(name)?;
        let mut logs = Vec::new();
        let piped =
            code::pipe::run(store, &env, ORIGIN, &code::pipe::Source::Frame(frame), &[code::pipe::Step::Code(next.id.clone())], &[], &mut |l| {
                logs.push(l.to_string());
            })?;
        let status = piped.runs.into_iter().next_back().map(|(_, i)| i.status).unwrap_or_else(|| "ok".into());
        return Ok(format!("piped {} into {} ({status})", c.name, next.name));
    }
    if kind == "space" {
        let next = store.find_space(name)?;
        if !code::spaces::approved(store, &next) {
            anyhow::bail!("approve {} first, then pipe into it", next.name);
        }
        let mut logs = Vec::new();
        let info = code::spaces::run(store, &env, ORIGIN, &next, Some(frame), "the app", &mut |l| logs.push(l.to_string()))?;
        return Ok(format!("piped {} into code space {} ({})", c.name, next.name, info.status));
    }
    let sink = match kind {
        "journal" => code::pipe::Sink::Journal(name.into()),
        "project" => code::pipe::Sink::Project(name.into(), None),
        "org" => code::pipe::Sink::Org(name.into(), None),
        "net" => code::pipe::Sink::Net(None, None),
        "model" => code::pipe::Sink::Model(name.into()),
        _ => anyhow::bail!("pipe into code, space, journal, model, project, org or net"),
    };
    let made = code::pipe::save(store, ORIGIN, &sink, &frame, Some((&c, &info)), &format!("code:{} | tee {}", c.name, sink.text()))?;
    Ok(format!("saved the result of {} to {} ({made})", c.name, sink.text()))
}

/// Pipe the code unit's last result into another code unit, a code space, or a save (`>`).
pub fn code_pipe_to(space: String, kind: String, name: String) -> anyhow::Result<String> {
    code_pipe_to_of(&open_write()?, &space, &kind, &name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::Store;
    use serde_json::json;

    #[test]
    fn plugins_list_by_kind() {
        let s = Store::open_in_memory().unwrap();
        let c = s.create_code_space("p", "", muninn_core::store::Lang::Math, "y = 1", Default::default()).unwrap();
        code::spaces::create(&s, "test", "pp", "", muninn_core::store::SpaceKind::Parser, &c).unwrap();
        assert_eq!(plugins_of(&s, Some("parser")).unwrap()[0].name, "pp");
        assert!(plugins_of(&s, Some("analysis")).unwrap().is_empty());
        assert_eq!(plugins_of(&s, None).unwrap()[0].lang, "math");
        assert!(plugins_of(&s, Some("nope")).is_err());
        let msg = plugin_install_of(&s, "p", "analysis", "pa").unwrap();
        assert!(msg.contains("installed plugin pa"));
        assert!(msg.contains("review and approve"));
        let targets = code_pipe_targets_of(&s, "p").unwrap();
        assert!(targets.iter().any(|t| t.kind == "space" && t.name == "pp"));
        assert!(targets.iter().any(|t| t.kind == "net"));
        assert!(code_pipe_to_of(&s, "p", "journal", "notes").unwrap_err().to_string().contains("run p first"));
    }

    #[test]
    fn latest_pipe_frame_reads_the_last_code_result() {
        let s = Store::open_in_memory().unwrap();
        assert!(latest_pipe_frame_of(&s, "code:mean").unwrap().is_none());
        let frame = json!({
            "fields": [{"name": "ts"}, {"name": "px"}],
            "columns": {"ts": [1, 2], "px": [10.5, 11.0]}
        });
        muninn_core::code::pipe::note_frame(&s, "test", "code:mean", &frame);
        let got = latest_pipe_frame_of(&s, "code:mean").unwrap().expect("frame");
        assert_eq!(got.columns, vec!["ts", "px"]);
        assert_eq!(got.row_count, 2);
        assert_eq!(got.cells, vec!["1", "10.5", "2", "11.0"]);
        assert!(latest_pipe_frame_of(&s, "code:other").unwrap().is_none());
    }

    #[test]
    fn code_create_edit_remove_without_running() {
        let s = Store::open_in_memory().unwrap();
        assert!(code_spaces_of(&s).unwrap().is_empty());
        let c =
            code::create(&s, "test", "mean", "avg close", Lang::Math, code::template(Lang::Math), muninn_core::store::CodeInput::default()).unwrap();
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
        let g = pl::Graph::parse(
            &["src=source:coinbase".into(), "bars=code:to-bars".into(), "out=model:bars".into()],
            &["src>bars".into(), "bars>out".into()],
        )
        .unwrap();
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

    /// The code home is process-wide: tests that change it take turns.
    fn home_turn() -> std::sync::MutexGuard<'static, ()> {
        static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
        TURN.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A code home for one test, in a temp dir. Dropping it (also while a panic unwinds) puts
    /// the home before back, removes the dir, then gives up the turn, in that order.
    struct CodeHome {
        before: Option<std::path::PathBuf>,
        dir: tempfile::TempDir,
        _turn: std::sync::MutexGuard<'static, ()>,
    }

    impl CodeHome {
        /// A new home in a temp dir, set while `turn` is held.
        fn new(turn: std::sync::MutexGuard<'static, ()>) -> CodeHome {
            let dir = tempfile::tempdir().unwrap();
            let before = muninn_core::code::replace_home(Some(dir.path()));
            CodeHome { before, dir, _turn: turn }
        }
        fn path(&self) -> &std::path::Path {
            self.dir.path()
        }
    }

    impl Drop for CodeHome {
        fn drop(&mut self) {
            muninn_core::code::replace_home(self.before.as_deref());
        }
    }

    fn with_code_home() -> CodeHome {
        CodeHome::new(home_turn())
    }

    #[test]
    fn a_code_home_puts_back_the_home_that_was_set_before_it() {
        let turn = home_turn();
        let outer = tempfile::tempdir().unwrap();
        let original = muninn_core::code::replace_home(Some(outer.path()));
        let inner = CodeHome::new(turn);
        let path = inner.path().to_path_buf();
        assert_eq!(inner.before.as_deref(), Some(outer.path()));
        drop(inner); // puts the outer home back, removes its dir, then gives up the turn
        let _turn = home_turn();
        assert!(!path.exists(), "the inner home dir is gone");
        assert!(outer.path().is_dir(), "the outer home is left alone");
        let now = muninn_core::code::replace_home(original.as_deref());
        assert_eq!(now.as_deref(), Some(outer.path()), "the outer home is set again");
    }

    #[test]
    fn a_code_home_is_put_back_and_removed_even_when_a_test_panics() {
        let path = std::panic::catch_unwind(|| {
            let home = with_code_home();
            std::panic::panic_any(home.path().to_path_buf());
        })
        .unwrap_err()
        .downcast::<std::path::PathBuf>()
        .unwrap();
        assert!(!path.exists(), "the home dir was removed while unwinding");
        // the turn is free again even though the panic poisoned it, and the home before is back
        let _turn = home_turn();
        let now = muninn_core::code::replace_home(None);
        muninn_core::code::replace_home(now.as_deref());
        assert_eq!(now, None, "no home is left set after the test");
    }

    #[test]
    fn pipe_stdin_is_a_raw_text_frame_and_format_needs_stdin() {
        let _home = with_code_home();
        let s = Store::open_in_memory().unwrap();
        let r = pipe_run_of(&s, "-", &[], &[], Some("hello\nworld".into()), None, None, 0, false).unwrap();
        let text = r.lines.join("\n");
        assert!(text.contains("hello") && text.contains("world"), "{text}");
        let err = pipe_run_of(&s, "source:coinbase", &[], &[], None, None, Some("csv".into()), 0, false).unwrap_err().to_string();
        assert!(err.contains("--format"), "{err}");
        let err = pipe_run_of(&s, "-", &[], &[], Some("".into()), None, None, 0, false).unwrap_err().to_string();
        assert!(err.contains("empty"), "{err}");
    }

    #[test]
    fn pipe_chunk_and_stdin_file_match_cli() {
        let _home = with_code_home();
        let s = Store::open_in_memory().unwrap();
        let r = pipe_run_of(&s, "-", &[], &[], Some("a\nb\nc\nd".into()), None, None, 2, false).unwrap();
        let text = r.lines.join("\n");
        assert!(text.contains("stdin#1") && text.contains("stdin#2"), "{text}");
        let dir = std::env::temp_dir();
        let path = dir.join("muninn-pipe-stdin-test.txt");
        std::fs::write(&path, "from-file\n").unwrap();
        let r = pipe_run_of(&s, "-", &[], &[], None, Some(path.to_string_lossy().into()), None, 0, false).unwrap();
        let text = r.lines.join("\n");
        assert!(text.contains("from-file"), "{text}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn pipeline_tee_end_in_model_and_monitor() {
        let s = Store::open_in_memory().unwrap();
        let g = pl::Graph::parse(&["src=source:coinbase".into(), "bars=code:to-bars".into()], &["src>bars".into()]).unwrap();
        let p = s.create_pipeline("feed", "ticks", &g).unwrap();
        let t = pipeline_tee_of(&s, &p.id, "src", "journal:Raw").unwrap();
        assert!(t.node_specs.iter().any(|n| n.contains("journal:Raw")), "{:?}", t.node_specs);
        let e = pipeline_end_in_model_of(&s, &p.id, "bars", "ticks").unwrap();
        assert!(e.node_specs.iter().any(|n| n.contains("model:ticks")), "{:?}", e.node_specs);
        assert!(e.edge_specs.iter().any(|x| x.contains("bars>")), "{:?}", e.edge_specs);
        assert!(s.find_model("ticks").is_ok());
        let m = pipeline_monitor_of(&s, &p.id).unwrap();
        assert_eq!(m.state, "not run yet");
        assert!(m.nodes.iter().any(|n| n.id == "src" && n.status == "idle"));
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
