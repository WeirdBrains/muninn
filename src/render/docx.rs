//! Docx: WordprocessingML written by hand into a ZIP package. Opens in Word, LibreOffice,
//! Pages and Google Docs. Headings, lists, tables, code, quotes, highlights, links and
//! PNG/JPEG/GIF/BMP images map to their Word equivalents; math is shown as readable text.
use super::{Block, Doc, Inline, endnotes, html::esc, text::math, zip::ZipWriter};
use crate::error::Result;

#[derive(Clone, Copy, Default)]
struct RPr {
    bold: bool,
    italic: bool,
    mono: bool,
    mark: bool,
    sup: bool,
    link: bool,
    small: bool,
}

impl RPr {
    fn xml(&self) -> String {
        let mut s = String::new();
        if self.link {
            s.push_str(r#"<w:rStyle w:val="Hyperlink"/>"#);
        }
        if self.mono {
            s.push_str(r#"<w:rFonts w:ascii="Liberation Mono" w:hAnsi="Liberation Mono" w:cs="Courier New"/>"#);
        }
        if self.bold {
            s.push_str("<w:b/>");
        }
        if self.italic {
            s.push_str("<w:i/>");
        }
        if self.link {
            s.push_str(r#"<w:color w:val="1F4E99"/><w:u w:val="single"/>"#);
        }
        if self.small {
            s.push_str(r#"<w:sz w:val="18"/>"#);
        }
        if self.mark {
            s.push_str(r#"<w:highlight w:val="yellow"/>"#);
        }
        if self.sup {
            s.push_str(r#"<w:vertAlign w:val="superscript"/>"#);
        }
        if s.is_empty() { s } else { format!("<w:rPr>{s}</w:rPr>") }
    }
}

struct Ctx<'a> {
    doc: &'a Doc,
    body: String,
    /// (id, type, target, external)
    rels: Vec<(String, &'static str, String, bool)>,
    media: Vec<(String, Vec<u8>)>,
    ordered: Vec<u32>,
    pic: u32,
}

const REL_LINK: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink";
const REL_IMAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image";
/// Text width of an A4 page with 2.5 cm margins, in EMU.
const TEXT_EMU: u64 = 5_727_700;

fn run(text: &str, r: RPr) -> String {
    format!(r#"<w:r>{}<w:t xml:space="preserve">{}</w:t></w:r>"#, r.xml(), esc(text))
}

impl Ctx<'_> {
    fn rel(&mut self, kind: &'static str, target: String, external: bool) -> String {
        let id = format!("rId{}", self.rels.len() + 10);
        self.rels.push((id.clone(), kind, target, external));
        id
    }

    fn runs(&mut self, v: &[Inline], r: RPr, out: &mut String) {
        for i in v {
            match i {
                Inline::Text(t) => out.push_str(&run(t, r)),
                Inline::Bold(c) => self.runs(c, RPr { bold: true, ..r }, out),
                Inline::Italic(c) => self.runs(c, RPr { italic: !r.italic, ..r }, out),
                Inline::Mono(t) => out.push_str(&run(t, RPr { mono: true, ..r })),
                Inline::Math(m) => out.push_str(&run(&math(m), RPr { italic: true, ..r })),
                Inline::Mark(c) => self.runs(c, RPr { mark: true, ..r }, out),
                Inline::Link(c, u) => {
                    let id = self.rel(REL_LINK, u.clone(), true);
                    out.push_str(&format!(r#"<w:hyperlink r:id="{id}">"#));
                    self.runs(c, RPr { link: true, ..r }, out);
                    out.push_str("</w:hyperlink>");
                }
                Inline::Note(c) => {
                    out.push_str(&run(" (", r));
                    self.runs(c, r, out);
                    out.push_str(&run(")", r));
                }
                Inline::NoteRef(n) => out.push_str(&run(&n.to_string(), RPr { sup: true, ..r })),
                Inline::Break => out.push_str("<w:r><w:br/></w:r>"),
            }
        }
    }

    fn para(&mut self, style: Option<&str>, ppr: &str, v: &[Inline], r: RPr) {
        let mut runs = String::new();
        self.runs(v, r, &mut runs);
        let st = style.map(|s| format!(r#"<w:pStyle w:val="{s}"/>"#)).unwrap_or_default();
        self.body.push_str(&format!("<w:p><w:pPr>{st}{ppr}</w:pPr>{runs}</w:p>"));
    }

    fn picture(&mut self, key: &str, caption: &str) -> bool {
        let Some(im) = self.doc.images.get(key) else { return false };
        if !matches!(im.mime.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/bmp") {
            return false;
        }
        let Some((w, h)) = super::image::size(&im.data) else { return false };
        let (mut cx, mut cy) = (w as u64 * 9525, h as u64 * 9525);
        if cx > TEXT_EMU {
            cy = cy * TEXT_EMU / cx.max(1);
            cx = TEXT_EMU;
        }
        self.pic += 1;
        let n = self.pic;
        let file = format!("image{n}.{}", im.ext());
        self.media.push((file.clone(), im.data.clone()));
        let id = self.rel(REL_IMAGE, format!("media/{file}"), false);
        let name = esc(&im.name);
        let descr = esc(caption);
        self.body.push_str(&format!(
            r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{n}" name="{name}" descr="{descr}"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="{n}" name="{name}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="{id}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
        ));
        true
    }

    fn block(&mut self, b: &Block, depth: usize, quote: bool) {
        let pstyle = if quote { Some("Quote") } else { None };
        match b {
            Block::Heading(l, v) => {
                let s = format!("Heading{}", (*l).clamp(1, 3));
                self.para(Some(&s), "", v, RPr::default());
            }
            Block::Para(v) => self.para(pstyle, "", v, RPr::default()),
            Block::List { ordered, items } => {
                let num = if *ordered {
                    let n = 2 + self.ordered.len() as u32;
                    self.ordered.push(n);
                    n
                } else {
                    1
                };
                let lvl = depth.min(8);
                for it in items {
                    let mut first = true;
                    for b in it {
                        match b {
                            Block::Para(v) if first => {
                                let ppr = format!(r#"<w:numPr><w:ilvl w:val="{lvl}"/><w:numId w:val="{num}"/></w:numPr>"#);
                                self.para(Some("ListParagraph"), &ppr, v, RPr::default());
                            }
                            Block::Para(v) => {
                                let ppr = format!(r#"<w:ind w:left="{}"/>"#, 720 * (lvl + 1));
                                self.para(Some("ListParagraph"), &ppr, v, RPr::default());
                            }
                            other => self.block(other, depth + 1, quote),
                        }
                        first = false;
                    }
                }
            }
            Block::Code(c) => {
                let mut runs = String::new();
                for (i, l) in c.lines().enumerate() {
                    if i > 0 {
                        runs.push_str("<w:r><w:br/></w:r>");
                    }
                    runs.push_str(&run(l, RPr::default()));
                }
                self.body.push_str(&format!(r#"<w:p><w:pPr><w:pStyle w:val="Code"/></w:pPr>{runs}</w:p>"#));
            }
            Block::Math(m) => self.para(Some("Math"), "", &[Inline::Text(math(m))], RPr { italic: true, ..RPr::default() }),
            Block::Quote(v) => {
                for b in v {
                    self.block(b, depth, true);
                }
            }
            Block::Table { head, rows } => {
                let n = rows.iter().map(|r| r.len()).chain([head.len()]).max().unwrap_or(0);
                if n == 0 {
                    return;
                }
                let b = r#"w:val="single" w:sz="4" w:space="0" w:color="999999""#;
                let mut s = format!(
                    r#"<w:tbl><w:tblPr><w:tblW w:w="5000" w:type="pct"/><w:tblBorders><w:top {b}/><w:left {b}/><w:bottom {b}/><w:right {b}/><w:insideH {b}/><w:insideV {b}/></w:tblBorders><w:tblCellMar><w:left w:w="80" w:type="dxa"/><w:right w:w="80" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid>"#
                );
                for _ in 0..n {
                    s.push_str(&format!(r#"<w:gridCol w:w="{}"/>"#, 9000 / n));
                }
                s.push_str("</w:tblGrid>");
                let row = |cells: &[String], header: bool| {
                    let mut r = String::from("<w:tr>");
                    if header {
                        r.push_str("<w:trPr><w:tblHeader/></w:trPr>");
                    }
                    for i in 0..n {
                        let c = cells.get(i).map(String::as_str).unwrap_or("");
                        let shade = if header { r#"<w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="EBEBEB"/></w:tcPr>"# } else { "" };
                        r.push_str(&format!(
                            r#"<w:tc>{shade}<w:p><w:pPr><w:pStyle w:val="TableText"/></w:pPr>{}</w:p></w:tc>"#,
                            run(c, RPr { bold: header, ..RPr::default() })
                        ));
                    }
                    r.push_str("</w:tr>");
                    r
                };
                if !head.is_empty() {
                    s.push_str(&row(head, true));
                }
                for r in rows {
                    s.push_str(&row(r, false));
                }
                s.push_str("</w:tbl><w:p/>");
                self.body.push_str(&s);
            }
            Block::Image { key, caption } => {
                if !self.picture(key, caption) {
                    let name = self.doc.images.get(key).map(|i| i.name.clone()).unwrap_or_else(|| key.clone());
                    let what = if self.doc.images.contains_key(key) { "this image type is not embedded in Docx" } else { "missing" };
                    self.para(Some("Caption"), "", &[Inline::Text(format!("[image {name}: {what}]"))], RPr::default());
                }
                if !caption.is_empty() {
                    self.para(Some("Caption"), "", &[Inline::Text(caption.clone())], RPr::default());
                }
            }
            Block::Rule => {
                self.body.push_str(r#"<w:p><w:pPr><w:pBdr><w:bottom w:val="single" w:sz="6" w:space="1" w:color="999999"/></w:pBdr></w:pPr></w:p>"#)
            }
            Block::Endnotes(v) => {
                for (n, c) in v {
                    let mut l = vec![Inline::NoteRef(*n), Inline::Text(" ".into())];
                    l.extend(c.iter().cloned());
                    self.para(Some("Note"), "", &l, RPr::default());
                }
            }
        }
    }
}

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing""#;
const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;

fn styles() -> String {
    let font = r#"<w:rFonts w:ascii="Liberation Serif" w:hAnsi="Liberation Serif" w:eastAsia="Liberation Serif" w:cs="Times New Roman"/>"#;
    let mono = r#"<w:rFonts w:ascii="Liberation Mono" w:hAnsi="Liberation Mono" w:cs="Courier New"/>"#;
    let p = |id: &str, name: &str, ppr: &str, rpr: &str| {
        format!(
            r#"<w:style w:type="paragraph" w:styleId="{id}"><w:name w:val="{name}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr>{ppr}</w:pPr><w:rPr>{rpr}</w:rPr></w:style>"#
        )
    };
    let mut s = format!(
        r#"{HEAD}<w:styles {NS}><w:docDefaults><w:rPrDefault><w:rPr>{font}<w:sz w:val="22"/><w:lang w:val="en-US"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="120" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style><w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:rPr><w:color w:val="1F4E99"/><w:u w:val="single"/></w:rPr></w:style>"#
    );
    s.push_str(&p("Title", "Title", r#"<w:jc w:val="center"/><w:spacing w:after="120"/>"#, r#"<w:b/><w:sz w:val="40"/>"#));
    s.push_str(&p("Subtitle", "Subtitle", r#"<w:jc w:val="center"/>"#, r#"<w:sz w:val="24"/>"#));
    for (i, sz) in [(1, 32), (2, 27), (3, 24)] {
        s.push_str(&p(
            &format!("Heading{i}"),
            &format!("heading {i}"),
            &format!(r#"<w:keepNext/><w:spacing w:before="240" w:after="80"/><w:outlineLvl w:val="{}"/>"#, i - 1),
            &format!(r#"<w:b/><w:sz w:val="{sz}"/>"#),
        ));
    }
    s.push_str(&p(
        "Code",
        "Code",
        r#"<w:shd w:val="clear" w:color="auto" w:fill="F2F2F2"/><w:spacing w:after="160" w:line="240" w:lineRule="auto"/>"#,
        &format!(r#"{mono}<w:sz w:val="18"/>"#),
    ));
    s.push_str(&p(
        "Quote",
        "Quote",
        r#"<w:ind w:left="567"/><w:pBdr><w:left w:val="single" w:sz="12" w:space="8" w:color="BFBFBF"/></w:pBdr>"#,
        r#"<w:color w:val="4D4D4D"/>"#,
    ));
    s.push_str(&p("Math", "Math", r#"<w:jc w:val="center"/>"#, "<w:i/>"));
    s.push_str(&p("Caption", "caption", r#"<w:jc w:val="center"/>"#, r#"<w:i/><w:sz w:val="19"/>"#));
    s.push_str(&p("Note", "Note", r#"<w:spacing w:after="40"/>"#, r#"<w:sz w:val="18"/>"#));
    s.push_str(&p("TableText", "Table Text", r#"<w:spacing w:after="0"/>"#, r#"<w:sz w:val="18"/>"#));
    s.push_str(&p("ListParagraph", "List Paragraph", r#"<w:spacing w:after="60"/>"#, ""));
    s.push_str("</w:styles>");
    s
}

fn numbering(ordered: &[u32]) -> String {
    let lvls = |ordered: bool| {
        (0..9)
            .map(|l| {
                let (fmt, text) = if ordered { ("decimal", format!("%{}.", l + 1)) } else { ("bullet", ["•", "◦", "▪"][l % 3].to_string()) };
                format!(
                    r#"<w:lvl w:ilvl="{l}"><w:start w:val="1"/><w:numFmt w:val="{fmt}"/><w:lvlText w:val="{text}"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="{}" w:hanging="360"/></w:pPr></w:lvl>"#,
                    720 * (l + 1)
                )
            })
            .collect::<String>()
    };
    let mut s = format!(
        r#"{HEAD}<w:numbering {NS}><w:abstractNum w:abstractNumId="0"><w:multiLevelType w:val="hybridMultilevel"/>{}</w:abstractNum><w:abstractNum w:abstractNumId="1"><w:multiLevelType w:val="hybridMultilevel"/>{}</w:abstractNum><w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num>"#,
        lvls(false),
        lvls(true)
    );
    for n in ordered {
        s.push_str(&format!(r#"<w:num w:numId="{n}"><w:abstractNumId w:val="1"/>"#));
        for l in 0..9 {
            s.push_str(&format!(r#"<w:lvlOverride w:ilvl="{l}"><w:startOverride w:val="1"/></w:lvlOverride>"#));
        }
        s.push_str("</w:num>");
    }
    s.push_str("</w:numbering>");
    s
}

/// Write the document as a .docx package.
pub fn write(doc: &Doc) -> Result<Vec<u8>> {
    let mut c = Ctx { doc, body: String::new(), rels: vec![], media: vec![], ordered: vec![], pic: 0 };
    if !doc.title.is_empty() {
        c.para(Some("Title"), "", &[Inline::Text(doc.title.clone())], RPr::default());
        if !doc.authors.is_empty() {
            c.para(Some("Subtitle"), "", &[Inline::Text(doc.authors.join(", "))], RPr::default());
        }
        if !doc.date.is_empty() {
            c.para(Some("Subtitle"), "", &[Inline::Text(doc.date.clone())], RPr { italic: true, ..RPr::default() });
        }
    }
    for b in &endnotes(&doc.blocks) {
        c.block(b, 0, false);
    }
    let document = format!(
        r#"{HEAD}<w:document {NS}><w:body>{}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1418" w:right="1418" w:bottom="1418" w:left="1418" w:header="709" w:footer="709" w:gutter="0"/></w:sectPr></w:body></w:document>"#,
        c.body
    );
    let mut rels = format!(
        r#"{HEAD}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#
    );
    for (id, kind, target, ext) in &c.rels {
        let mode = if *ext { r#" TargetMode="External""# } else { "" };
        rels.push_str(&format!(r#"<Relationship Id="{id}" Type="{kind}" Target="{}"{mode}/>"#, esc(target)));
    }
    rels.push_str("</Relationships>");
    let types = format!(
        r#"{HEAD}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="jpg" ContentType="image/jpeg"/><Default Extension="gif" ContentType="image/gif"/><Default Extension="bmp" ContentType="image/bmp"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/></Types>"#
    );
    let root_rels = format!(
        r#"{HEAD}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/></Relationships>"#
    );
    let core = format!(
        r#"{HEAD}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>{}</dc:title><dc:creator>{}</dc:creator></cp:coreProperties>"#,
        esc(&doc.title),
        esc(&doc.authors.join(", "))
    );
    let mut z = ZipWriter::default();
    z.add("[Content_Types].xml", types.as_bytes())?;
    z.add("_rels/.rels", root_rels.as_bytes())?;
    z.add("docProps/core.xml", core.as_bytes())?;
    z.add("word/document.xml", document.as_bytes())?;
    z.add("word/styles.xml", styles().as_bytes())?;
    z.add("word/numbering.xml", numbering(&c.ordered).as_bytes())?;
    z.add("word/_rels/document.xml.rels", rels.as_bytes())?;
    for (name, data) in &c.media {
        z.add(&format!("word/media/{name}"), data)?;
    }
    Ok(z.finish())
}

#[cfg(test)]
mod tests {
    use super::super::{sample, zip};
    use super::*;

    #[test]
    fn docx_package() {
        let b = write(&sample::doc()).unwrap();
        let files = zip::read(&b).expect("a zip");
        let get = |n: &str| files.iter().find(|f| f.0 == n).map(|f| String::from_utf8_lossy(&f.1).to_string());
        let d = get("word/document.xml").expect("document");
        assert!(d.contains("Weekly review &amp; notes"));
        assert!(d.contains(r#"w:val="Heading1""#));
        assert!(d.contains("<w:highlight"));
        assert!(d.contains("<w:hyperlink"));
        assert!(d.contains("<w:tbl>"));
        assert!(d.contains("<w:drawing>"));
        assert!(d.contains("superscript"), "footnote markers");
        assert!(files.iter().any(|f| f.0 == "word/media/image1.png"));
        let rels = get("word/_rels/document.xml.rels").unwrap();
        assert!(rels.contains("https://example.com/a_b?x=1&amp;y=2"));
        assert!(get("[Content_Types].xml").is_some());
        let n = get("word/numbering.xml").unwrap();
        assert!(n.contains(r#"<w:num w:numId="2">"#), "each ordered list restarts at 1");
        // every opened element is closed
        assert_eq!(d.matches("<w:p>").count() + d.matches("<w:p/>").count(), d.matches("</w:p>").count() + d.matches("<w:p/>").count());
    }
}
