//! Reports (`mnnn report` / TUI Reports): list, create, sections, preview and export.
use super::data::open_write;
use muninn_core::export::files::{self as export_files, OutFormat, Target};
use muninn_core::model::{Duration, Symbol};
use muninn_core::store::{EntryFormat, NewAction, NewSection, Report, Section, SectionChange, SectionKind, SeriesRef, Store};
use std::path::PathBuf;

pub struct ReportInfo {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub description: String,
    pub sections: i64,
}

pub struct SectionInfo {
    pub id: String,
    pub short_id: String,
    pub report_id: String,
    pub position: i64,
    pub kind: String,
    pub title: String,
    pub heading: String,
    pub body: String,
    pub format: String,
}

fn report_info(store: &Store, r: Report) -> anyhow::Result<ReportInfo> {
    let sections = store.sections(&r.id)?.len() as i64;
    Ok(ReportInfo { short_id: r.short_id().to_string(), id: r.id, name: r.name, description: r.description, sections })
}

fn section_info(s: Section) -> SectionInfo {
    SectionInfo {
        short_id: s.short_id().to_string(),
        heading: s.heading(),
        kind: s.kind.as_str().into(),
        format: s.format.as_str().into(),
        id: s.id,
        report_id: s.report_id,
        position: s.position,
        title: s.title,
        body: s.body,
    }
}

fn note(store: &Store, kind: &str, summary: String, params: serde_json::Value) {
    store.note_action(&NewAction::new(kind, "app", summary).params(params));
}

pub(crate) fn reports_of(store: &Store) -> anyhow::Result<Vec<ReportInfo>> {
    store.reports()?.into_iter().map(|r| report_info(store, r)).collect()
}

pub fn reports() -> anyhow::Result<Vec<ReportInfo>> {
    reports_of(&super::data::open()?)
}

pub(crate) fn create_report_of(store: &Store, name: &str, description: &str, project: Option<&str>) -> anyhow::Result<ReportInfo> {
    let r = store.create_report(name, description)?;
    note(store, "report.create", format!("create report {}", r.name), serde_json::json!({ "report": r.id, "name": r.name }));
    if let Some(p) = project.map(str::trim).filter(|s| !s.is_empty()) {
        let p = store.find_project(p)?;
        let (it, _) = store.add_project_item(&p.id, muninn_core::store::ItemKind::Report, &r.id, serde_json::json!({}))?;
        note(
            store,
            "project.add",
            format!("add report {} to project {}", r.name, p.name),
            serde_json::json!({ "project": p.id, "item": it.id, "kind": it.kind, "ref": r.id }),
        );
    }
    report_info(store, r)
}

pub fn create_report(name: String, description: String, project: Option<String>) -> anyhow::Result<ReportInfo> {
    create_report_of(&open_write()?, &name, &description, project.as_deref())
}

pub(crate) fn update_report_of(store: &Store, id: &str, name: Option<&str>, description: Option<&str>) -> anyhow::Result<ReportInfo> {
    let r = store.find_report(id)?;
    let r = store.update_report(&r.id, name.map(str::trim).filter(|s| !s.is_empty()), description)?;
    note(store, "report.edit", format!("edit report {}", r.name), serde_json::json!({ "report": r.id, "name": r.name }));
    report_info(store, r)
}

pub fn update_report(id: String, name: Option<String>, description: Option<String>) -> anyhow::Result<ReportInfo> {
    update_report_of(&open_write()?, &id, name.as_deref(), description.as_deref())
}

pub(crate) fn remove_report_of(store: &Store, id: &str) -> anyhow::Result<String> {
    let r = store.find_report(id)?;
    let name = r.name.clone();
    store.remove_report(&r.id)?;
    note(store, "report.remove", format!("remove report {name}"), serde_json::json!({ "report": r.id, "name": name }));
    Ok(format!("removed report {name}"))
}

pub fn remove_report(id: String) -> anyhow::Result<String> {
    remove_report_of(&open_write()?, &id)
}

pub(crate) fn sections_of(store: &Store, report: &str) -> anyhow::Result<Vec<SectionInfo>> {
    let r = store.find_report(report)?;
    Ok(store.sections(&r.id)?.into_iter().map(section_info).collect())
}

pub fn report_sections(report: String) -> anyhow::Result<Vec<SectionInfo>> {
    sections_of(&super::data::open()?, &report)
}

fn parse_kind(kind: &str) -> anyhow::Result<SectionKind> {
    SectionKind::parse(kind.trim().to_ascii_lowercase().as_str())
        .ok_or_else(|| anyhow::anyhow!("`{kind}`: a section is text, records, series, journal, project, authors, code or model"))
}

fn parse_series_ref(store: &Store, s: &str, from: Option<i64>, to: Option<i64>) -> anyhow::Result<SeriesRef> {
    let parts: Vec<&str> = s.split(':').collect();
    let [src, sym, per] = parts.as_slice() else {
        return Err(anyhow::anyhow!("invalid series `{s}` (use SOURCE:SYMBOL:PERIOD, e.g. binance:BTC/USDT:5m)"));
    };
    let r = SeriesRef {
        source: src.trim().to_string(),
        symbol: store.canonical_symbol(&Symbol::parse(sym)?)?,
        duration: Duration::parse(per)?,
        from,
        to,
    };
    if !store.series()?.iter().any(|x| x.source == r.source && x.symbol == r.symbol && x.duration == r.duration) {
        return Err(anyhow::anyhow!("no series {} (see Data or `mnnn search find`)", r.label()));
    }
    Ok(r)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn add_section_of(
    store: &Store,
    report: &str,
    kind: &str,
    title: &str,
    body: &str,
    format: &str,
    target: Option<&str>,
    from: Option<i64>,
    to: Option<i64>,
    rows: Option<i64>,
) -> anyhow::Result<SectionInfo> {
    let r = store.find_report(report)?;
    let kind = parse_kind(kind)?;
    let mut s = match kind {
        SectionKind::Text => {
            let format = if format.trim().is_empty() { EntryFormat::Markdown } else { EntryFormat::parse(format)? };
            NewSection::text(title, body, format)
        }
        SectionKind::Authors => NewSection::new(SectionKind::Authors),
        SectionKind::Journal => {
            let key = target.map(str::trim).filter(|x| !x.is_empty()).ok_or_else(|| anyhow::anyhow!("name the journal this section shows"))?;
            let j = store.find_journal(key)?;
            let mut s = NewSection::new(SectionKind::Journal);
            s.params.journal = Some(j.id);
            s
        }
        SectionKind::Project => {
            let key = target.map(str::trim).filter(|x| !x.is_empty()).ok_or_else(|| anyhow::anyhow!("name the project this section shows"))?;
            let p = store.find_project(key)?;
            let mut s = NewSection::new(SectionKind::Project);
            s.params.project = Some(p.id);
            s
        }
        SectionKind::Series => {
            let mut s = NewSection::new(SectionKind::Series);
            if let Some(q) = target.map(str::trim).filter(|x| !x.is_empty()) {
                s.params.search = Some(store.find_live_search(q)?.id);
            }
            s
        }
        SectionKind::Code => {
            let key = target.map(str::trim).filter(|x| !x.is_empty()).ok_or_else(|| anyhow::anyhow!("name the code space"))?;
            let c = store.find_code_space(key)?;
            let mut s = NewSection::new(SectionKind::Code);
            s.params.space = Some(c.id);
            s
        }
        SectionKind::Model => {
            let key = target.map(str::trim).filter(|x| !x.is_empty()).ok_or_else(|| anyhow::anyhow!("name the model"))?;
            let m = store.find_model(key)?;
            let mut s = NewSection::new(SectionKind::Model);
            s.params.model = Some(m.id);
            s
        }
        SectionKind::Records => {
            let key = target
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .ok_or_else(|| anyhow::anyhow!("name the series as SOURCE:SYMBOL:PERIOD (e.g. binance:BTC/USDT:5m)"))?;
            let mut s = NewSection::new(SectionKind::Records);
            s.params.series = Some(parse_series_ref(store, key, from, to)?);
            if let Some(n) = rows {
                s.params.rows = Some(n.max(0) as usize);
            }
            s
        }
    };
    if !title.trim().is_empty() && s.title.is_empty() {
        s.title = title.to_string();
    }
    let sec = store.add_section(&r.id, &s)?;
    note(
        store,
        "report.section.add",
        format!("add {} section to report {}", sec.kind.as_str(), r.name),
        serde_json::json!({ "report": r.id, "section": sec.id, "kind": sec.kind.as_str() }),
    );
    Ok(section_info(sec))
}

/// `mnnn report add` for text, records, journal, project, authors, series, code or model sections.
#[allow(clippy::too_many_arguments)]
pub fn add_report_section(
    report: String,
    kind: String,
    title: String,
    body: String,
    format: String,
    target: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
    rows: Option<i64>,
) -> anyhow::Result<SectionInfo> {
    add_section_of(&open_write()?, &report, &kind, &title, &body, &format, target.as_deref(), from, to, rows)
}

pub struct ReportExportInfo {
    pub path: String,
    pub files: i64,
    pub message: String,
}

pub(crate) fn export_report_of(store: &Store, report: &str, format: &str, out: &str) -> anyhow::Result<ReportExportInfo> {
    let r = store.find_report(report)?;
    let fmt = OutFormat::parse(format)?;
    let dest = PathBuf::from(out);
    let files = export_files::write_recorded(store, &Target::Report(r.id.clone()), fmt, &dest, "app")?;
    let n = files.len() as i64;
    Ok(ReportExportInfo {
        path: dest.display().to_string(),
        files: n,
        message: format!("exported report {} to {} ({})", r.name, dest.display(), fmt.as_str()),
    })
}

/// Export a report as PDF, Word, HTML, LaTeX, text, JSON or CSV (`mnnn report export` / TUI `E`). File bytes stay in Rust.
pub fn export_report(report: String, format: String, out: String) -> anyhow::Result<ReportExportInfo> {
    export_report_of(&open_write()?, &report, &format, &out)
}

pub(crate) fn preview_report_of(store: &Store, report: &str, width: i64) -> anyhow::Result<String> {
    let r = store.find_report(report)?;
    let doc = muninn_core::export::docs::report(store, &r.id)?;
    Ok(muninn_core::render::text::write(&doc, width.max(40) as usize))
}

/// Print a report as text (`mnnn report preview` / TUI `v`). No file is written.
pub fn preview_report(report: String, width: i64) -> anyhow::Result<String> {
    preview_report_of(&super::data::open()?, &report, width)
}

pub(crate) fn update_section_of(
    store: &Store,
    report: &str,
    section: &str,
    title: Option<&str>,
    body: Option<&str>,
    format: Option<&str>,
) -> anyhow::Result<SectionInfo> {
    let r = store.find_report(report)?;
    let s = store.find_section(&r.id, section)?;
    let format = format.map(EntryFormat::parse).transpose()?;
    let s = store.update_section(&s.id, &SectionChange { title: title.map(str::to_string), body: body.map(str::to_string), format, params: None })?;
    note(
        store,
        "report.section.edit",
        format!("edit section {} of report {}", s.heading(), r.name),
        serde_json::json!({ "report": r.id, "section": s.id }),
    );
    Ok(section_info(s))
}

pub fn update_report_section(
    report: String,
    section: String,
    title: Option<String>,
    body: Option<String>,
    format: Option<String>,
) -> anyhow::Result<SectionInfo> {
    update_section_of(&open_write()?, &report, &section, title.as_deref(), body.as_deref(), format.as_deref())
}

pub(crate) fn remove_section_of(store: &Store, report: &str, section: &str) -> anyhow::Result<String> {
    let r = store.find_report(report)?;
    let s = store.find_section(&r.id, section)?;
    let heading = s.heading();
    store.remove_section(&s.id)?;
    note(
        store,
        "report.section.remove",
        format!("remove section {heading} from report {}", r.name),
        serde_json::json!({ "report": r.id, "section": s.id }),
    );
    Ok(format!("removed section {heading}"))
}

pub fn remove_report_section(report: String, section: String) -> anyhow::Result<String> {
    remove_section_of(&open_write()?, &report, &section)
}

pub(crate) fn move_section_of(store: &Store, report: &str, section: &str, to: usize) -> anyhow::Result<SectionInfo> {
    let r = store.find_report(report)?;
    let s = store.find_section(&r.id, section)?;
    let s = store.move_section(&s.id, to)?;
    note(
        store,
        "report.section.move",
        format!("move section {} in report {}", s.heading(), r.name),
        serde_json::json!({ "report": r.id, "section": s.id, "to": to }),
    );
    Ok(section_info(s))
}

pub fn move_report_section(report: String, section: String, to: i64) -> anyhow::Result<SectionInfo> {
    move_section_of(&open_write()?, &report, &section, to.max(1) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muninn_core::store::Store;

    #[test]
    fn create_text_section_and_move() {
        let s = Store::open_in_memory().unwrap();
        let r = create_report_of(&s, "Weekly", "notes", None).unwrap();
        assert_eq!((r.name.as_str(), r.description.as_str(), r.sections), ("Weekly", "notes", 0));
        let a = add_section_of(&s, &r.id, "text", "Intro", "hello", "markdown", None, None, None, None).unwrap();
        assert_eq!((a.kind.as_str(), a.title.as_str(), a.body.as_str(), a.position), ("text", "Intro", "hello", 1));
        let b = add_section_of(&s, &r.id, "authors", "Who", "", "", None, None, None, None).unwrap();
        assert_eq!(b.kind, "authors");
        let b = move_section_of(&s, &r.id, &b.id, 1).unwrap();
        assert_eq!(b.position, 1);
        assert!(remove_section_of(&s, &r.id, &a.id).unwrap().contains("Intro"));
        let r = update_report_of(&s, &r.id, Some("Month"), None).unwrap();
        assert_eq!(r.name, "Month");
        assert!(remove_report_of(&s, &r.id).unwrap().contains("Month"));
    }

    #[test]
    fn writes_need_unlock() {
        crate::api::data::lock();
        assert!(create_report("X".into(), "".into(), None).is_err());
        assert!(reports().is_err());
        assert!(export_report("x".into(), "json".into(), "/tmp/x.json".into()).is_err());
        assert!(preview_report("x".into(), 80).is_err());
    }

    fn store_with_bars() -> Store {
        let mut s = Store::open_in_memory().unwrap();
        let src = s.add_source("coinbase", "csv", None).unwrap();
        let t0 = 1_704_067_200_000i64;
        let bars = vec![muninn_core::model::Bar { ts: t0, open: 1.0, high: 2.0, low: 0.5, close: 1.5, volume: 3.0, flags: 0 }];
        s.upsert_bars(&src, &muninn_core::model::Symbol::new("BTC", "USD"), muninn_core::model::Duration(300), &bars).unwrap();
        s
    }

    #[test]
    fn records_section_preview_and_export() {
        let s = store_with_bars();
        let r = create_report_of(&s, "Weekly", "notes", None).unwrap();
        let missing = add_section_of(&s, &r.id, "records", "Bars", "", "", None, None, None, None).map(|_| ()).unwrap_err();
        assert!(missing.to_string().contains("SOURCE:SYMBOL:PERIOD"), "{missing}");
        let unknown = add_section_of(&s, &r.id, "records", "Bars", "", "", Some("nope:FOO/BAR:1m"), None, None, None).map(|_| ()).unwrap_err();
        assert!(unknown.to_string().contains("no series"), "{unknown}");
        let from = 1_704_067_200_000i64;
        let a = add_section_of(&s, &r.id, "records", "Bars", "", "", Some("coinbase:BTC/USD:5m"), Some(from), None, Some(10)).unwrap();
        assert_eq!(a.kind, "records");
        assert_eq!(a.title, "Bars");
        let text = preview_report_of(&s, &r.id, 80).unwrap();
        assert!(text.contains("Weekly") || text.contains("Bars") || text.contains("BTC"), "preview was: {text}");
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("weekly.json");
        let exp = export_report_of(&s, &r.id, "json", out.to_str().unwrap()).unwrap();
        assert!(out.is_file());
        assert!(exp.message.contains("json"));
        assert!(exp.files >= 1);
        let body = std::fs::read_to_string(&out).unwrap();
        assert!(body.contains("Weekly") || body.contains("Bars") || body.contains("BTC"), "{body}");
    }
}
