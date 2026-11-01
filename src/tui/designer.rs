//! Pipelines in the TUI: running one on a worker thread (the UI stays live; hops and view
//! frames arrive through the history like any other run).
use super::App;
use crate::code::{self, CodeEnv};
use crate::store::Store;
use std::sync::mpsc;

pub struct Job {
    pub pipeline: String,
    pub rx: mpsc::Receiver<Result<String, String>>,
}

#[derive(Default)]
pub struct DesignerState {
    pub job: Option<Job>,
}

/// Run a pipeline on a thread: its code spaces must be approved (the Code window asks).
pub fn run(app: &mut App, name: &str) {
    if app.designer.job.is_some() {
        return app.set_status("a pipeline is running already");
    }
    let p = match app.store.find_pipeline(name) {
        Ok(p) => p,
        Err(e) => return app.set_status(e.to_string()),
    };
    let problems = p.graph.problems(Some(&app.store));
    if !problems.is_empty() {
        return app.set_status(format!("{} is not valid: {}", p.name, problems.join("; ")));
    }
    for s in p.graph.code_spaces() {
        match app.store.find_code_space(&s) {
            Ok(c) if code::approved(&c) => {}
            Ok(c) => return app.set_status(format!("approve code space {} first (Code window: open it, r asks)", c.name)),
            Err(e) => return app.set_status(e.to_string()),
        }
    }
    super::frametile::open_many(app, super::frametile::views_of(&p));
    let (db, dek, home) = (app.paths.db.clone(), app.dek.clone(), app.paths.home.clone());
    let (tx, rx) = mpsc::channel();
    let pname = p.name.clone();
    std::thread::spawn(move || {
        let r = (|| -> crate::Result<String> {
            let s = Store::open(&db, Some(&dek.0))?;
            let env = CodeEnv::new(&home);
            let r = code::pipeline::run(&s, &env, "tui", &p, None, &mut |_| {})?;
            let landed: Vec<String> = r.landed.iter().map(|(m, n)| format!("{n} → {m}")).collect();
            Ok(format!("pipeline {} {} · {}", p.name, if r.ok() { "ran" } else { "ran with errors" }, landed.join(", ")))
        })();
        let _ = tx.send(r.map_err(|e| e.to_string()));
    });
    app.designer.job = Some(Job { pipeline: pname.clone(), rx });
    app.set_status(format!("running pipeline {pname}… (the UI stays live)"));
}

pub fn poll(app: &mut App) {
    let Some(j) = app.designer.job.as_ref() else { return };
    let r = match j.rx.try_recv() {
        Ok(r) => r,
        Err(mpsc::TryRecvError::Empty) => return,
        Err(mpsc::TryRecvError::Disconnected) => Err(format!("pipeline {} stopped", j.pipeline)),
    };
    app.designer.job = None;
    match r {
        Ok(m) => app.set_status(m),
        Err(e) => app.set_status(format!("pipeline failed: {e}")),
    }
    app.refresh_history(true);
    app.refresh_rows();
}
