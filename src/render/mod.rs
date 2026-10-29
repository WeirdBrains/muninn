//! Documents for humans: one model, many formats. See `docs/export-formats.md`.
//!
//! Exporters build a [`Doc`]. [`latex::write`] turns it into LaTeX, the canonical source.
//! PDF and Docx are generated from that LaTeX: [`latex::read`] parses it back into a `Doc`
//! (the LaTeX Muninn writes, plus what journal entries commonly use) and [`pdf`] and
//! [`docx`] lay it out with fonts bundled in the binary, so no TeX or other tool is needed.
//! HTML and plain text are written from the model directly; JSON and CSV come from the data.
use std::collections::BTreeMap;

pub mod csv;
pub mod docx;
pub mod html;
pub mod image;
pub mod latex;
pub mod markdown;
pub mod pdf;
pub mod text;
pub mod zip;

/// A document.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Doc {
    pub title: String,
    pub authors: Vec<String>,
    pub date: String,
    pub blocks: Vec<Block>,
    /// Images by key (the key is what [`Block::Image`] refers to).
    pub images: BTreeMap<String, Image>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}

impl Image {
    /// File extension for the image type.
    pub fn ext(&self) -> &'static str {
        match self.mime.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/gif" => "gif",
            "image/webp" => "webp",
            "image/svg+xml" => "svg",
            "image/bmp" => "bmp",
            _ => "bin",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// Level 1 to 3.
    Heading(u8, Vec<Inline>),
    Para(Vec<Inline>),
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    /// Verbatim text in a monospace font.
    Code(String),
    /// Display math (LaTeX source).
    Math(String),
    Quote(Vec<Block>),
    Table {
        head: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Image {
        key: String,
        caption: String,
    },
    Rule,
    /// Numbered notes (made by [`endnotes`] from [`Inline::Note`]).
    Endnotes(Vec<(usize, Vec<Inline>)>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Mono(String),
    /// Inline math (LaTeX source).
    Math(String),
    /// Highlighted text.
    Mark(Vec<Inline>),
    Link(Vec<Inline>, String),
    /// A footnote.
    Note(Vec<Inline>),
    /// The number of a note (made by [`endnotes`]).
    NoteRef(usize),
    /// A line break.
    Break,
}

/// Plain text of inlines (formatting dropped).
pub fn plain(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) | Inline::Mono(t) | Inline::Math(t) => s.push_str(t),
            Inline::Bold(c) | Inline::Italic(c) | Inline::Mark(c) | Inline::Link(c, _) => s.push_str(&plain(c)),
            Inline::Note(_) => {}
            Inline::NoteRef(n) => s.push_str(&format!("[{n}]")),
            Inline::Break => s.push(' '),
        }
    }
    s
}

/// Text inline.
pub fn t(s: impl Into<String>) -> Inline {
    Inline::Text(s.into())
}

/// A paragraph of plain text.
pub fn para(s: impl Into<String>) -> Block {
    Block::Para(vec![t(s)])
}

/// An italic paragraph (meta lines: who wrote it, when).
pub fn meta(s: impl Into<String>) -> Block {
    Block::Para(vec![Inline::Italic(vec![t(s)])])
}

/// Turn footnotes into numbered references, with the notes listed at the end of each
/// top-level section (before the next level-1 heading) and at the end of the document.
pub fn endnotes(blocks: &[Block]) -> Vec<Block> {
    let mut out = vec![];
    let mut pending: Vec<(usize, Vec<Inline>)> = vec![];
    let mut n = 0;
    for b in blocks {
        if matches!(b, Block::Heading(1, _)) && !pending.is_empty() {
            out.push(Block::Endnotes(std::mem::take(&mut pending)));
        }
        out.push(number_block(b, &mut n, &mut pending));
    }
    if !pending.is_empty() {
        out.push(Block::Endnotes(pending));
    }
    out
}

fn number_block(b: &Block, n: &mut usize, notes: &mut Vec<(usize, Vec<Inline>)>) -> Block {
    let il = |v: &[Inline], n: &mut usize, notes: &mut Vec<(usize, Vec<Inline>)>| number_inlines(v, n, notes);
    match b {
        Block::Heading(l, v) => Block::Heading(*l, il(v, n, notes)),
        Block::Para(v) => Block::Para(il(v, n, notes)),
        Block::List { ordered, items } => {
            Block::List { ordered: *ordered, items: items.iter().map(|it| it.iter().map(|b| number_block(b, n, notes)).collect()).collect() }
        }
        Block::Quote(v) => Block::Quote(v.iter().map(|b| number_block(b, n, notes)).collect()),
        o => o.clone(),
    }
}

fn number_inlines(v: &[Inline], n: &mut usize, notes: &mut Vec<(usize, Vec<Inline>)>) -> Vec<Inline> {
    v.iter()
        .map(|i| match i {
            Inline::Note(c) => {
                *n += 1;
                let k = *n;
                let body = number_inlines(c, n, notes);
                notes.push((k, body));
                Inline::NoteRef(k)
            }
            Inline::Bold(c) => Inline::Bold(number_inlines(c, n, notes)),
            Inline::Italic(c) => Inline::Italic(number_inlines(c, n, notes)),
            Inline::Mark(c) => Inline::Mark(number_inlines(c, n, notes)),
            Inline::Link(c, u) => Inline::Link(number_inlines(c, n, notes), u.clone()),
            o => o.clone(),
        })
        .collect()
}

/// Raise heading levels by `by` (capped at 3), e.g. headings inside an entry body.
pub fn shift_headings(blocks: Vec<Block>, by: u8) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|b| match b {
            Block::Heading(l, v) => Block::Heading((l + by).min(3), v),
            Block::Quote(v) => Block::Quote(shift_headings(v, by)),
            o => o,
        })
        .collect()
}

/// Output formats for documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocFormat {
    Html,
    Latex,
    Pdf,
    Docx,
    Text,
}

impl DocFormat {
    pub fn ext(self) -> &'static str {
        match self {
            DocFormat::Html => "html",
            DocFormat::Latex => "tex",
            DocFormat::Pdf => "pdf",
            DocFormat::Docx => "docx",
            DocFormat::Text => "txt",
        }
    }
}

/// Render a document to bytes. LaTeX images are not included (see [`write_files`]);
/// PDF and Docx are generated from the LaTeX source.
pub fn render(doc: &Doc, f: DocFormat) -> crate::Result<Vec<u8>> {
    Ok(match f {
        DocFormat::Html => html::write(doc).into_bytes(),
        DocFormat::Text => text::write(doc, 100).into_bytes(),
        DocFormat::Latex => latex::write(doc, "").into_bytes(),
        DocFormat::Pdf => pdf::write(&from_latex(doc))?,
        DocFormat::Docx => docx::write(&from_latex(doc))?,
    })
}

/// The document as PDF and Docx see it: written to LaTeX and read back.
pub fn from_latex(doc: &Doc) -> Doc {
    latex::read(&latex::write(doc, ""), &doc.images)
}

/// Write a document to `out`; LaTeX also writes its images to `<stem>-images/`.
/// Returns the files written.
pub fn write_files(doc: &Doc, f: DocFormat, out: &std::path::Path) -> crate::Result<Vec<std::path::PathBuf>> {
    if let Some(p) = out.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(p)?;
    }
    let mut written = vec![out.to_path_buf()];
    if f == DocFormat::Latex {
        let stem = out.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "doc".into());
        let dir_name = format!("{stem}-images");
        std::fs::write(out, latex::write(doc, &dir_name))?;
        if !doc.images.is_empty() {
            let dir = out.with_file_name(&dir_name);
            std::fs::create_dir_all(&dir)?;
            for (k, im) in &doc.images {
                let p = dir.join(format!("{k}.{}", im.ext()));
                std::fs::write(&p, &im.data)?;
                written.push(p);
            }
        }
    } else {
        std::fs::write(out, render(doc, f)?)?;
    }
    Ok(written)
}

#[cfg(test)]
pub(crate) mod sample {
    use super::*;

    /// A tiny PNG (`w`×`h`, red).
    pub fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = vec![];
        {
            let mut e = ::png::Encoder::new(&mut out, w, h);
            e.set_color(::png::ColorType::Rgb);
            e.set_depth(::png::BitDepth::Eight);
            let mut wr = e.write_header().unwrap();
            wr.write_image_data(&[200u8, 30, 30].repeat((w * h) as usize)).unwrap();
        }
        out
    }

    /// A document using every block and inline kind.
    pub fn doc() -> Doc {
        let mut images = BTreeMap::new();
        images.insert("img1".to_string(), Image { name: "chart.png".into(), mime: "image/png".into(), data: png(8, 4) });
        Doc {
            title: "Weekly review & notes".into(),
            authors: vec!["Ada Lovelace".into(), "Grace Hopper".into()],
            date: "2026-10-29".into(),
            images,
            blocks: vec![
                Block::Heading(1, vec![t("Monday: BTC 50% up")]),
                Block::Para(vec![
                    t("BTC "),
                    Inline::Mark(vec![t("broke out")]),
                    t(" on "),
                    Inline::Bold(vec![t("thin "), Inline::Italic(vec![t("volume")])]),
                    t(", see "),
                    Inline::Link(vec![t("the chart")], "https://example.com/a_b?x=1&y=2".into()),
                    t(" and "),
                    Inline::Mono("fn main() {}".into()),
                    t(". Price $p = \\{x\\}$: "),
                    Inline::Math("x^2 + \\alpha".into()),
                    Inline::Note(vec![t("Checked twice — Ada")]),
                    t(" ~#_%&^ \\ done--now"),
                ]),
                Block::Para(vec![t("line one"), Inline::Break, t("line two")]),
                Block::Heading(2, vec![t("Plan")]),
                Block::List {
                    ordered: true,
                    items: vec![vec![para("buy")], vec![para("hold"), Block::List { ordered: false, items: vec![vec![para("nested")]] }]],
                },
                Block::Code("let x = 1;\n  {weird} \\stuff % here".into()),
                Block::Math("\\sum_{i=1}^{n} x_i".into()),
                Block::Quote(vec![para("Be fearful when others are greedy.")]),
                Block::Table {
                    head: vec!["time".into(), "close".into()],
                    rows: vec![vec!["2024-01-01".into(), "42,000.5".into()], vec!["2024-01-02".into(), "43 & more".into()]],
                },
                Block::Image { key: "img1".into(), caption: "BTC 5m".into() },
                Block::Rule,
                Block::Heading(3, vec![t("End")]),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_become_numbered_endnotes() {
        let d = vec![
            Block::Heading(1, vec![t("A")]),
            Block::Para(vec![t("x"), Inline::Note(vec![t("n1")]), Inline::Bold(vec![Inline::Note(vec![t("n2")])])]),
            Block::Heading(1, vec![t("B")]),
            Block::Para(vec![Inline::Note(vec![t("n3")])]),
        ];
        let e = endnotes(&d);
        assert_eq!(e.len(), 6);
        assert_eq!(e[2], Block::Endnotes(vec![(1, vec![t("n1")]), (2, vec![t("n2")])]));
        assert_eq!(e[4], Block::Para(vec![Inline::NoteRef(3)]));
        assert_eq!(plain(&[t("a"), Inline::NoteRef(2)]), "a[2]");
    }

    #[test]
    fn every_format_renders() {
        let d = sample::doc();
        for f in [DocFormat::Html, DocFormat::Latex, DocFormat::Pdf, DocFormat::Docx, DocFormat::Text] {
            let b = render(&d, f).unwrap();
            assert!(b.len() > 200, "{f:?}");
            // for looking at the output by hand: MUNINN_RENDER_OUT=/tmp/dir cargo test every_format
            if let Ok(dir) = std::env::var("MUNINN_RENDER_OUT") {
                std::fs::write(std::path::Path::new(&dir).join(format!("sample.{}", f.ext())), &b).unwrap();
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("x/review.tex");
        let w = write_files(&d, DocFormat::Latex, &out).unwrap();
        assert_eq!(w.len(), 2);
        assert!(dir.path().join("x/review-images/img1.png").exists());
        assert!(std::fs::read_to_string(&out).unwrap().contains("review-images/img1.png"));
    }
}
