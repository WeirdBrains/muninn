//! Writing documents to files in every format, printing, and recording both in history.
use super::archive;
use super::docs;
use crate::error::{Error, Result};
use crate::render::{DocFormat, write_files};
use crate::store::{NewAction, Store};
use std::path::{Path, PathBuf};

/// Formats a document can be exported in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutFormat {
    Html,
    Json,
    Csv,
    Latex,
    Pdf,
    Docx,
    Text,
    /// Projects only: one `.mnnp` file with everything (see [`archive`]).
    Archive,
}

impl OutFormat {
    pub const ALL: [OutFormat; 8] =
        [OutFormat::Pdf, OutFormat::Html, OutFormat::Docx, OutFormat::Latex, OutFormat::Json, OutFormat::Csv, OutFormat::Text, OutFormat::Archive];

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "html" | "htm" => OutFormat::Html,
            "json" => OutFormat::Json,
            "csv" => OutFormat::Csv,
            "latex" | "tex" => OutFormat::Latex,
            "pdf" => OutFormat::Pdf,
            "docx" | "word" => OutFormat::Docx,
            "text" | "txt" => OutFormat::Text,
            "archive" | "mnnp" => OutFormat::Archive,
            o => return Err(Error::invalid("--format", format!("`{o}` (html, json, csv, latex, pdf, docx, text or archive)"))),
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            OutFormat::Html => "html",
            OutFormat::Json => "json",
            OutFormat::Csv => "csv",
            OutFormat::Latex => "latex",
            OutFormat::Pdf => "pdf",
            OutFormat::Docx => "docx",
            OutFormat::Text => "text",
            OutFormat::Archive => "archive",
        }
    }
    pub fn ext(self) -> &'static str {
        match self {
            OutFormat::Json => "json",
            OutFormat::Csv => "csv",
            OutFormat::Archive => "mnnp",
            o => o.doc().map(DocFormat::ext).unwrap_or("out"),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            OutFormat::Html => "HTML (a web page, images inside)",
            OutFormat::Json => "JSON (data, can be imported again)",
            OutFormat::Csv => "CSV (the tables)",
            OutFormat::Latex => "LaTeX (source, images beside it)",
            OutFormat::Pdf => "PDF",
            OutFormat::Docx => "Word (.docx)",
            OutFormat::Text => "Plain text",
            OutFormat::Archive => "Project archive (.mnnp, everything, can be imported again)",
        }
    }
    fn doc(self) -> Option<DocFormat> {
        Some(match self {
            OutFormat::Html => DocFormat::Html,
            OutFormat::Latex => DocFormat::Latex,
            OutFormat::Pdf => DocFormat::Pdf,
            OutFormat::Docx => DocFormat::Docx,
            OutFormat::Text => DocFormat::Text,
            _ => return None,
        })
    }
}

/// What to export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A journal; with entries, just those.
    Journal {
        id: String,
        entries: Vec<String>,
    },
    Report(String),
    /// A project; `rows` bars listed per result in documents (0: all); `data` puts bars in
    /// archives and JSON.
    Project {
        id: String,
        rows: usize,
        data: bool,
    },
}

impl Target {
    /// Formats this target can be written in.
    pub fn formats(&self) -> Vec<OutFormat> {
        OutFormat::ALL
            .into_iter()
            .filter(|f| match (self, f) {
                (Target::Project { .. }, _) => true,
                (_, OutFormat::Archive) => false,
                (Target::Journal { .. }, OutFormat::Csv) => false,
                _ => true,
            })
            .collect()
    }

    /// (history kind prefix, what, name)
    fn describe(&self, store: &Store) -> Result<(&'static str, String, String)> {
        Ok(match self {
            Target::Journal { id, entries } => {
                let j = store.journal(id)?.ok_or_else(|| Error::invalid("journal", format!("`{id}` not found")))?;
                match entries.as_slice() {
                    [one] => {
                        let e = store.entry(one)?.ok_or_else(|| Error::invalid("entry", format!("`{one}` not found")))?;
                        ("entry", format!("entry {} of journal {}", e.heading(), j.name), e.heading())
                    }
                    [] => ("journal", format!("journal {}", j.name), j.name),
                    v => ("journal", format!("{} entries of journal {}", v.len(), j.name), j.name),
                }
            }
            Target::Report(id) => {
                let r = store.report(id)?.ok_or_else(|| Error::invalid("report", format!("`{id}` not found")))?;
                ("report", format!("report {}", r.name), r.name)
            }
            Target::Project { id, .. } => {
                let p = store.project(id)?.ok_or_else(|| Error::invalid("project", format!("`{id}` not found")))?;
                ("project", format!("project {}", p.name), p.name)
            }
        })
    }

    fn params(&self) -> serde_json::Value {
        match self {
            Target::Journal { id, entries } => serde_json::json!({ "journal": id, "entries": entries }),
            Target::Report(id) => serde_json::json!({ "report": id }),
            Target::Project { id, rows, data } => serde_json::json!({ "project": id, "rows": rows, "data": data }),
        }
    }

    /// A file name for it: `<slug>.<ext>`.
    pub fn file_name(&self, store: &Store, f: OutFormat) -> String {
        let name = self.describe(store).map(|d| d.2).unwrap_or_else(|_| "export".into());
        format!("{}.{}", slug(&name), f.ext())
    }

    /// The document for people.
    pub fn doc(&self, store: &Store) -> Result<crate::render::Doc> {
        match self {
            Target::Journal { id, entries } => docs::journal(store, id, entries),
            Target::Report(id) => docs::report(store, id),
            Target::Project { id, rows, .. } => docs::project(store, id, *rows),
        }
    }
}

/// `Weekly report: BTC!` → `weekly-report-btc`.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').chars().take(60).collect::<String>();
    if out.is_empty() { "export".into() } else { out }
}

/// A report as JSON: its definition and the tables its data sections show now.
fn report_json(store: &Store, id: &str) -> Result<String> {
    let r = store.report(id)?.ok_or_else(|| Error::invalid("report", format!("`{id}` not found")))?;
    let sections = store.sections(&r.id)?;
    let d = docs::report(store, &r.id)?;
    let csv = docs::tables_csv(&d);
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "format": "muninn-report",
        "version": 1,
        "muninn": env!("CARGO_PKG_VERSION"),
        "exported_at": crate::store::now_ms(),
        "report": r,
        "authors": d.authors,
        "sections": sections,
        "tables_csv": csv,
    }))?)
}

/// Write `t` as `f` to `out`. Returns the files written.
pub fn write(store: &Store, t: &Target, f: OutFormat, out: &Path) -> Result<Vec<PathBuf>> {
    if !t.formats().contains(&f) {
        let what = match t {
            Target::Journal { .. } => "journals and entries",
            Target::Report(_) => "reports",
            Target::Project { .. } => "projects",
        };
        return Err(Error::invalid(
            "--format",
            format!("{} is not available for {what} (choose {})", f.as_str(), t.formats().iter().map(|f| f.as_str()).collect::<Vec<_>>().join(", ")),
        ));
    }
    if let Some(p) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(p)?;
    }
    let bytes: Vec<u8> = match (t, f) {
        (Target::Project { id, rows, data }, OutFormat::Archive) => archive::project_archive(store, id, *data, *rows)?.0,
        (Target::Project { id, data, .. }, OutFormat::Json) => archive::project_json(store, id, *data)?.0.into_bytes(),
        (Target::Journal { id, entries }, OutFormat::Json) => archive::journal_json(store, id, entries)?.into_bytes(),
        (Target::Report(id), OutFormat::Json) => report_json(store, id)?.into_bytes(),
        (_, OutFormat::Csv) => docs::tables_csv(&t.doc(store)?).into_bytes(),
        (_, f) => return write_files(&t.doc(store)?, f.doc().expect("document format"), out),
    };
    std::fs::write(out, bytes)?;
    Ok(vec![out.to_path_buf()])
}

/// [`write`] and record it in history (`<kind>.export`).
pub fn write_recorded(store: &Store, t: &Target, f: OutFormat, out: &Path, origin: &str) -> Result<Vec<PathBuf>> {
    let files = write(store, t, f, out)?;
    let (kind, what, _) = t.describe(store)?;
    let mut params = t.params();
    params["format"] = f.as_str().into();
    params["out"] = out.display().to_string().into();
    params["files"] = files.len().into();
    store.note_action(
        &NewAction::new(&format!("{kind}.export"), origin, format!("export {what} to {} ({})", out.display(), f.as_str())).params(params),
    );
    Ok(files)
}

/// How a print went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printed {
    pub pdf: PathBuf,
    /// The command that took the job (`lp` or `lpr`); None: no print command here.
    pub command: Option<String>,
    /// What the print command said (job id).
    pub output: String,
    /// Only saved: printing was not asked for (`--save-only`).
    pub saved_only: bool,
}

impl Printed {
    pub fn message(&self) -> String {
        match &self.command {
            Some(c) => {
                let o = self.output.trim();
                format!(
                    "sent to the printer with {c}{} (PDF kept at {})",
                    if o.is_empty() { String::new() } else { format!(": {o}") },
                    self.pdf.display()
                )
            }
            None if self.saved_only => format!("saved the PDF at {} (not sent to a printer)", self.pdf.display()),
            None => format!("no print command (lp or lpr) found: the PDF is at {}", self.pdf.display()),
        }
    }
}

fn find_program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

/// Print a PDF with `lp` (or `lpr`), on `printer` or the default one.
pub fn send_to_printer(pdf: &Path, title: &str, printer: Option<&str>) -> Result<(Option<String>, String)> {
    let (cmd, args): (&str, Vec<String>) = if find_program("lp").is_some() {
        let mut a = vec!["-t".to_string(), title.to_string()];
        if let Some(p) = printer {
            a.extend(["-d".to_string(), p.to_string()]);
        }
        ("lp", a)
    } else if find_program("lpr").is_some() {
        let mut a = vec!["-T".to_string(), title.to_string()];
        if let Some(p) = printer {
            a.extend(["-P".to_string(), p.to_string()]);
        }
        ("lpr", a)
    } else {
        return Ok((None, String::new()));
    };
    let o = std::process::Command::new(cmd).args(&args).arg(pdf).output()?;
    if !o.status.success() {
        return Err(Error::invalid(
            "print",
            format!("{cmd} failed: {} (the PDF is at {})", String::from_utf8_lossy(&o.stderr).trim(), pdf.display()),
        ));
    }
    Ok((Some(cmd.to_string()), String::from_utf8_lossy(&o.stdout).to_string()))
}

/// Print a journal (or some entries): render to PDF at `out` (or `<dir>/<name>-<time>.pdf`)
/// and hand it to `lp`/`lpr` when there is one (and `send`). Recorded as `journal.print`.
#[allow(clippy::too_many_arguments)]
pub fn print_journal(
    store: &Store,
    id: &str,
    entries: &[String],
    printer: Option<&str>,
    out: Option<&Path>,
    dir: &Path,
    send: bool,
    origin: &str,
) -> Result<Printed> {
    let t = Target::Journal { id: id.to_string(), entries: entries.to_vec() };
    let (_, what, name) = t.describe(store)?;
    let pdf = match out {
        Some(p) => p.to_path_buf(),
        None => {
            crate::paths::ensure_dir(dir)?;
            dir.join(format!("{}-{}.pdf", slug(&name), chrono::Local::now().format("%Y%m%d-%H%M%S")))
        }
    };
    write(store, &t, OutFormat::Pdf, &pdf)?;
    let (command, output) = if send { send_to_printer(&pdf, &name, printer)? } else { (None, String::new()) };
    let p = Printed { pdf, command, output, saved_only: !send };
    let mut params = t.params();
    params["pdf"] = p.pdf.display().to_string().into();
    params["command"] = p.command.clone().into();
    params["printer"] = printer.into();
    let summary = match &p.command {
        Some(c) => format!("print {what} with {c}"),
        None if !send => format!("print {what} to {} (saved only)", p.pdf.display()),
        None => format!("print {what} to {} (no printer command)", p.pdf.display()),
    };
    store.note_action(&NewAction::new("journal.print", origin, summary).params(params));
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::EntryFormat;

    #[test]
    fn every_format_writes() {
        let s = Store::open_in_memory().unwrap();
        s.add_author("Ada", None, &[]).unwrap();
        let p = s.create_project("Big Move!", "").unwrap();
        let j = s.create_journal("Log", Some(&p.id), EntryFormat::Markdown).unwrap();
        let e = s.add_entry(&j.id, "Day", "text", None).unwrap();
        let r = s.create_report("Weekly", "").unwrap();
        let dir = tempfile::tempdir().unwrap();
        for t in [
            Target::Journal { id: j.id.clone(), entries: vec![] },
            Target::Journal { id: j.id.clone(), entries: vec![e.id.clone()] },
            Target::Report(r.id.clone()),
            Target::Project { id: p.id.clone(), rows: 20, data: true },
        ] {
            for f in t.formats() {
                let out = dir.path().join(t.file_name(&s, f));
                let files = write_recorded(&s, &t, f, &out, "cli").unwrap();
                assert!(files[0].metadata().unwrap().len() > 0, "{t:?} {f:?}");
            }
        }
        assert!(write(&s, &Target::Journal { id: j.id.clone(), entries: vec![] }, OutFormat::Csv, &dir.path().join("x.csv")).is_err());
        assert!(dir.path().join("big-move.mnnp").exists());
        assert!(dir.path().join("day.pdf").exists());
        let h = s.actions().unwrap();
        assert!(h.iter().any(|a| a.kind == "entry.export" && a.summary.contains("entry Day of journal Log")));
        assert!(h.iter().any(|a| a.kind == "project.export" && a.summary.contains("(archive)")));
        assert!(h.iter().all(|a| a.author_id.is_some()), "authorship");
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("Weekly report: BTC!"), "weekly-report-btc");
        assert_eq!(slug("!!"), "export");
    }

    #[test]
    fn formats_parse() {
        for f in OutFormat::ALL {
            assert_eq!(OutFormat::parse(f.as_str()).unwrap(), f);
        }
        assert!(OutFormat::parse("rtf").is_err());
    }
}
