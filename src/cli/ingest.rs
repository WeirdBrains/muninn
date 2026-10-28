//! `mnnn ingest ...`
use super::{CommonIngestArgs, Ctx, IngestCommand};
use crate::cli::args::{DataFormat, Paginate, PasswordArgs, RedisType, ServiceCommand, ValueFormat};
use crate::ingest::service::{self as svc, Secret};
use crate::ingest::{self, IngestOptions, IngestReport};
use crate::store::Store;

pub fn options(ctx: &Ctx, c: &CommonIngestArgs) -> anyhow::Result<IngestOptions> {
    Ok(IngestOptions {
        source: c.source.clone(),
        symbol: c.symbol.clone(),
        duration: c.duration.clone(),
        map: c.map.clone(),
        rejects: c.rejects.clone(),
        force: c.force,
        drop_zero_volume: ctx.prefs()?.ingest.drop_zero_volume,
        run_id: None,
        progress: None,
        no_recurse: c.no_recursive,
    })
}

pub fn report(ctx: &Ctx, reports: &[IngestReport]) {
    for r in reports {
        ctx.io.info(r.summary());
        for w in &r.warnings {
            ctx.io.warn(w);
        }
    }
}

pub fn run(ctx: &Ctx, cmd: IngestCommand) -> anyhow::Result<()> {
    let mut store = ctx.open_store()?;
    let reports = match cmd {
        IngestCommand::Csv(a) => {
            let opts = options(ctx, &a.common)?;
            let csv =
                ingest::csv::CsvOptions { delimiter: a.delimiter, quote: a.quote, escape: a.escape, header: !a.no_header, skip_rows: a.skip_rows };
            let n = ingest::find_files(&a.path, ingest::CSV_EXTS, !opts.no_recurse).map(|v| v.len()).unwrap_or(0);
            let opts = cancellable(opts);
            ingest::tracked(&mut store, "cli", n, opts, |st, o| ingest::ingest_csv(st, ctx.io, o, &a.path, &csv)).map_err(cancelled)?
        }
        IngestCommand::Json(a) => {
            let opts = cancellable(options(ctx, &a.common)?);
            let n = ingest::find_files(&a.path, ingest::JSON_EXTS, !opts.no_recurse).map(|v| v.len()).unwrap_or(0);
            ingest::tracked(&mut store, "cli", n, opts, |st, o| ingest::json::ingest_json(st, ctx.io, o, &a.path, a.records.as_deref()))
                .map_err(cancelled)?
        }
        IngestCommand::Path(a) => {
            let opts = cancellable(options(ctx, &a.common)?);
            let path = a.target.or(a.path).expect("clap requires a path");
            let n = ingest::find_files(&path, ingest::DATA_EXTS, !opts.no_recurse).map(|v| v.len()).unwrap_or(0);
            let csv = ingest::csv::CsvOptions { delimiter: a.delimiter, header: !a.no_header, ..Default::default() };
            let ticker = Ticker::start(ctx, opts.progress.clone());
            let io = ctx.io;
            let root = path.clone();
            let r = ingest::tracked(&mut store, "cli", n, opts, |st, o| {
                ingest::ingest_path(st, io, o, &path, &csv, a.records.as_deref(), |i, total, f| {
                    let rel = f.strip_prefix(&root).unwrap_or(f).display().to_string();
                    let rel = if rel.is_empty() { f.display().to_string() } else { rel };
                    ticker.file(i, total, &rel);
                })
            });
            ticker.stop();
            r.map_err(cancelled)?
        }
        IngestCommand::Service(s) => service(ctx, &mut store, s)?,
        IngestCommand::Runs(a) => {
            let run = match a.show {
                Some(id) => store.import_run(id)?,
                None => store.last_import_run()?,
            };
            let Some(r) = run else {
                println!("no import runs yet");
                return Ok(());
            };
            println!("run #{} · {} · {} · {}/{} file(s) kept", r.id, r.origin, r.status, r.files_done, r.files_total);
            if let Some(at) = r.cancel_requested_at {
                println!(
                    "cancel requested {} by {} during {} ({})",
                    ingest::time::fmt_ts(at),
                    r.cancel_by.unwrap_or_default(),
                    r.cancel_phase.unwrap_or_default(),
                    r.cancel_file.unwrap_or_default()
                );
            }
            if let Some(n) = r.note.filter(|n| !n.is_empty()) {
                println!("note: {n}");
            }
            for x in store.import_artifacts(r.id)? {
                println!("  {:<12} {:<13} {} {}", x.state, x.kind, x.reference, x.file);
            }
            return Ok(());
        }
    };
    report(ctx, &reports);
    Ok(())
}

/// Per-file and overall progress on stderr for `ingest path`: a `[i/n] file` line per
/// file, plus a live `file % · overall %` line when stderr is a terminal.
struct Ticker {
    quiet: bool,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: std::cell::RefCell<Option<std::thread::JoinHandle<()>>>,
    tty: bool,
}

impl Ticker {
    fn start(ctx: &Ctx, p: Option<std::sync::Arc<ingest::Progress>>) -> Self {
        use std::io::IsTerminal;
        let quiet = ctx.global.quiet;
        let tty = !quiet && std::io::stderr().is_terminal();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handle = match (tty, p) {
            (true, Some(p)) => {
                let s = stop.clone();
                Some(std::thread::spawn(move || {
                    while !s.load(std::sync::atomic::Ordering::Relaxed) {
                        let e = p.estimate();
                        eprint!("\r  file {:>3.0}% · overall {:>3.0}%  ", e.file * 100.0, e.overall * 100.0);
                        std::thread::sleep(std::time::Duration::from_millis(250));
                    }
                    eprint!("\r{:40}\r", "");
                }))
            }
            _ => None,
        };
        Self { quiet, stop, handle: std::cell::RefCell::new(handle), tty }
    }
    fn file(&self, i: usize, n: usize, name: &str) {
        if !self.quiet {
            if self.tty {
                eprint!("\r{:40}\r", "");
            }
            eprintln!("[{}/{n}] {name}", i + 1);
        }
    }
    fn stop(&self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(h) = self.handle.borrow_mut().take() {
            let _ = h.join();
        }
    }
}

/// Ctrl-C asks the import to stop cleanly (the current file is rolled back) instead of
/// killing the process.
fn cancellable(mut o: IngestOptions) -> IngestOptions {
    let p = std::sync::Arc::new(ingest::Progress::default());
    let h = p.clone();
    if ctrlc::set_handler(move || h.request_cancel("cli")).is_ok() {
        o.progress = Some(p);
    }
    o
}

fn cancelled(e: crate::Error) -> anyhow::Error {
    match e {
        crate::Error::Aborted => anyhow::anyhow!("import cancelled: finished files were kept, the current file was rolled back"),
        e => e.into(),
    }
}

fn secret(p: &PasswordArgs) -> Secret {
    Secret { literal: p.password.clone(), env: p.password_env.clone(), stdin: p.password_stdin }
}

fn service(ctx: &Ctx, store: &mut Store, cmd: ServiceCommand) -> anyhow::Result<Vec<IngestReport>> {
    Ok(match cmd {
        ServiceCommand::Http(a) => {
            let req = svc::http::HttpRequest {
                url: a.url.clone(),
                method: a.method.as_str().to_string(),
                body: a.body.as_deref().map(svc::sql_or_file).transpose()?,
                params: a.params.clone(),
                headers: a.headers.clone(),
                format: a.format.map(|f| match f {
                    DataFormat::Json => svc::http::Format::Json,
                    DataFormat::Csv => svc::http::Format::Csv,
                    DataFormat::Ndjson => svc::http::Format::Ndjson,
                }),
                records: a.records.clone(),
                paging: match a.paginate {
                    Paginate::None => svc::http::Paging::None,
                    Paginate::Cursor => svc::http::Paging::Cursor,
                    Paginate::Time => svc::http::Paging::Time,
                },
                cursor_path: a.cursor_path.clone(),
                cursor_param: a.cursor_param.clone(),
                start_param: a.start_param.clone(),
                from: a.from.clone(),
                to: a.to.clone(),
                max_pages: a.max_pages,
                rate_limit: a.rate_limit,
                retries: a.retries,
                insecure: a.insecure,
                timeout: a.common.timeout,
            };
            req.validate()?;
            let host = url::Url::parse(&a.url).ok().and_then(|u| u.host_str().map(str::to_string));
            let input = req.fetch()?;
            let loc = input.locator.clone();
            svc::run_inputs(store, ctx.io, options(ctx, &a.common.ingest)?, &loc, host, vec![input])?
        }
        ServiceCommand::Redis(a) => {
            let src = svc::redis::RedisSource {
                uri: a.uri.clone(),
                port: a.port,
                username: a.username.clone(),
                password: secret(&a.password).resolve(&ctx.io, None)?,
                db: a.db_index,
                tls: a.tls,
                key: a.key.clone(),
                pattern: a.pattern.clone(),
                kind: match a.kind {
                    RedisType::Auto => svc::redis::Kind::Auto,
                    RedisType::Stream => svc::redis::Kind::Stream,
                    RedisType::List => svc::redis::Kind::List,
                    RedisType::Zset => svc::redis::Kind::Zset,
                    RedisType::Hash => svc::redis::Kind::Hash,
                    RedisType::String => svc::redis::Kind::String,
                },
                value_format: match a.value_format {
                    ValueFormat::Json => svc::redis::ValueFormat::Json,
                    ValueFormat::Csv => svc::redis::ValueFormat::Csv,
                    ValueFormat::Fields => svc::redis::ValueFormat::Fields,
                },
                from: a.from.clone(),
                to: a.to.clone(),
                batch: a.batch.max(1),
                timeout: a.common.timeout,
            };
            let inputs = src.read()?;
            svc::run_inputs(store, ctx.io, options(ctx, &a.common.ingest)?, &src.display_url(), Some(src.host()), inputs)?
        }
        ServiceCommand::Sqlite(a) => {
            let src = svc::sqlite::SqliteSource { path: &a.path, table: a.table.as_deref(), query: a.query.as_deref(), key: a.key.as_deref() };
            let input = src.read()?;
            let loc = input.locator.clone();
            let stem = a.path.file_stem().map(|s| s.to_string_lossy().to_string());
            svc::run_inputs(store, ctx.io, options(ctx, &a.common.ingest)?, &loc, stem, vec![input])?
        }
        ServiceCommand::Postgres(a) => {
            let src = svc::postgres::PgSource {
                url: a.url.clone(),
                username: a.username.clone(),
                password: secret(&a.password).resolve(&ctx.io, if a.url.is_none() { Some("Postgres password") } else { None })?,
                host: a.host.clone(),
                port: a.port,
                database: a.database.clone(),
                secure: a.secure,
                ca_cert: a.ca_cert.clone(),
                schema: a.schema.clone(),
                table: a.table.clone(),
                query: a.query.clone(),
                batch: a.batch,
                timeout: a.common.timeout,
            };
            let input = src.read()?;
            let loc = input.locator.clone();
            svc::run_inputs(store, ctx.io, options(ctx, &a.common.ingest)?, &loc, Some(src.host_label()), vec![input])?
        }
    })
}
