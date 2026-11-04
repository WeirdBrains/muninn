//! Project archives (`.mnnp`, a zstd-compressed tar) and JSON packs of projects and
//! journals. An archive holds:
//!
//! ```text
//! muninn-project.json        the project, its items, journals, reports, live searches, authors
//! attachments/<id>-<name>    every image of every entry
//! data/<n>-<series>.csv      the bars the project uses (timestamp,open,high,low,close,volume,flags)
//! project.html               the whole project for reading in a browser
//! ```
//!
//! A JSON pack is the same manifest with attachments (base64) and bars inline.
use crate::error::{Error, Result};
use crate::model::Bar;
use crate::store::Store;
use crate::store::archive::{AttachmentPack, Blobs, DataPack, Inline, JournalFile, ProjectPack, Restored, bars_csv, parse_bars_csv};
use base64::Engine;
use std::collections::HashMap;
use std::path::Path;

pub const MANIFEST: &str = "muninn-project.json";
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// What was written.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Written {
    pub journals: usize,
    pub entries: usize,
    pub attachments: usize,
    pub reports: usize,
    pub live_searches: usize,
    pub series: usize,
    pub bars: usize,
    pub bytes: u64,
}

fn counts(p: &ProjectPack) -> Written {
    Written {
        journals: p.journals.len(),
        entries: p.journals.iter().map(|j| j.entries.len()).sum(),
        attachments: p.journals.iter().map(|j| j.attachments.len()).sum(),
        reports: p.reports.len(),
        live_searches: p.live_searches.len(),
        series: p.data.len(),
        bars: p.data.iter().map(|d| d.bars).sum(),
        bytes: 0,
    }
}

fn safe(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' }).collect();
    if s.is_empty() { "file".into() } else { s }
}

/// Fill a journal's attachment bytes inline (base64).
fn inline_attachments(store: &Store, atts: &mut [AttachmentPack]) -> Result<()> {
    for a in atts {
        if let Some((_, data)) = store.attachment_data(&a.meta.id)? {
            a.data = Some(base64::engine::general_purpose::STANDARD.encode(data));
        }
    }
    Ok(())
}

/// A project as one JSON document (attachments and bars inline).
pub fn project_json(store: &Store, id: &str, with_data: bool) -> Result<(String, Written)> {
    let mut p = store.project_pack(id, with_data)?;
    for j in &mut p.journals {
        inline_attachments(store, &mut j.attachments)?;
    }
    for d in &mut p.data {
        let bars = store.pack_bars(d)?;
        d.bars = bars.len();
        d.rows = Some(bars.iter().map(|b| (b.ts, b.open, b.high, b.low, b.close, b.volume, b.flags)).collect());
    }
    let s = serde_json::to_string_pretty(&p)?;
    let mut w = counts(&p);
    w.bytes = s.len() as u64;
    Ok((s, w))
}

/// A project as an archive (see the module docs).
pub fn project_archive(store: &Store, id: &str, with_data: bool, rows: usize) -> Result<(Vec<u8>, Written)> {
    let mut p = store.project_pack(id, with_data)?;
    let mut files: Vec<(String, Vec<u8>)> = vec![];
    for j in &mut p.journals {
        for a in &mut j.attachments {
            if let Some((_, data)) = store.attachment_data(&a.meta.id)? {
                let path = format!("attachments/{}-{}", a.meta.id, safe(&a.meta.name));
                files.push((path.clone(), data));
                a.file = Some(path);
            }
        }
    }
    for (n, d) in p.data.iter_mut().enumerate() {
        let bars = store.pack_bars(d)?;
        d.bars = bars.len();
        let path = format!("data/{:03}-{}.csv", n + 1, safe(&format!("{}-{}-{}", d.series.source, d.series.symbol.display(), d.series.duration)));
        files.push((path.clone(), bars_csv(&bars).into_bytes()));
        d.file = Some(path);
    }
    let html = crate::render::html::write(&super::docs::project(store, &p.project.id, rows)?);
    files.push(("project.html".into(), html.into_bytes()));
    files.insert(0, (MANIFEST.into(), serde_json::to_vec_pretty(&p)?));
    let mut out = Vec::new();
    {
        let enc = zstd::Encoder::new(&mut out, 3)?;
        let mut b = tar::Builder::new(enc);
        for (path, data) in &files {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(0o600);
            h.set_mtime((p.exported_at / 1000).max(0) as u64);
            h.set_cksum();
            b.append_data(&mut h, path, &data[..])?;
        }
        b.into_inner()?.finish()?;
    }
    let mut w = counts(&p);
    w.bytes = out.len() as u64;
    Ok((out, w))
}

/// Files of an archive by path.
struct Files(HashMap<String, Vec<u8>>);

impl Blobs for Files {
    fn attachment(&self, a: &AttachmentPack) -> Result<Vec<u8>> {
        match &a.file {
            Some(f) => self.0.get(f).cloned().ok_or_else(|| Error::invalid("archive", format!("{f} is missing"))),
            None => Inline.attachment(a),
        }
    }
    fn bars(&self, d: &DataPack) -> Result<Vec<Bar>> {
        match &d.file {
            Some(f) => {
                let b = self.0.get(f).ok_or_else(|| Error::invalid("archive", format!("{f} is missing")))?;
                parse_bars_csv(&String::from_utf8_lossy(b))
            }
            None => Inline.bars(d),
        }
    }
}

/// Read a project archive or JSON pack.
pub fn read_project(bytes: &[u8]) -> Result<(ProjectPack, Box<dyn Blobs>)> {
    if bytes.starts_with(&ZSTD_MAGIC) {
        let mut files = HashMap::new();
        let mut a = tar::Archive::new(zstd::Decoder::new(bytes)?);
        for e in a.entries()? {
            let mut e = e?;
            let path = e.path()?.to_string_lossy().trim_start_matches("./").to_string();
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut e, &mut data)?;
            files.insert(path, data);
        }
        let m = files.get(MANIFEST).ok_or_else(|| Error::invalid("archive", format!("{MANIFEST} is missing: not a Muninn project archive")))?;
        let p: ProjectPack = serde_json::from_slice(m).map_err(|e| Error::invalid("archive", format!("{MANIFEST}: {e}")))?;
        Ok((p, Box::new(Files(files))))
    } else {
        let p: ProjectPack =
            serde_json::from_slice(bytes).map_err(|e| Error::invalid("project file", format!("not a Muninn project archive or JSON pack ({e})")))?;
        Ok((p, Box::new(Inline)))
    }
}

/// Import a project archive or JSON pack.
pub fn import_project(store: &mut Store, path: &Path, name: Option<&str>) -> Result<Restored> {
    let bytes = std::fs::read(path).map_err(|e| Error::invalid("project file", format!("{}: {e}", path.display())))?;
    let (p, blobs) = read_project(&bytes)?;
    store.import_project(&p, name, blobs.as_ref())
}

/// A journal (or some of its entries) as JSON with images inline.
pub fn journal_json(store: &Store, id: &str, entries: &[String]) -> Result<String> {
    let mut f: JournalFile = store.journal_file(id)?;
    if !entries.is_empty() {
        f.pack.entries.retain(|e| entries.contains(&e.id));
        f.pack.annotations.retain(|a| a.entry_id.as_ref().is_none_or(|e| entries.contains(e)));
        f.pack.attachments.retain(|a| entries.contains(&a.meta.entry_id));
    }
    inline_attachments(store, &mut f.pack.attachments)?;
    Ok(serde_json::to_string_pretty(&f)?)
}

/// Import a journal written by [`journal_json`].
pub fn import_journal(store: &Store, path: &Path, project: Option<&str>, name: Option<&str>) -> Result<Restored> {
    let bytes = std::fs::read(path).map_err(|e| Error::invalid("journal file", format!("{}: {e}", path.display())))?;
    let f: JournalFile =
        serde_json::from_slice(&bytes).map_err(|e| Error::invalid("journal file", format!("not a Muninn journal JSON file ({e})")))?;
    store.import_journal(&f, project, name, &Inline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Duration, Symbol};
    use crate::store::{EntryFormat, ItemKind, NewSection, SearchSpec, SectionKind, SeriesRef};

    /// A project with a journal (entry, image, notes), a report, a live search and a result.
    fn setup() -> (Store, String) {
        let mut s = Store::open_in_memory().unwrap();
        s.add_author("Ada", Some("ada@example.com"), &[]).unwrap();
        let src = s.add_source("binance", "file", None).unwrap();
        let bars: Vec<Bar> = (0..12)
            .map(|i| Bar {
                ts: i * 60_000,
                open: 0.1 + i as f64,
                high: 2.0 / 3.0 + i as f64,
                low: 0.05,
                close: 1e-9 + i as f64,
                volume: 1.25,
                flags: 0,
            })
            .collect();
        s.upsert_bars(&src, &Symbol::new("BTC", "USDT"), Duration(60), &bars).unwrap();
        s.upsert_bars(&src, &Symbol::new("ETH", "USDT"), Duration(60), &bars[..4]).unwrap();
        let p = s.create_project("Halving", "what moved").unwrap();
        let j = s.create_journal("Log", Some(&p.id), EntryFormat::Markdown).unwrap();
        let e = s.add_entry(&j.id, "Day 1", "BTC broke out.", None).unwrap();
        let a = s.attach_image(&e.id, "chart.png", &crate::render::sample::png(3, 3)).unwrap();
        let body = format!("BTC broke out.\n\n![c](attachment:{})", a.id);
        s.update_entry(&e.id, None, Some(&body), None).unwrap();
        s.add_highlight(&e.id, crate::store::journals::select(&body, "broke out", 1).unwrap(), None, "big").unwrap();
        s.add_note(&j.id, None, None, "journal note").unwrap();
        let r = SeriesRef { source: "binance".into(), symbol: Symbol::new("BTC", "USDT"), duration: Duration(60), from: Some(0), to: Some(300_000) };
        s.add_project_result(&p.id, &r).unwrap();
        let spec = SearchSpec { query: "eth".into(), ..Default::default() };
        let l = s.save_live_search("eth", &spec, None).unwrap();
        s.add_project_item(&p.id, ItemKind::LiveSearch, &l.id, serde_json::json!({})).unwrap();
        let rep = s.create_report("Weekly", "").unwrap();
        s.add_section(&rep.id, &NewSection::text("Intro", "Hi", EntryFormat::Markdown)).unwrap();
        let mut js = NewSection::new(SectionKind::Journal);
        js.params.journal = Some(j.id.clone());
        js.params.entries = vec![e.id.clone()];
        s.add_section(&rep.id, &js).unwrap();
        let mut ss = NewSection::new(SectionKind::Series);
        ss.params.search = Some(l.id.clone());
        s.add_section(&rep.id, &ss).unwrap();
        s.add_project_item(&p.id, ItemKind::Report, &rep.id, serde_json::json!({})).unwrap();
        (s, p.id)
    }

    fn check_restored(src: &Store, dst: &Store, new_project: &str) {
        let items = dst.project_items(new_project).unwrap();
        assert_eq!(items.len(), 4, "{items:?}");
        let j = dst.journals().unwrap().into_iter().find(|j| j.project_id.as_deref() == Some(new_project)).unwrap();
        let e = &dst.entries(&j.id).unwrap()[0];
        let att = &dst.attachments(&e.id).unwrap()[0];
        assert!(e.body.contains(&format!("attachment:{}", att.id)), "image reference rewritten");
        let (_, data) = dst.attachment_data(&att.id).unwrap().unwrap();
        assert_eq!(data, crate::render::sample::png(3, 3));
        let anns = dst.annotations(&j.id).unwrap();
        assert_eq!(anns.len(), 2);
        assert!(anns.iter().all(|a| a.author_id.is_some()), "authorship kept");
        let rep = dst.reports().unwrap().into_iter().find(|r| items.iter().any(|i| i.ref_id == r.id)).unwrap();
        let secs = dst.sections(&rep.id).unwrap();
        assert_eq!(secs[1].params.journal.as_deref(), Some(j.id.as_str()));
        assert_eq!(secs[1].params.entries, vec![e.id.clone()]);
        assert!(dst.live_search(secs[2].params.search.as_deref().unwrap()).unwrap().is_some());
        // bars are exact
        let f = |s: &Store, sym: &str| {
            s.query_bars(
                &SeriesRef { source: "binance".into(), symbol: Symbol::new(sym, "USDT"), duration: Duration(60), from: None, to: None }.filter(),
            )
            .unwrap()
            .into_iter()
            .map(|b| b.bar)
            .collect::<Vec<_>>()
        };
        assert_eq!(f(dst, "ETH"), f(src, "ETH"), "live search matches travel whole");
        assert_eq!(f(dst, "BTC"), f(src, "BTC")[..6].to_vec(), "results travel with their range");
        // the restored report renders the same
        let a = crate::render::text::write(&super::super::docs::report(dst, &rep.id).unwrap(), 100);
        assert!(a.contains("broke out") && a.contains("big — Ada") && a.contains("ETH/USDT"), "{a}");
    }

    #[test]
    fn archives_round_trip() {
        let (s, pid) = setup();
        let (bytes, w) = project_archive(&s, &pid, true, 20).unwrap();
        assert_eq!((w.journals, w.entries, w.attachments, w.reports, w.live_searches, w.series), (1, 1, 1, 1, 1, 2));
        let (p, _) = read_project(&bytes).unwrap();
        assert_eq!(p.project.name, "Halving");
        let mut d = Store::open_in_memory().unwrap();
        let out = d.import_project(&p, None, read_project(&bytes).unwrap().1.as_ref()).unwrap();
        assert_eq!(out.authors_added, 1);
        let np = out.project.unwrap();
        assert_eq!(np.name, "Halving");
        check_restored(&s, &d, &np.id);
        // again: a second copy next to the first
        let again = d.import_project(&p, None, read_project(&bytes).unwrap().1.as_ref()).unwrap();
        assert_eq!(again.project.unwrap().name, "Halving (imported)");
        assert_eq!(again.authors_added, 0);
        // into the source store too
        let mut s2 = s;
        let back = s2.import_project(&p, Some("Copy"), read_project(&bytes).unwrap().1.as_ref()).unwrap();
        assert_eq!(back.project.unwrap().name, "Copy");
    }

    #[test]
    fn json_packs_round_trip() {
        let (s, pid) = setup();
        let (json, _) = project_json(&s, &pid, true).unwrap();
        let (p, blobs) = read_project(json.as_bytes()).unwrap();
        let mut d = Store::open_in_memory().unwrap();
        let np = d.import_project(&p, None, blobs.as_ref()).unwrap().project.unwrap();
        check_restored(&s, &d, &np.id);
        assert!(read_project(b"{}").is_err());
        let (nodata, w) = project_json(&s, &pid, false).unwrap();
        assert_eq!(w.series, 0);
        assert!(nodata.len() < json.len());
    }

    #[test]
    fn journals_round_trip() {
        let (s, _) = setup();
        let j = s.find_journal("Log").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("log.json");
        std::fs::write(&path, journal_json(&s, &j.id, &[]).unwrap()).unwrap();
        let d = Store::open_in_memory().unwrap();
        let out = import_journal(&d, &path, None, Some("Imported log")).unwrap();
        assert_eq!((out.entries, out.attachments), (1, 1));
        let nj = &out.journals[0];
        assert_eq!(nj.name, "Imported log");
        assert_eq!(d.annotations(&nj.id).unwrap().len(), 2);
        let txt = crate::render::text::write(&super::super::docs::journal(&d, &nj.id, &[]).unwrap(), 100);
        assert!(txt.contains("big — Ada"), "{txt}");
        // a bad file fails cleanly and leaves nothing behind
        std::fs::write(&path, "{\"format\":\"muninn-project\"}").unwrap();
        assert!(import_journal(&d, &path, None, None).is_err());
        assert_eq!(d.journals().unwrap().len(), 1);
    }
}
