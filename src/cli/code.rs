//! `mnnn code`: code spaces (compiled plugins over your data). Thin over [`crate::code`].
use super::{CodeCommand, CodeInputArgs, CodeSourceArgs, Ctx};
use crate::code::{self, CodeEnv};
use crate::ingest::time::fmt_ts;
use crate::store::{CodeChange, CodeInput, CodeSpace, ItemKind, Lang, Store};
use anyhow::anyhow;
use serde_json::json;
use std::io::Read;

impl CodeInputArgs {
    fn given(&self) -> bool {
        self.model.is_some()
            || !self.filter.is_empty()
            || self.matching.is_some()
            || !self.group.is_empty()
            || !self.sort.is_empty()
            || self.limit.is_some()
    }
    fn input(&self) -> anyhow::Result<CodeInput> {
        let Some(model) = &self.model else {
            if self.given() {
                return Err(anyhow!("--where, --matching, --group, --sort and --limit go with --model"));
            }
            return Ok(CodeInput::default());
        };
        let split =
            |v: &[String], sep: char| v.iter().flat_map(|x| x.split(sep)).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>();
        let i = CodeInput {
            model: model.trim().to_string(),
            filter: split(&self.filter, ';'),
            sort: split(&self.sort, ','),
            group: split(&self.group, ','),
            text: self.matching.clone().unwrap_or_default(),
            limit: self.limit,
        };
        // the same checks as the TUI's one-line input
        code::parse_input(&i.text())?;
        Ok(i)
    }
}

/// The source given: a file, stdin, the text, the editor, or `None`.
fn source_of(src: &CodeSourceArgs, lang: Lang, current: Option<&str>) -> anyhow::Result<Option<String>> {
    if let Some(f) = &src.file {
        if f.as_os_str() == "-" {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s)?;
            return Ok(Some(s));
        }
        return Ok(Some(std::fs::read_to_string(f).map_err(|e| anyhow!("{}: {e}", f.display()))?));
    }
    if let Some(s) = &src.source {
        return Ok(Some(s.clone()));
    }
    if src.editor {
        return Ok(Some(in_editor(current.unwrap_or(code::template(lang)), lang)?));
    }
    Ok(None)
}

fn in_editor(text: &str, lang: Lang) -> anyhow::Result<String> {
    let editor =
        std::env::var("VISUAL").or_else(|_| std::env::var("EDITOR")).map_err(|_| anyhow!("set $VISUAL or $EDITOR (or give --file or --source)"))?;
    let path = std::env::temp_dir().join(format!("mnnn-code-{}.{}", std::process::id(), lang.ext()));
    std::fs::write(&path, text)?;
    let mut parts = editor.split_whitespace();
    let prog = parts.next().ok_or_else(|| anyhow!("$EDITOR is empty"))?;
    let status = std::process::Command::new(prog).args(parts).arg(&path).status();
    let out = std::fs::read_to_string(&path);
    let _ = std::fs::remove_file(&path);
    if !status?.success() {
        return Err(anyhow!("the editor failed; nothing was changed"));
    }
    Ok(out?)
}

fn status_of(c: &CodeSpace) -> &'static str {
    match (code::approved(c), c.approved_hash.is_some()) {
        (true, _) => "approved",
        (false, true) => "changed since approved",
        (false, false) => "not approved",
    }
}

fn last_run(c: &CodeSpace) -> String {
    match &c.last_run {
        None => "never run".into(),
        Some(r) => {
            format!("{} {} ({} ms{})", r.status, fmt_ts(r.at), r.ms, if r.writes > 0 { format!(", {} writes", r.writes) } else { String::new() })
        }
    }
}

/// Approve (after asking) when the source is new or changed.
fn ensure_approved(ctx: &Ctx, store: &Store, c: &CodeSpace) -> anyhow::Result<()> {
    if code::approved(c) {
        return Ok(());
    }
    let hash = code::trust_hash(c)?;
    eprintln!("code space {} ({}) is {}; it runs as native code with your rights.", c.name, c.lang.as_str(), status_of(c));
    eprintln!("review it with `mnnn code show {}` (trust hash {})", c.name, &hash[..16]);
    if !ctx.io.confirm(&format!("approve code space `{}` and run it?", c.name))? {
        return Err(crate::Error::Aborted.into());
    }
    code::approve(store, "cli", c)?;
    Ok(())
}

fn print_run(info: &crate::store::RunInfo, rows: usize) {
    for l in &info.logs {
        eprintln!("{l}");
    }
    if let Some(v) = code::run_view(info) {
        for l in v.text_lines(if rows == 0 { usize::MAX } else { rows }) {
            println!("{l}");
        }
    }
}

pub fn run(ctx: &Ctx, cmd: CodeCommand) -> anyhow::Result<()> {
    if let CodeCommand::Sdk { out } = &cmd {
        code::write_sdk(out)?;
        ctx.io.info(format!("wrote the muninn-plugin SDK {} (ABI {}) to {}", code::SDK_VERSION, code::abi::ABI_VERSION, out.display()));
        return Ok(());
    }
    if let CodeCommand::Check { space: None } = &cmd {
        let mut ok = true;
        for (name, st) in code::toolchain_status() {
            match st {
                Ok(v) => println!("{name:<8} {v}"),
                Err(e) => {
                    ok &= name != "cargo";
                    println!("{name:<8} missing: {e}");
                }
            }
        }
        println!("ABI      {} (SDK {})", code::abi::ABI_VERSION, code::SDK_VERSION);
        if !ok {
            ctx.io.note("without cargo only prebuilt plugins (--lang plugin) run; see `mnnn help code-spaces`");
        }
        return Ok(());
    }
    let store = ctx.open_store()?;
    match cmd {
        CodeCommand::Create { name, lang, source, input, description, project } => {
            let lang = lang.lang();
            let src = source_of(&source, lang, None)?.unwrap_or_else(|| code::template(lang).to_string());
            let p = project.map(|p| store.find_project(&p)).transpose()?;
            let c = code::create(&store, "cli", &name, &description, lang, &src, input.input()?)?;
            ctx.io.info(format!("created code space {} ({}); review and run it with `mnnn code run {} -y`", c.name, lang.as_str(), c.name));
            if let Some(p) = p {
                let (it, _) = store.add_project_item(&p.id, ItemKind::Code, &c.id, json!({}))?;
                ctx.io.info(format!("added it to project {}", p.name));
                super::searches::record(
                    &store,
                    "project.add",
                    format!("add code space {} to project {}", c.name, p.name),
                    json!({ "project": p.id, "item": it.id, "kind": it.kind, "ref": it.ref_id }),
                );
            }
        }
        CodeCommand::Edit { space, name, description, lang, source, input, no_input } => {
            let c = store.find_code_space(&space)?;
            let lang = lang.map(|l| l.lang());
            let ch = CodeChange {
                name,
                description,
                lang,
                source: source_of(&source, lang.unwrap_or(c.lang), Some(&c.source))?,
                input: if no_input {
                    Some(CodeInput::default())
                } else if input.given() {
                    Some(input.input()?)
                } else {
                    None
                },
            };
            let n = code::edit(&store, "cli", &c.id, &ch)?;
            ctx.io.info(format!("updated code space {} ({})", n.name, status_of(&n)));
        }
        CodeCommand::List { scope, json } => {
            let scopes = store.code_scopes()?;
            let want = scope.as_deref().map(|f| scope_filter(&store, f)).transpose()?;
            let scope_of = |c: &crate::store::CodeSpace| scopes.get(&c.id).cloned().unwrap_or(crate::store::CodeScope::User);
            let v: Vec<_> = store.code_spaces()?.into_iter().filter(|c| want.as_ref().is_none_or(|w| w.matches(&scope_of(c)))).collect();
            let label = |sc: &crate::store::CodeScope| match sc.org_id() {
                Some(o) => format!("org:{}", store.org(o).ok().flatten().map(|o| o.slug).unwrap_or_else(|| o.to_string())),
                None => sc.kind().to_string(),
            };
            if json {
                let out: Vec<_> = v
                    .iter()
                    .map(|c| json!({ "id": c.id, "name": c.name, "lang": c.lang.as_str(), "scope": scope_of(c), "approved": code::approved(c), "input": c.input, "last_run": c.last_run.as_ref().map(|r| json!({ "at": r.at, "status": r.status })) }))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else if v.is_empty() {
                ctx.io.info(if scope.is_some() {
                    "no code spaces in that scope".to_string()
                } else {
                    "no code spaces yet; create one with `mnnn code create NAME --lang math`".to_string()
                });
            } else {
                for c in &v {
                    println!("{}\t{}\t{}\t{}\t{}\t{}", c.short_id(), c.name, c.lang.as_str(), label(&scope_of(c)), status_of(c), last_run(c));
                }
            }
        }
        CodeCommand::Show { space, json, source } => {
            let c = store.find_code_space(&space)?;
            if source {
                print!("{}", c.source);
                if !c.source.ends_with('\n') {
                    println!();
                }
            } else if json {
                let mut v = serde_json::to_value(&c)?;
                v["approved"] = json!(code::approved(&c));
                v["trust_hash"] = json!(code::trust_hash(&c).ok());
                println!("{}", serde_json::to_string_pretty(&v)?);
            } else {
                println!("id:          {}", c.id);
                println!("name:        {}", c.name);
                if !c.description.is_empty() {
                    println!("description: {}", c.description);
                }
                println!("language:    {}", c.lang.as_str());
                println!("input:       {}", c.input.text());
                println!("author:      {}", crate::history::author_label(&store, c.author_id.as_deref()));
                println!(
                    "approval:    {} (trust hash {})",
                    status_of(&c),
                    code::trust_hash(&c).map(|h| h[..16].to_string()).unwrap_or_else(|e| e.to_string())
                );
                println!("last run:    {}", last_run(&c));
                if let Some(e) = c.last_run.as_ref().and_then(|r| r.error.as_ref()) {
                    println!("error:       {e}");
                }
                println!("\n{}", c.source.trim_end());
            }
        }
        CodeCommand::Remove { space } => {
            let c = store.find_code_space(&space)?;
            if !ctx.io.confirm(&format!("remove code space `{}`? (its source is kept in the history)", c.name))? {
                return Err(crate::Error::Aborted.into());
            }
            code::remove(&store, "cli", &c.id)?;
            ctx.io.info(format!("removed code space {}", c.name));
        }
        CodeCommand::Approve { space } => {
            let c = store.find_code_space(&space)?;
            if code::approved(&c) {
                ctx.io.info(format!("code space {} is approved already", c.name));
                return Ok(());
            }
            ensure_approved(ctx, &store, &c)?;
            ctx.io.info(format!("approved code space {}", c.name));
        }
        CodeCommand::Build { space } => {
            let c = store.find_code_space(&space)?;
            let env = CodeEnv::for_store(&store)?;
            let b = code::build_space(&store, &env, "cli", &c, &mut |l| eprintln!("{l}"))?;
            ctx.io.info(format!("{} code space {} ({})", if b.cached { "already built:" } else { "built" }, c.name, b.path.display()));
        }
        CodeCommand::Run { space, json, rows } => {
            let c = store.find_code_space(&space)?;
            ensure_approved(ctx, &store, &c)?;
            let c = store.code_space(&c.id)?.ok_or_else(|| anyhow!("code space {} vanished", c.name))?;
            let env = CodeEnv::for_store(&store)?;
            let quiet = ctx.global.quiet;
            let info = code::run(&store, &env, "cli", &c, &mut |l| {
                if !quiet {
                    eprintln!("{l}")
                }
            })?;
            if json {
                println!("{}", serde_json::to_string_pretty(&code::run_json(&info))?);
            } else {
                print_run(&info, rows);
            }
            if let Some(e) = &info.error {
                return Err(anyhow!("code space {} failed: {e}", c.name));
            }
            ctx.io.note(format!(
                "ran code space {} in {} ms{}",
                c.name,
                info.ms,
                if info.writes > 0 { format!(", {} writes", info.writes) } else { String::new() }
            ));
        }
        CodeCommand::Check { space: Some(space) } => {
            let c = store.find_code_space(&space)?;
            let env = CodeEnv::for_store(&store)?;
            for l in code::check(&env, c.lang, &c.source)? {
                println!("{l}");
            }
            println!("approval: {}", status_of(&c));
        }
        CodeCommand::Check { space: None } | CodeCommand::Sdk { .. } => {}
    }
    Ok(())
}

/// A `--scope` filter.
enum ScopeFilter {
    Kind(&'static str),
    Org(String),
}

impl ScopeFilter {
    fn matches(&self, s: &crate::store::CodeScope) -> bool {
        match self {
            ScopeFilter::Kind(k) => s.kind() == *k,
            ScopeFilter::Org(id) => s.org_id() == Some(id.as_str()),
        }
    }
}

fn scope_filter(store: &Store, f: &str) -> crate::error::Result<ScopeFilter> {
    let f = f.trim();
    Ok(match f.to_ascii_lowercase().as_str() {
        "user" | "personal" => ScopeFilter::Kind("user"),
        "global" | "builtin" | "built-in" => ScopeFilter::Kind("global"),
        "org" | "orgs" | "organization" => ScopeFilter::Kind("organization"),
        _ => match f.split_once(':') {
            Some((k, name)) if matches!(k.to_ascii_lowercase().as_str(), "org" | "organization") => ScopeFilter::Org(store.find_org(name)?.id),
            _ => return Err(crate::error::Error::invalid("scope", format!("{f}: use user, org, org:NAME or global"))),
        },
    })
}
