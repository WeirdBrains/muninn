//! `mnnn ingest ...`
use super::{CommonIngestArgs, Ctx, IngestCommand};
use crate::cli::args::{DataFormat, Paginate, PasswordArgs, ProgressArgs, RedisType, ServiceCommand, ValueFormat};
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
        track_files: true,
        default_source: None,
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
            let opts = cancellable(options(ctx, &a.common)?);
            let csv =
                ingest::csv::CsvOptions { delimiter: a.delimiter, quote: a.quote, escape: a.escape, header: !a.no_header, skip_rows: a.skip_rows };
            let files = ingest::find_files(&a.path, ingest::CSV_EXTS, !opts.no_recurse).unwrap_or_default();
            let ticker = Ticker::start(ctx, a.progress, opts.progress.clone(), names(&a.path, &files), false);
            let r = ingest::tracked(&mut store, "cli", files.len(), opts, |st, o| ingest::ingest_csv(st, ctx.io, o, &a.path, &csv));
            ticker.stop();
            r.map_err(cancelled)?
        }
        IngestCommand::Json(a) => {
            let opts = cancellable(options(ctx, &a.common)?);
            let files = ingest::find_files(&a.path, ingest::JSON_EXTS, !opts.no_recurse).unwrap_or_default();
            let ticker = Ticker::start(ctx, a.progress, opts.progress.clone(), names(&a.path, &files), false);
            let r = ingest::tracked(&mut store, "cli", files.len(), opts, |st, o| {
                ingest::json::ingest_json(st, ctx.io, o, &a.path, a.records.as_deref())
            });
            ticker.stop();
            r.map_err(cancelled)?
        }
        IngestCommand::Path(a) => {
            let opts = cancellable(options(ctx, &a.common)?);
            let path = a.target.or(a.path).expect("clap requires a path");
            let files = ingest::find_files(&path, ingest::DATA_EXTS, !opts.no_recurse).unwrap_or_default();
            let csv = ingest::csv::CsvOptions { delimiter: a.delimiter, header: !a.no_header, ..Default::default() };
            // `ingest path` always names each file as it starts (unless --quiet)
            let ticker = Ticker::start(ctx, a.progress, opts.progress.clone(), names(&path, &files), true);
            let io = ctx.io;
            let r = ingest::tracked(&mut store, "cli", files.len(), opts, |st, o| {
                ingest::ingest_path(st, io, o, &path, &csv, a.records.as_deref(), |_, _, _| {})
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

/// File names relative to the ingested path, for progress lines.
fn names(root: &std::path::Path, files: &[std::path::PathBuf]) -> Vec<String> {
    files
        .iter()
        .map(|f| {
            let rel = f.strip_prefix(root).unwrap_or(f).display().to_string();
            if rel.is_empty() { f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| f.display().to_string()) } else { rel }
        })
        .collect()
}

/// When progress is shown: `--progress` forces it, `--no-progress` and `--quiet` turn it
/// off, otherwise it is on when stderr is a terminal. Returns (show, live) where `live`
/// redraws one line in place (terminal) instead of printing periodic plain lines.
pub fn progress_mode(a: ProgressArgs, quiet: bool, tty: bool) -> (bool, bool) {
    if a.no_progress || (quiet && !a.progress) {
        return (false, false);
    }
    (a.progress || tty, tty)
}

pub fn fmt_bytes(n: u64) -> String {
    const U: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{n} B") } else { format!("{v:.1} {}", U[i]) }
}

pub fn fmt_secs(s: f64) -> String {
    let s = s.max(0.0).round() as u64;
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

fn fmt_count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// One progress line: `[1/3] BTC_5m.csv  42% · 180.3 MiB/429.0 MiB · 12.4 MiB/s · ETA 0:20`
/// (plus ` · overall N%` for several files). `elapsed` is seconds since the run began.
pub fn progress_line(e: &ingest::progress::Estimate, files: usize, name: &str, file_size: u64, elapsed: f64) -> String {
    let rate = if elapsed > 0.0 { e.bytes_done as f64 / elapsed } else { 0.0 };
    let eta = if rate > 0.0 { fmt_secs(e.bytes_total.saturating_sub(e.bytes_done) as f64 / rate) } else { "--:--".into() };
    let done = (e.file * file_size as f64) as u64;
    let mut l = format!(
        "[{}/{files}] {name}  {:>3.0}% · {}/{} · {}/s · ETA {eta}",
        e.file_index + 1,
        e.file * 100.0,
        fmt_bytes(done),
        fmt_bytes(file_size),
        fmt_bytes(rate as u64)
    );
    if files > 1 {
        l.push_str(&format!(" · overall {:.0}%", e.overall * 100.0));
    }
    l
}

/// Per-file summary: `done BTC_5m.csv: 1,234,567 bars in 42.1s (29,321 bars/s)`.
pub fn summary_line(name: &str, bars: u64, ms: i64) -> String {
    let secs = ms.max(0) as f64 / 1000.0;
    let rate = if secs > 0.0 { format!(" ({} bars/s)", fmt_count((bars as f64 / secs) as u64)) } else { String::new() };
    format!("done {name}: {} bars in {secs:.1}s{rate}", fmt_count(bars))
}

/// Progress for file ingests on stderr. A background thread drains file events from the
/// shared [`ingest::Progress`] (start lines, per-file summaries) and, while a file runs,
/// redraws a live line on a terminal or prints a plain line every few seconds otherwise.
struct Ticker {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: std::cell::RefCell<Option<std::thread::JoinHandle<()>>>,
}

impl Ticker {
    fn start(ctx: &Ctx, args: ProgressArgs, p: Option<std::sync::Arc<ingest::Progress>>, names: Vec<String>, name_files: bool) -> Self {
        use std::io::IsTerminal;
        use std::sync::atomic::Ordering::Relaxed;
        let quiet = ctx.global.quiet;
        let (show, live) = progress_mode(args, quiet, std::io::stderr().is_terminal());
        let name_files = name_files && !quiet;
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let handle = match p {
            Some(p) if show || name_files => {
                p.record_events.store(true, Relaxed);
                let s = stop.clone();
                Some(std::thread::spawn(move || {
                    let started = std::time::Instant::now();
                    let n = names.len();
                    let name = |i: u64| names.get(i as usize).cloned().unwrap_or_default();
                    let mut drawn = false;
                    let mut last_plain = std::time::Instant::now();
                    let mut running: Option<(u64, u64)> = None;
                    let clear = |drawn: &mut bool| {
                        if *drawn {
                            eprint!("\r\x1b[2K");
                            *drawn = false;
                        }
                    };
                    let mut paused_for = std::time::Duration::ZERO;
                    loop {
                        let stopping = s.load(Relaxed);
                        if p.paused.load(Relaxed) && !stopping {
                            // a prompt is up: clear our line and keep quiet until it is answered
                            clear(&mut drawn);
                            p.pause_ack.store(true, Relaxed);
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            paused_for += std::time::Duration::from_millis(50);
                            last_plain = std::time::Instant::now();
                            continue;
                        }
                        for ev in p.take_events() {
                            clear(&mut drawn);
                            match ev {
                                ingest::progress::FileEvent::Begin { index, size } => {
                                    running = Some((index, size));
                                    last_plain = std::time::Instant::now();
                                    if name_files || (show && !live) {
                                        eprintln!("[{}/{n}] {} ({})", index + 1, name(index), fmt_bytes(size));
                                    }
                                }
                                ingest::progress::FileEvent::End { index, bars, ms } => {
                                    running = None;
                                    if show {
                                        eprintln!("{}", summary_line(&name(index), bars, ms));
                                    }
                                }
                            }
                        }
                        if stopping {
                            clear(&mut drawn);
                            break;
                        }
                        if show && let Some((i, size)) = running {
                            let line = progress_line(&p.estimate(), n, &name(i), size, started.elapsed().saturating_sub(paused_for).as_secs_f64());
                            if live {
                                eprint!("\r\x1b[2K{line}");
                                drawn = true;
                            } else if last_plain.elapsed() >= std::time::Duration::from_secs(5) {
                                eprintln!("{line}");
                                last_plain = std::time::Instant::now();
                            }
                        }
                        std::thread::sleep(std::time::Duration::from_millis(250));
                    }
                }))
            }
            _ => None,
        };
        Self { stop, handle: std::cell::RefCell::new(handle) }
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
    // progress is reported even if the Ctrl-C handler could not be installed
    let _ = ctrlc::set_handler(move || h.request_cancel("cli"));
    o.progress = Some(p);
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

#[cfg(test)]
mod progress_tests {
    use super::*;

    #[test]
    fn mode_follows_flags_and_tty() {
        let a = |progress, no_progress| ProgressArgs { progress, no_progress };
        assert_eq!(progress_mode(a(false, false), false, true), (true, true));
        assert_eq!(progress_mode(a(false, false), false, false), (false, false));
        assert_eq!(progress_mode(a(true, false), false, false), (true, false));
        assert_eq!(progress_mode(a(false, true), false, true), (false, false));
        assert_eq!(progress_mode(a(false, false), true, true), (false, false));
        assert_eq!(progress_mode(a(true, false), true, false), (true, false));
    }

    #[test]
    fn lines_read_well() {
        let e = ingest::progress::Estimate { file_index: 0, file: 0.42, overall: 0.14, bytes_done: 180 << 20, bytes_total: 1290 << 20 };
        let l = progress_line(&e, 3, "BTC_5m.csv", 429 << 20, 10.0);
        assert_eq!(l, "[1/3] BTC_5m.csv   42% · 180.2 MiB/429.0 MiB · 18.0 MiB/s · ETA 1:02 · overall 14%");
        assert_eq!(summary_line("BTC_5m.csv", 1_234_567, 42_100), "done BTC_5m.csv: 1,234,567 bars in 42.1s (29,324 bars/s)");
        assert_eq!(fmt_secs(3725.0), "1:02:05");
        assert_eq!(fmt_bytes(512), "512 B");
    }
}
