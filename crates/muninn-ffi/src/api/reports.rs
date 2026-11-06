//! Reports (`mnnn report` / TUI Reports): list, create, sections.
use super::data::open_write;
use muninn_core::store::{EntryFormat, NewAction, NewSection, Report, Section, SectionChange, SectionKind, Store};

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
    SectionKind::parse(kind.trim().to_ascii_lowercase().as_str()).ok_or_else(|| {
        anyhow::anyhow!("`{kind}`: a section is text, records, series, journal, project, authors, code or model")
    })
}

pub(crate) fn add_section_of(
    store: &Store,
    report: &str,
    kind: &str,
    title: &str,
    body: &str,
    format: &str,
    target: Option<&str>,
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
            return Err(anyhow::anyhow!("records sections stay in the TUI or `mnnn report add --records`"));
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

/// `mnnn report add` for text, journal, project, authors, series, code or model sections.
pub fn add_report_section(
    report: String,
    kind: String,
    title: String,
    body: String,
    format: String,
    target: Option<String>,
) -> anyhow::Result<SectionInfo> {
    add_section_of(&open_write()?, &report, &kind, &title, &body, &format, target.as_deref())
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
    note(store, "report.section.edit", format!("edit section {} of report {}", s.heading(), r.name), serde_json::json!({ "report": r.id, "section": s.id }));
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
    note(store, "report.section.remove", format!("remove section {heading} from report {}", r.name), serde_json::json!({ "report": r.id, "section": s.id }));
    Ok(format!("removed section {heading}"))
}

pub fn remove_report_section(report: String, section: String) -> anyhow::Result<String> {
    remove_section_of(&open_write()?, &report, &section)
}

pub(crate) fn move_section_of(store: &Store, report: &str, section: &str, to: usize) -> anyhow::Result<SectionInfo> {
    let r = store.find_report(report)?;
    let s = store.find_section(&r.id, section)?;
    let s = store.move_section(&s.id, to)?;
    note(store, "report.section.move", format!("move section {} in report {}", s.heading(), r.name), serde_json::json!({ "report": r.id, "section": s.id, "to": to }));
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
        let a = add_section_of(&s, &r.id, "text", "Intro", "hello", "markdown", None).unwrap();
        assert_eq!((a.kind.as_str(), a.title.as_str(), a.body.as_str(), a.position), ("text", "Intro", "hello", 1));
        let b = add_section_of(&s, &r.id, "authors", "Who", "", "", None).unwrap();
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
    }
}
