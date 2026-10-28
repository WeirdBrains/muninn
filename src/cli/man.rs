//! Man pages generated from the clap definitions (clap_mangen), plus hand-written sections for mnnn(1).
use super::Cli;
use clap::CommandFactory;
use std::io::Write;
use std::path::Path;

const EXTRA: &str = r#".SH DESCRIPTION
Muninn ingests cryptocurrency market data from CSV and JSON files and from HTTP, Redis, SQLite and Postgres
services, normalizes every record into OHLCV bars (UTC epoch-millisecond open time, base/quote symbol, source,
duration) and stores them in a SQLCipher-encrypted SQLite database. It has a subcommand CLI and a terminal UI
(\fBmnnn tui\fR) with a navigator, ingest and export forms, key and backup screens, themes and searchable help.
.SH FILES
.TP
.I ~/.muninn/storage/muninn.database
SQLCipher-encrypted bar database.
.TP
.I ~/.muninn/keys/
\fBenvelope.toml\fR (data key wrapped by the service key and by a master-derived key) and \fBservice/v<N>.toml\fR
(service keys wrapped by your password). Never contains the master key, the 24 words or the shards.
.TP
.I ~/.muninn/themes/<name>/interface.toml
Theme palette and role mapping with [dark] and [light] variants; \fIassets/\fR beside it is free for extras.
.TP
.I ~/.muninn/preferences.toml
Active theme and mode, ingest defaults.
.PP
The root can be moved with \fB--home\fR or \fBMUNINN_HOME\fR. Directories are created 0700, files 0600.
.SH ENVIRONMENT
.TP
.B MUNINN_HOME
Root directory instead of ~/.muninn.
.TP
.B MUNINN_DB
Database file path.
.TP
.B MUNINN_NON_INTERACTIVE
Never prompt.
.TP
.B MUNINN_QUIET
Same as \fB-q\fR.
.TP
.B MUNINN_YES
Same as \fB-y\fR.
.TP
.B MUNINN_THEME_MODE
light, dark or system for this run.
.TP
.B MUNINN_PASSWORD
Password that unlocks the service key (otherwise prompted).
.TP
.B MUNINN_EXPORT_PASSWORD
Password for key exports and encrypted backups (falls back to MUNINN_PASSWORD).
.TP
.B MUNINN_LOG
Tracing filter, e.g. debug.
.TP
.B MUNINN_RAW_DATA
Directory with full raw CSV files for the ignored full-file tests.
.SH EXIT STATUS
.TP
.B 0
Success.
.TP
.B 1
Any runtime error (message on stderr), including a missing value under \fB--non-interactive\fR or a declined confirmation.
.TP
.B 2
Invalid command-line usage.
.SH SECURITY
The master key is 256 random bits shown once as 24 BIP39 words and as five 3-of-5 Shamir shards; none of these
are stored. Service keys are derived with HKDF-SHA256 and stored wrapped by an Argon2id password key; the database
key is wrapped by the service key and by a master-derived key (XChaCha20-Poly1305). Rotate with
\fBmnnn keys rotate\fR, re-encrypt with \fBmnnn keys rekey\fR. Prefer \fB--password-env\fR or
\fB--password-stdin\fR over \fB--password\fR, which ends up in shell history. Do not store the words and three or
more shards in the same place. Backups keep the database encrypted; use \fB--encrypt\fR to protect preferences
and themes too.
.SH EXAMPLES
.nf
export MUNINN_HOME=/tmp/mnnn-demo
mnnn keys init
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data/BTCUSDT_5m.csv --source binance -y
mnnn ingest csv --path ~/Projects/kommandosaldat/raw/data --source binance -y
mnnn export --format parquet --out /tmp/btc.parquet --symbol BTC/USDT
mnnn -q export --format csv --out - --from 2026-10-01 | head
mnnn backup create --out ~/backups --encrypt
mnnn keys recover --shard a.age --shard b.age --shard c.age
mnnn theme mode system
mnnn help keys
.fi
.SH SEE ALSO
\fBmnnn-ingest\fR(1), \fBmnnn-export\fR(1), \fBmnnn-sources\fR(1), \fBmnnn-symbols\fR(1), \fBmnnn-keys\fR(1),
\fBmnnn-backup\fR(1), \fBmnnn-theme\fR(1), \fBmnnn-tui\fR(1), \fBmnnn-help\fR(1), \fBsqlcipher\fR(1), \fBage\fR(1)
"#;

fn page_name(path: &[String]) -> String {
    path.join("-")
}

fn render(cmd: &clap::Command, path: &[String], out: &Path, count: &mut usize) -> std::io::Result<()> {
    let name = page_name(path);
    let cmd = cmd.clone().name(name.clone()).bin_name(path.join(" ")).version(env!("CARGO_PKG_VERSION"));
    let man = clap_mangen::Man::new(cmd.clone());
    let mut buf: Vec<u8> = Vec::new();
    man.render_title(&mut buf)?;
    man.render_name_section(&mut buf)?;
    man.render_synopsis_section(&mut buf)?;
    if path.len() == 1 {
        buf.write_all(EXTRA.as_bytes())?;
    } else {
        man.render_description_section(&mut buf)?;
    }
    man.render_options_section(&mut buf)?;
    if cmd.get_subcommands().any(|s| !s.is_hide_set()) {
        man.render_subcommands_section(&mut buf)?;
    }
    man.render_version_section(&mut buf)?;
    if path.len() > 1 {
        writeln!(buf, ".SH SEE ALSO\n\\fBmnnn\\fR(1)")?;
    }
    std::fs::write(out.join(format!("{name}.1")), buf)?;
    *count += 1;
    for sub in cmd.get_subcommands().filter(|s| !s.is_hide_set()) {
        let mut p = path.to_vec();
        p.push(sub.get_name().to_string());
        render(sub, &p, out, count)?;
    }
    Ok(())
}

/// Write mnnn.1 and one page per (visible) subcommand. Returns the number of pages.
pub fn generate(out: &Path) -> std::io::Result<usize> {
    std::fs::create_dir_all(out)?;
    let mut cmd = Cli::command();
    cmd.build();
    let mut n = 0;
    render(&cmd, &["mnnn".to_string()], out, &mut n)?;
    Ok(n)
}

/// Every page name that should exist.
pub fn expected_pages() -> Vec<String> {
    fn walk(c: &clap::Command, path: Vec<String>, out: &mut Vec<String>) {
        out.push(path.join("-"));
        for s in c.get_subcommands().filter(|s| !s.is_hide_set()) {
            let mut p = path.clone();
            p.push(s.get_name().to_string());
            walk(s, p, out);
        }
    }
    let mut cmd = Cli::command();
    cmd.build();
    let mut v = vec![];
    walk(&cmd, vec!["mnnn".into()], &mut v);
    v
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_subcommand_has_a_page() {
        let dir = tempfile::tempdir().unwrap();
        let n = super::generate(dir.path()).unwrap();
        let expected = super::expected_pages();
        assert_eq!(n, expected.len());
        for p in &expected {
            let f = dir.path().join(format!("{p}.1"));
            assert!(f.exists(), "missing {p}.1");
        }
        for p in ["mnnn", "mnnn-ingest", "mnnn-ingest-csv", "mnnn-ingest-service-http", "mnnn-keys", "mnnn-backup", "mnnn-theme", "mnnn-tui"] {
            assert!(expected.iter().any(|e| e == p), "{p}");
        }
        let top = std::fs::read_to_string(dir.path().join("mnnn.1")).unwrap();
        for s in [".SH DESCRIPTION", ".SH FILES", ".SH ENVIRONMENT", ".SH EXIT STATUS", ".SH SECURITY", ".SH EXAMPLES", ".SH SEE ALSO"] {
            assert!(top.contains(s), "{s}");
        }
        assert!(top.contains("muninn.database"));
        // The committed pages are in sync with the clap definitions.
        let committed = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("man");
        if committed.exists() {
            for p in &expected {
                let a = std::fs::read_to_string(dir.path().join(format!("{p}.1"))).unwrap();
                let b = std::fs::read_to_string(committed.join(format!("{p}.1"))).unwrap_or_default();
                assert_eq!(a, b, "man/{p}.1 is stale; run `cargo run -- man --out man`");
            }
        }
    }
}
