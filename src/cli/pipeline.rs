//! `mnnn pipeline …`.
use super::{Ctx, PipelineCommand};
use crate::code::{self, CodeEnv, pipeline as pl};
use crate::store::{NewAction, Store};
use serde_json::{Value, json};
use std::io::Read;

fn note(store: &Store, kind: &str, summary: String, params: Value) {
    store.note_action(&NewAction::new(kind, "cli", summary).params(params));
}

fn graph_lines(store: &Store, g: &pl::Graph) -> Vec<String> {
    let order = g.order().unwrap_or_else(|_| g.nodes.iter().map(|n| n.id.clone()).collect());
    let mut v = vec![];
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
        v.push(format!("  {id} = {}{tees}{to}", n.kind.text()));
    }
    for p in g.problems(Some(store)) {
        v.push(format!("  ! {p}"));
    }
    v
}

pub fn run(ctx: &Ctx, c: PipelineCommand) -> anyhow::Result<()> {
    let store = ctx.open_store()?;
    match c {
        PipelineCommand::Create { name, description, nodes, edges } => {
            let g = pl::Graph::parse(&nodes, &edges)?;
            let p = store.create_pipeline(&name, &description, &g)?;
            note(&store, "pipeline.create", format!("pipeline {} created", p.name), json!({ "pipeline": p.id, "graph": p.graph }));
            println!("pipeline {} ({} nodes, {} edges)", p.name, g.nodes.len(), g.edges.len());
            for l in graph_lines(&store, &g) {
                println!("{l}");
            }
        }
        PipelineCommand::Edit { pipeline, name, description, nodes, edges, remove, remove_edges } => {
            let p = store.find_pipeline(&pipeline)?;
            let mut g = p.graph.clone();
            for r in &remove {
                if !g.remove_node(r) {
                    anyhow::bail!("no node `{r}` in {}", p.name);
                }
            }
            for r in &remove_edges {
                let (a, b) = r.split_once('>').ok_or_else(|| anyhow::anyhow!("`{r}`: an edge is FROM>TO"))?;
                g.edges.retain(|e| !(e.from == a.trim() && e.to == b.trim()));
            }
            for n in &nodes {
                g.add_node(n)?;
            }
            for e in &edges {
                g.add_edges(e)?;
            }
            let changed = (g != p.graph).then_some(&g);
            let q = store.update_pipeline(&p.id, name.as_deref(), description.as_deref(), changed)?;
            note(&store, "pipeline.edit", format!("pipeline {} edited (v{})", q.name, q.version), json!({ "pipeline": q.id, "graph": q.graph }));
            println!("pipeline {} v{}", q.name, q.version);
            for l in graph_lines(&store, &q.graph) {
                println!("{l}");
            }
        }
        PipelineCommand::Show { pipeline, json } => {
            let p = store.find_pipeline(&pipeline)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&json!({ "pipeline": p, "problems": p.graph.problems(Some(&store)) }))?);
            } else {
                println!(
                    "pipeline {} v{}{}",
                    p.name,
                    p.version,
                    if p.description.is_empty() { String::new() } else { format!(": {}", p.description) }
                );
                for l in graph_lines(&store, &p.graph) {
                    println!("{l}");
                }
            }
        }
        PipelineCommand::List { json } => {
            let ps = store.pipelines()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&ps)?);
            } else if ps.is_empty() {
                ctx.io.note("no pipelines (`mnnn pipeline create NAME --node … --edge …`)");
            } else {
                for p in ps {
                    let ok = if p.graph.problems(Some(&store)).is_empty() { "" } else { " (not valid)" };
                    println!("{}  v{}  {} nodes → {}{ok}", p.name, p.version, p.graph.nodes.len(), p.graph.sinks().join(", "));
                }
            }
        }
        PipelineCommand::Run { pipeline, stdin, chunk, json } => {
            let p = store.find_pipeline(&pipeline)?;
            for n in p.graph.code_spaces() {
                super::code::ensure_approved(ctx, &store, &store.find_code_space(&n)?)?;
            }
            let env = CodeEnv::for_store(&store)?;
            let go = |input: Option<Value>| -> anyhow::Result<()> {
                let quiet = ctx.global.quiet;
                let r = pl::run(&store, &env, "cli", &p, input, &mut |l| {
                    if !quiet {
                        eprintln!("{l}")
                    }
                })?;
                if json {
                    println!("{}", serde_json::to_string_pretty(&json!({ "nodes": r.nodes, "landed": r.landed }))?);
                } else {
                    for n in &r.nodes {
                        let extra = n.error.clone().unwrap_or_else(|| format!("{} → {} rows, {} ms", n.rows_in, n.rows_out, n.ms));
                        println!("{:<12} {:<18} {:<16} {extra}", n.id, n.node, n.status);
                        for s in &n.saved {
                            println!("{:<12} saved {s}", "");
                        }
                    }
                    for (m, k) in &r.landed {
                        ctx.io.note(format!("{k} rows → model {m}"));
                    }
                }
                if !r.ok() {
                    anyhow::bail!("pipeline {} ran with errors", p.name);
                }
                Ok(())
            };
            match stdin.as_deref() {
                None => go(None)?,
                Some("-") if chunk == 0 => {
                    let mut text = String::new();
                    std::io::stdin().read_to_string(&mut text)?;
                    go(Some(code::pipe::read_data(&text, "stdin")?))?
                }
                Some("-") => {
                    let (mut head, mut buf): (Option<String>, Vec<String>) = (None, vec![]);
                    let flush = |buf: &mut Vec<String>, head: &Option<String>| -> anyhow::Result<()> {
                        if buf.is_empty() {
                            return Ok(());
                        }
                        let text = head.iter().cloned().chain(buf.drain(..)).collect::<Vec<_>>().join("\n");
                        go(Some(code::pipe::read_data(&text, "stdin")?))
                    };
                    for line in std::io::stdin().lines() {
                        let line = line?;
                        if line.trim().is_empty() {
                            continue;
                        }
                        if head.is_none() && buf.is_empty() && !line.trim_start().starts_with('{') {
                            head = Some(line);
                            continue;
                        }
                        buf.push(line);
                        if buf.len() >= chunk {
                            flush(&mut buf, &head)?;
                        }
                    }
                    flush(&mut buf, &head)?;
                }
                Some(x) => anyhow::bail!("`{x}`: only `-` (standard input) goes after the pipeline"),
            }
        }
        PipelineCommand::Remove { pipeline } => {
            let p = store.find_pipeline(&pipeline)?;
            if !ctx.io.confirm(&format!("remove pipeline `{}`?", p.name))? {
                return Err(crate::Error::Aborted.into());
            }
            store.remove_pipeline(&p.id)?;
            note(&store, "pipeline.remove", format!("pipeline {} removed", p.name), json!({ "pipeline": p.id }));
            ctx.io.note(format!("removed pipeline {}", p.name));
        }
        PipelineCommand::Export { pipeline, out, no_templates } => {
            let e = pl::export(&store, &pipeline, !no_templates)?;
            std::fs::write(&out, serde_json::to_vec_pretty(&e)?)?;
            note(
                &store,
                "pipeline.export",
                format!("pipeline {} exported", e.name),
                json!({ "pipeline": e.name, "file": out.display().to_string() }),
            );
            ctx.io.note(format!("wrote {} ({} code spaces, {} models)", out.display(), e.code.len(), e.models.len()));
        }
        PipelineCommand::Import { file, name, merge, dry_run } => {
            let e = pl::Export::parse(&std::fs::read(&file)?)?;
            let plan = pl::plan_import(&store, &e, name.as_deref(), merge)?;
            for l in plan.lines() {
                println!("{l}");
            }
            if dry_run {
                return Ok(());
            }
            if !ctx.io.confirm("import it?")? {
                return Err(crate::Error::Aborted.into());
            }
            let p = pl::import(&store, &e, &plan)?;
            note(
                &store,
                "pipeline.import",
                format!("pipeline {} imported (v{})", p.name, p.version),
                json!({ "pipeline": p.id, "file": file.display().to_string(), "plan": plan }),
            );
            ctx.io
                .note(format!("imported pipeline {} v{}; approve its code before it runs (`mnnn pipeline run {}` asks)", p.name, p.version, p.name));
        }
    }
    Ok(())
}
