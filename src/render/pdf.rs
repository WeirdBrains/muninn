//! PDF: a small layout engine with fonts bundled in the binary (Liberation Serif and Mono,
//! SIL Open Font License, subset into each file). No external tools are involved.
//!
//! A4 pages, 2.5 cm margins, 11 pt serif text, greedy line breaking, page numbers. Headings
//! stay with what follows, tables repeat their header on new pages, PNG and JPEG images are
//! embedded (other image types are named in the text), links are clickable, highlights get
//! a yellow background, footnotes become numbered notes at the end of each section.
use super::{Block, Doc, Inline, endnotes, text::math};
use crate::error::{Error, Result};
use pdf_writer::types::{ActionType, AnnotationType, CidFontType, FontFlags, SystemInfo, UnicodeCmap};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use std::collections::{BTreeMap, HashMap};
use subsetter::GlyphRemapper;

static FONTS: [&[u8]; 5] = [
    include_bytes!("../../assets/fonts/LiberationSerif-Regular.ttf"),
    include_bytes!("../../assets/fonts/LiberationSerif-Bold.ttf"),
    include_bytes!("../../assets/fonts/LiberationSerif-Italic.ttf"),
    include_bytes!("../../assets/fonts/LiberationSerif-BoldItalic.ttf"),
    include_bytes!("../../assets/fonts/LiberationMono-Regular.ttf"),
];
const NAMES: [&str; 5] = ["LiberationSerif", "LiberationSerif-Bold", "LiberationSerif-Italic", "LiberationSerif-BoldItalic", "LiberationMono"];
const MONO: usize = 4;

const W: f32 = 595.28;
const H: f32 = 841.89;
const MARGIN: f32 = 72.0;
const TOP: f32 = H - MARGIN;
const BOTTOM: f32 = MARGIN;
const BODY: f32 = 11.0;
const BLACK: (f32, f32, f32) = (0.1, 0.1, 0.1);
const LINK: (f32, f32, f32) = (0.1, 0.25, 0.6);
const MARK: (f32, f32, f32) = (1.0, 0.95, 0.48);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    mono: bool,
    size: f32,
    rise: f32,
    mark: bool,
    color: (f32, f32, f32),
    link: Option<usize>,
}

impl Style {
    fn body(size: f32) -> Self {
        Style { bold: false, italic: false, mono: false, size, rise: 0.0, mark: false, color: BLACK, link: None }
    }
    fn face(&self) -> usize {
        if self.mono { MONO } else { usize::from(self.bold) + 2 * usize::from(self.italic) }
    }
}

enum Frag {
    Text(String, Style),
    Break,
}

enum Tok {
    Word(Vec<(String, Style)>),
    Space(Style),
    Break,
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Center,
}

enum Op {
    Text { x: f32, y: f32, face: usize, size: f32, color: (f32, f32, f32), text: String },
    Rect { x: f32, y: f32, w: f32, h: f32, color: (f32, f32, f32) },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, gray: f32 },
    Image { idx: usize, x: f32, y: f32, w: f32, h: f32 },
    Link { x: f32, y: f32, w: f32, h: f32, link: usize },
}

struct Fonts {
    faces: Vec<ttf_parser::Face<'static>>,
    cache: Vec<HashMap<char, (u16, f32)>>,
}

impl Fonts {
    fn load() -> Result<Self> {
        let faces = FONTS
            .iter()
            .map(|d| ttf_parser::Face::parse(d, 0).map_err(|e| Error::invalid("bundled font", e.to_string())))
            .collect::<Result<Vec<_>>>()?;
        Ok(Fonts { cache: vec![HashMap::new(); faces.len()], faces })
    }
    /// Glyph id and advance (in em) for a character.
    fn glyph(&mut self, face: usize, c: char) -> (u16, f32) {
        if let Some(g) = self.cache[face].get(&c) {
            return *g;
        }
        let f = &self.faces[face];
        let c2 = if c == '\u{a0}' { ' ' } else { c };
        let gid = f.glyph_index(c2).or_else(|| f.glyph_index('?')).unwrap_or(ttf_parser::GlyphId(0));
        let adv = f.glyph_hor_advance(gid).unwrap_or(0) as f32 / f.units_per_em() as f32;
        self.cache[face].insert(c, (gid.0, adv));
        (gid.0, adv)
    }
    fn width(&mut self, face: usize, s: &str, size: f32) -> f32 {
        s.chars().filter(|c| !c.is_control()).map(|c| self.glyph(face, c).1).sum::<f32>() * size
    }
}

struct PdfImage {
    w: u32,
    h: u32,
    data: Vec<u8>,
    dct: bool,
    comps: u8,
    alpha: Option<Vec<u8>>,
}

fn zlib(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

fn decode(im: &super::Image) -> Option<PdfImage> {
    match im.mime.as_str() {
        "image/jpeg" => {
            let (w, h, comps) = super::image::jpeg(&im.data)?;
            Some(PdfImage { w, h, data: im.data.clone(), dct: true, comps, alpha: None })
        }
        "image/png" => {
            let mut d = png::Decoder::new(std::io::Cursor::new(&im.data));
            d.set_transformations(png::Transformations::normalize_to_color8());
            let mut r = d.read_info().ok()?;
            let mut buf = vec![0; r.output_buffer_size()?];
            let info = r.next_frame(&mut buf).ok()?;
            buf.truncate(info.buffer_size());
            let (w, h) = (info.width, info.height);
            let split = |n: usize| -> (Vec<u8>, Vec<u8>) {
                let mut c = Vec::with_capacity(buf.len());
                let mut a = Vec::with_capacity(buf.len() / n);
                for px in buf.chunks(n) {
                    c.extend_from_slice(&px[..n - 1]);
                    a.push(px[n - 1]);
                }
                (c, a)
            };
            let (data, comps, alpha) = match info.color_type {
                png::ColorType::Rgb => (buf.clone(), 3, None),
                png::ColorType::Grayscale => (buf.clone(), 1, None),
                png::ColorType::Rgba => {
                    let (c, a) = split(4);
                    (c, 3, Some(a))
                }
                png::ColorType::GrayscaleAlpha => {
                    let (c, a) = split(2);
                    (c, 1, Some(a))
                }
                png::ColorType::Indexed => return None,
            };
            Some(PdfImage { w, h, data: zlib(&data), dct: false, comps, alpha: alpha.map(|a| zlib(&a)) })
        }
        _ => None,
    }
}

struct Layout<'a> {
    fonts: Fonts,
    doc: &'a Doc,
    pages: Vec<Vec<Op>>,
    y: f32,
    images: Vec<PdfImage>,
    image_index: HashMap<String, Option<usize>>,
    links: Vec<String>,
    quote: usize,
}

impl<'a> Layout<'a> {
    fn page(&mut self) -> &mut Vec<Op> {
        self.pages.last_mut().expect("a page")
    }
    fn new_page(&mut self) {
        self.pages.push(vec![]);
        self.y = TOP;
    }
    fn at_top(&self) -> bool {
        self.y >= TOP - 0.1
    }
    fn ensure(&mut self, h: f32) {
        if self.y - h < BOTTOM && !self.at_top() {
            self.new_page();
        }
    }
    fn gap(&mut self, h: f32) {
        if !self.at_top() {
            self.y -= h;
        }
    }

    fn frags(&mut self, v: &[Inline], st: Style, out: &mut Vec<Frag>) {
        for i in v {
            match i {
                Inline::Text(t) => out.push(Frag::Text(t.clone(), st)),
                Inline::Bold(c) => self.frags(c, Style { bold: true, ..st }, out),
                Inline::Italic(c) => self.frags(c, Style { italic: !st.italic, ..st }, out),
                Inline::Mono(t) => out.push(Frag::Text(t.clone(), Style { mono: true, size: st.size * 0.9, ..st })),
                Inline::Math(m) => out.push(Frag::Text(math(m), Style { italic: true, ..st })),
                Inline::Mark(c) => self.frags(c, Style { mark: true, ..st }, out),
                Inline::Link(c, u) => {
                    self.links.push(u.clone());
                    let l = self.links.len() - 1;
                    self.frags(c, Style { link: Some(l), color: LINK, ..st }, out);
                }
                Inline::Note(c) => {
                    out.push(Frag::Text(" (".into(), st));
                    self.frags(c, st, out);
                    out.push(Frag::Text(")".into(), st));
                }
                Inline::NoteRef(n) => out.push(Frag::Text(n.to_string(), Style { size: st.size * 0.7, rise: st.size * 0.35, ..st })),
                Inline::Break => out.push(Frag::Break),
            }
        }
    }

    fn tokens(frags: Vec<Frag>) -> Vec<Tok> {
        let mut out: Vec<Tok> = vec![];
        for f in frags {
            match f {
                Frag::Break => out.push(Tok::Break),
                Frag::Text(t, st) => {
                    for c in t.chars() {
                        if c.is_whitespace() && c != '\u{a0}' {
                            if !matches!(out.last(), Some(Tok::Space(_)) | None | Some(Tok::Break)) {
                                out.push(Tok::Space(st));
                            }
                        } else if c.is_control() {
                        } else {
                            match out.last_mut() {
                                Some(Tok::Word(w)) => match w.last_mut() {
                                    Some((s, s2)) if *s2 == st => s.push(c),
                                    _ => w.push((c.to_string(), st)),
                                },
                                _ => out.push(Tok::Word(vec![(c.to_string(), st)])),
                            }
                        }
                    }
                }
            }
        }
        out
    }

    fn word_width(&mut self, w: &[(String, Style)]) -> f32 {
        w.iter().map(|(s, st)| self.fonts.width(st.face(), s, st.size)).sum()
    }

    /// Lay out inline content in `x0..x0+width`.
    fn flow(&mut self, v: &[Inline], st: Style, x0: f32, width: f32, align: Align) {
        let mut fr = vec![];
        self.frags(v, st, &mut fr);
        let toks = Self::tokens(fr);
        // break into lines
        let mut lines: Vec<Vec<Tok>> = vec![];
        let mut cur: Vec<Tok> = vec![];
        let mut cur_w = 0.0;
        let mut space: Option<Style> = None;
        for t in toks {
            match t {
                Tok::Space(s) => {
                    if !cur.is_empty() {
                        space = Some(s);
                    }
                }
                Tok::Break => {
                    lines.push(std::mem::take(&mut cur));
                    cur_w = 0.0;
                    space = None;
                }
                Tok::Word(w) => {
                    let ww = self.word_width(&w);
                    let sw = space.map(|s| self.fonts.width(s.face(), " ", s.size)).unwrap_or(0.0);
                    if !cur.is_empty() && cur_w + sw + ww > width {
                        lines.push(std::mem::take(&mut cur));
                        cur_w = 0.0;
                        space = None;
                    }
                    if ww > width {
                        // a word wider than the line: break it by characters
                        let mut piece: Vec<(String, Style)> = vec![];
                        let mut pw = 0.0;
                        for (s, st) in w {
                            for c in s.chars() {
                                let cw = self.fonts.width(st.face(), &c.to_string(), st.size);
                                if pw + cw > width && !piece.is_empty() {
                                    lines.push(vec![Tok::Word(std::mem::take(&mut piece))]);
                                    pw = 0.0;
                                }
                                match piece.last_mut() {
                                    Some((p, ps)) if *ps == st => p.push(c),
                                    _ => piece.push((c.to_string(), st)),
                                }
                                pw += cw;
                            }
                        }
                        cur = vec![Tok::Word(piece)];
                        cur_w = pw;
                        space = None;
                        continue;
                    }
                    if let Some(s) = space.take()
                        && !cur.is_empty()
                    {
                        cur.push(Tok::Space(s));
                        cur_w += sw;
                    }
                    cur.push(Tok::Word(w));
                    cur_w += ww;
                }
            }
        }
        if !cur.is_empty() || lines.is_empty() {
            lines.push(cur);
        }
        for line in lines {
            self.draw_line(line, st, x0, width, align);
        }
    }

    fn draw_line(&mut self, line: Vec<Tok>, st: Style, x0: f32, width: f32, align: Align) {
        let max = line
            .iter()
            .filter_map(|t| match t {
                Tok::Word(w) => w.iter().map(|p| p.1.size).reduce(f32::max),
                _ => None,
            })
            .fold(st.size, f32::max);
        let lh = max * 1.32;
        self.ensure(lh);
        let base = self.y - max * 0.92;
        let mut lw = 0.0;
        for t in &line {
            lw += match t {
                Tok::Word(w) => self.word_width(w),
                Tok::Space(s) => self.fonts.width(s.face(), " ", s.size),
                Tok::Break => 0.0,
            };
        }
        let mut x = if align == Align::Center { x0 + (width - lw).max(0.0) / 2.0 } else { x0 };
        let mut back = vec![];
        let mut front = vec![];
        let n = line.len();
        for (k, t) in line.into_iter().enumerate() {
            match t {
                Tok::Space(s) => {
                    let w = self.fonts.width(s.face(), " ", s.size);
                    if s.mark && k + 1 < n {
                        back.push(Op::Rect { x, y: base - s.size * 0.25, w, h: s.size * 1.15, color: MARK });
                    }
                    x += w;
                }
                Tok::Word(w) => {
                    for (s, ps) in w {
                        let pw = self.fonts.width(ps.face(), &s, ps.size);
                        if ps.mark {
                            back.push(Op::Rect { x, y: base - ps.size * 0.25, w: pw, h: ps.size * 1.15, color: MARK });
                        }
                        if let Some(l) = ps.link {
                            front.push(Op::Link { x, y: base - ps.size * 0.25, w: pw, h: ps.size * 1.15, link: l });
                        }
                        front.push(Op::Text { x, y: base + ps.rise, face: ps.face(), size: ps.size, color: ps.color, text: s });
                        x += pw;
                    }
                }
                Tok::Break => {}
            }
        }
        if self.quote > 0 {
            let bx = x0 - 10.0;
            back.push(Op::Line { x1: bx, y1: self.y, x2: bx, y2: self.y - lh, width: 1.5, gray: 0.75 });
        }
        let p = self.page();
        p.extend(back);
        p.extend(front);
        self.y -= lh;
    }

    fn image(&mut self, key: &str) -> Option<usize> {
        if let Some(i) = self.image_index.get(key) {
            return *i;
        }
        let idx = self.doc.images.get(key).and_then(decode).map(|im| {
            self.images.push(im);
            self.images.len() - 1
        });
        self.image_index.insert(key.to_string(), idx);
        idx
    }

    fn block(&mut self, b: &Block, x0: f32, width: f32) {
        let body = Style::body(BODY);
        let st = if self.quote > 0 { Style { color: (0.3, 0.3, 0.3), ..body } } else { body };
        match b {
            Block::Heading(l, v) => {
                let size = match l {
                    1 => 16.0,
                    2 => 13.5,
                    _ => 12.0,
                };
                self.gap(size * 0.8);
                // keep the heading with at least three lines of what follows
                self.ensure(size * 1.32 + BODY * 1.32 * 3.0);
                self.flow(v, Style { bold: true, size, ..st }, x0, width, Align::Left);
                self.y -= 3.0;
            }
            Block::Para(v) => {
                self.flow(v, st, x0, width, Align::Left);
                self.y -= 6.0;
            }
            Block::List { ordered, items } => {
                let ind = 18.0;
                for (i, it) in items.iter().enumerate() {
                    let mark = if *ordered { format!("{}.", i + 1) } else { "•".into() };
                    self.ensure(BODY * 1.32);
                    let page = self.pages.len() - 1;
                    let y = self.y - BODY * 0.92;
                    let mw = self.fonts.width(0, &mark, BODY);
                    for (k, b) in it.iter().enumerate() {
                        self.block(b, x0 + ind, width - ind);
                        if k + 1 == it.len() && matches!(b, Block::Para(_)) {
                            self.y += 4.0;
                        }
                    }
                    self.pages[page].push(Op::Text { x: x0 + ind - 5.0 - mw, y, face: 0, size: BODY, color: st.color, text: mark });
                }
                self.y -= 4.0;
            }
            Block::Code(c) => {
                let size = 9.0;
                let lh = size * 1.3;
                let cw = self.fonts.width(MONO, "m", size).max(0.1);
                let per = (((width - 12.0) / cw).floor() as usize).max(10);
                self.y -= 2.0;
                for l in c.lines() {
                    let chars: Vec<char> = l.chars().collect();
                    let chunks: Vec<String> =
                        if chars.is_empty() { vec![String::new()] } else { chars.chunks(per).map(|c| c.iter().collect()).collect() };
                    for s in chunks {
                        self.ensure(lh);
                        let y = self.y;
                        let p = self.page();
                        p.push(Op::Rect { x: x0, y: y - lh, w: width, h: lh, color: (0.95, 0.95, 0.95) });
                        p.push(Op::Text { x: x0 + 6.0, y: y - size * 0.95, face: MONO, size, color: BLACK, text: s });
                        self.y -= lh;
                    }
                }
                self.y -= 8.0;
            }
            Block::Math(m) => {
                self.flow(&[Inline::Text(math(m))], Style { italic: true, ..st }, x0, width, Align::Center);
                self.y -= 6.0;
            }
            Block::Quote(v) => {
                self.quote += 1;
                for b in v {
                    self.block(b, x0 + 18.0, width - 18.0);
                }
                self.quote -= 1;
            }
            Block::Table { head, rows } => self.table(head, rows, x0, width),
            Block::Image { key, caption } => {
                match self.image(key) {
                    Some(i) => {
                        let im = &self.images[i];
                        let (mut w, mut h) = (im.w as f32 * 0.75, im.h as f32 * 0.75);
                        let maxh = (TOP - BOTTOM) * 0.45;
                        let s = (width / w).min(maxh / h).min(1.0);
                        w *= s;
                        h *= s;
                        self.ensure(h + if caption.is_empty() { 0.0 } else { 14.0 });
                        let x = x0 + (width - w) / 2.0;
                        let y = self.y - h;
                        self.page().push(Op::Image { idx: i, x, y, w, h });
                        self.y -= h + 4.0;
                    }
                    None => {
                        let name = self.doc.images.get(key).map(|i| i.name.clone()).unwrap_or_else(|| key.to_string());
                        let what = if self.doc.images.contains_key(key) { "this image type is not embedded in PDF" } else { "missing" };
                        self.flow(&[Inline::Text(format!("[image {name}: {what}]"))], Style { italic: true, ..st }, x0, width, Align::Center);
                    }
                }
                if !caption.is_empty() {
                    self.flow(&[Inline::Text(caption.clone())], Style { italic: true, size: 9.5, ..st }, x0, width, Align::Center);
                }
                self.y -= 6.0;
            }
            Block::Rule => {
                self.ensure(10.0);
                let y = self.y - 4.0;
                self.page().push(Op::Line { x1: x0, y1: y, x2: x0 + width, y2: y, width: 0.5, gray: 0.6 });
                self.y -= 10.0;
            }
            Block::Endnotes(v) => {
                self.gap(4.0);
                self.ensure(12.0);
                let y = self.y - 2.0;
                self.page().push(Op::Line { x1: x0, y1: y, x2: x0 + 120.0, y2: y, width: 0.5, gray: 0.6 });
                self.y -= 6.0;
                for (n, c) in v {
                    let mut l = vec![Inline::Text(format!("{n}. "))];
                    l.extend(c.iter().cloned());
                    self.flow(&l, Style { size: 9.0, ..st }, x0, width, Align::Left);
                    self.y -= 2.0;
                }
                self.y -= 6.0;
            }
        }
    }

    fn wrap_cell(&mut self, s: &str, st: Style, width: f32) -> Vec<String> {
        let mut out = vec![];
        let mut cur = String::new();
        for w in s.split_whitespace() {
            let cand = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
            if !cur.is_empty() && self.fonts.width(st.face(), &cand, st.size) > width {
                out.push(std::mem::replace(&mut cur, w.to_string()));
            } else {
                cur = cand;
            }
        }
        out.push(cur);
        // hard-split anything still too wide
        let mut res = vec![];
        for l in out {
            if self.fonts.width(st.face(), &l, st.size) <= width {
                res.push(l);
                continue;
            }
            let mut piece = String::new();
            for c in l.chars() {
                piece.push(c);
                if self.fonts.width(st.face(), &piece, st.size) > width && piece.chars().count() > 1 {
                    let last = piece.pop().unwrap_or(' ');
                    res.push(std::mem::replace(&mut piece, last.to_string()));
                }
            }
            res.push(piece);
        }
        res
    }

    fn table(&mut self, head: &[String], rows: &[Vec<String>], x0: f32, width: f32) {
        let st = Style::body(9.0);
        let hs = Style { bold: true, ..st };
        let n = rows.iter().map(|r| r.len()).chain([head.len()]).max().unwrap_or(0);
        if n == 0 {
            return;
        }
        let pad = 4.0;
        let mut nat = vec![20.0f32; n];
        for (i, h) in head.iter().enumerate() {
            nat[i] = nat[i].max(self.fonts.width(hs.face(), h, hs.size) + 2.0 * pad);
        }
        for r in rows.iter().take(500) {
            for (i, c) in r.iter().enumerate() {
                nat[i] = nat[i].max(self.fonts.width(st.face(), c, st.size) + 2.0 * pad);
            }
        }
        let total: f32 = nat.iter().sum();
        let cols: Vec<f32> = if total <= width {
            nat
        } else {
            // shrink wide columns first: everyone keeps at least an equal share
            let fair = width / n as f32;
            let small: f32 = nat.iter().filter(|w| **w <= fair).sum();
            let big: f32 = nat.iter().filter(|w| **w > fair).sum();
            let room = (width - small).max(0.0);
            nat.iter().map(|w| if *w <= fair { *w } else { w * room / big.max(1.0) }).collect()
        };
        let lh = st.size * 1.3;
        self.y -= 2.0;
        let draw_row = |me: &mut Self, cells: &[String], style: Style, header: bool| {
            let wrapped: Vec<Vec<String>> =
                (0..n).map(|i| me.wrap_cell(cells.get(i).map(String::as_str).unwrap_or(""), style, cols[i] - 2.0 * pad)).collect();
            let lines = wrapped.iter().map(|w| w.len()).max().unwrap_or(1);
            let rh = lines as f32 * lh + 4.0;
            let y = me.y;
            let tw: f32 = cols.iter().sum();
            let p = me.page();
            if header {
                p.push(Op::Rect { x: x0, y: y - rh, w: tw, h: rh, color: (0.92, 0.92, 0.92) });
            }
            let mut x = x0;
            for (i, w) in wrapped.into_iter().enumerate() {
                for (k, l) in w.into_iter().enumerate() {
                    p.push(Op::Text {
                        x: x + pad,
                        y: y - 2.0 - style.size * 0.95 - k as f32 * lh,
                        face: style.face(),
                        size: style.size,
                        color: BLACK,
                        text: l,
                    });
                }
                x += cols[i];
            }
            p.push(Op::Line {
                x1: x0,
                y1: y - rh,
                x2: x0 + tw,
                y2: y - rh,
                width: if header { 0.8 } else { 0.3 },
                gray: if header { 0.3 } else { 0.7 },
            });
            me.y -= rh;
            rh
        };
        let head_h = if head.is_empty() { 0.0 } else { lh + 4.0 };
        self.ensure(head_h + lh + 4.0);
        if !head.is_empty() {
            draw_row(self, head, hs, true);
        }
        for r in rows {
            let est = r
                .iter()
                .enumerate()
                .map(|(i, c)| (self.fonts.width(0, c, st.size) / (cols[i.min(n - 1)] - 2.0 * pad).max(1.0)).ceil().max(1.0))
                .fold(1.0, f32::max)
                * lh
                + 4.0;
            if self.y - est < BOTTOM {
                self.new_page();
                if !head.is_empty() {
                    draw_row(self, head, hs, true);
                }
            }
            draw_row(self, r, st, false);
        }
        self.y -= 8.0;
    }
}

/// Lay out and write the document.
pub fn write(doc: &Doc) -> Result<Vec<u8>> {
    let mut l =
        Layout { fonts: Fonts::load()?, doc, pages: vec![vec![]], y: TOP, images: vec![], image_index: HashMap::new(), links: vec![], quote: 0 };
    let width = W - 2.0 * MARGIN;
    if !doc.title.is_empty() {
        let st = Style::body(BODY);
        l.flow(&[Inline::Text(doc.title.clone())], Style { bold: true, size: 20.0, ..st }, MARGIN, width, Align::Center);
        l.y -= 4.0;
        if !doc.authors.is_empty() {
            l.flow(&[Inline::Text(doc.authors.join(", "))], Style { size: 12.0, ..st }, MARGIN, width, Align::Center);
        }
        if !doc.date.is_empty() {
            l.flow(&[Inline::Text(doc.date.clone())], Style { italic: true, size: 10.5, ..st }, MARGIN, width, Align::Center);
        }
        l.y -= 18.0;
    }
    for b in &endnotes(&doc.blocks) {
        l.block(b, MARGIN, width);
    }
    emit(l)
}

fn emit(mut l: Layout) -> Result<Vec<u8>> {
    let mut pdf = Pdf::new();
    let mut alloc = Ref::new(1);
    let catalog = alloc.bump();
    let tree = alloc.bump();
    let info = alloc.bump();
    let mut remap: Vec<GlyphRemapper> = (0..FONTS.len()).map(|_| GlyphRemapper::new()).collect();
    let mut uni: Vec<BTreeMap<u16, char>> = vec![BTreeMap::new(); FONTS.len()];
    let fnames: Vec<String> = (0..FONTS.len()).map(|i| format!("F{i}")).collect();
    let inames: Vec<String> = (0..l.images.len()).map(|i| format!("Im{i}")).collect();
    let total = l.pages.len();
    let pages = std::mem::take(&mut l.pages);
    let mut contents = vec![];
    for (pi, ops) in pages.into_iter().enumerate() {
        let mut c = Content::new();
        let mut links = vec![];
        let mut text = |c: &mut Content, x: f32, y: f32, face: usize, size: f32, color: (f32, f32, f32), s: &str, l: &mut Layout| {
            let mut bytes = vec![];
            for ch in s.chars().filter(|c| !c.is_control()) {
                let (g, _) = l.fonts.glyph(face, ch);
                let n = remap[face].remap(g);
                uni[face].entry(n).or_insert(ch);
                bytes.extend_from_slice(&n.to_be_bytes());
            }
            c.begin_text();
            c.set_fill_rgb(color.0, color.1, color.2);
            c.set_font(Name(fnames[face].as_bytes()), size);
            c.set_text_matrix([1.0, 0.0, 0.0, 1.0, x, y]);
            c.show(Str(&bytes));
            c.end_text();
        };
        for op in ops {
            match op {
                Op::Rect { x, y, w, h, color } => {
                    c.set_fill_rgb(color.0, color.1, color.2);
                    c.rect(x, y, w, h);
                    c.fill_nonzero();
                }
                Op::Line { x1, y1, x2, y2, width, gray } => {
                    c.set_stroke_gray(gray);
                    c.set_line_width(width);
                    c.move_to(x1, y1);
                    c.line_to(x2, y2);
                    c.stroke();
                }
                Op::Text { x, y, face, size, color, text: s } => text(&mut c, x, y, face, size, color, &s, &mut l),
                Op::Image { idx, x, y, w, h } => {
                    c.save_state();
                    c.transform([w, 0.0, 0.0, h, x, y]);
                    c.x_object(Name(inames[idx].as_bytes()));
                    c.restore_state();
                }
                Op::Link { x, y, w, h, link } => links.push((Rect::new(x, y, x + w, y + h), link)),
            }
        }
        if total > 1 {
            let s = format!("{} / {total}", pi + 1);
            let w = l.fonts.width(0, &s, 9.0);
            text(&mut c, (W - w) / 2.0, 40.0, 0, 9.0, (0.45, 0.45, 0.45), &s, &mut l);
        }
        contents.push((c.finish(), links));
    }

    // fonts (only the ones used, subset to the glyphs used)
    let mut font_refs: Vec<Option<Ref>> = vec![None; FONTS.len()];
    for i in 0..FONTS.len() {
        if remap[i].num_gids() <= 1 {
            continue;
        }
        let face = &l.fonts.faces[i];
        let upem = face.units_per_em() as f32;
        let k = 1000.0 / upem;
        let sub = subsetter::subset(FONTS[i], 0, &remap[i]).map_err(|e| Error::invalid("font subset", format!("{e:?}")))?;
        let (t0, cid, desc, file, cmap) = (alloc.bump(), alloc.bump(), alloc.bump(), alloc.bump(), alloc.bump());
        let tag: String = (0..6).map(|j| (b'A' + ((i * 7 + j * 3 + remap[i].num_gids() as usize) % 26) as u8) as char).collect();
        let base = format!("{tag}+{}", NAMES[i]);
        pdf.type0_font(t0).base_font(Name(base.as_bytes())).encoding_predefined(Name(b"Identity-H")).descendant_font(cid).to_unicode(cmap);
        let sys = SystemInfo { registry: Str(b"Adobe"), ordering: Str(b"Identity"), supplement: 0 };
        let mut cf = pdf.cid_font(cid);
        cf.subtype(CidFontType::Type2);
        cf.base_font(Name(base.as_bytes()));
        cf.system_info(sys);
        cf.font_descriptor(desc);
        cf.cid_to_gid_map_predefined(Name(b"Identity"));
        let widths: Vec<f32> = remap[i].remapped_gids().map(|g| face.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap_or(0) as f32 * k).collect();
        cf.widths().consecutive(0, widths);
        cf.finish();
        let bb = face.global_bounding_box();
        let mut flags = FontFlags::NON_SYMBOLIC;
        if i == MONO {
            flags |= FontFlags::FIXED_PITCH;
        } else {
            flags |= FontFlags::SERIF;
        }
        if i == 2 || i == 3 {
            flags |= FontFlags::ITALIC;
        }
        pdf.font_descriptor(desc)
            .name(Name(base.as_bytes()))
            .flags(flags)
            .bbox(Rect::new(bb.x_min as f32 * k, bb.y_min as f32 * k, bb.x_max as f32 * k, bb.y_max as f32 * k))
            .italic_angle(face.italic_angle())
            .ascent(face.ascender() as f32 * k)
            .descent(face.descender() as f32 * k)
            .cap_height(face.capital_height().unwrap_or(face.ascender()) as f32 * k)
            .stem_v(if i == 1 || i == 3 { 140.0 } else { 80.0 })
            .font_file2(file);
        let comp = zlib(&sub);
        pdf.stream(file, &comp).filter(Filter::FlateDecode).pair(Name(b"Length1"), sub.len() as i32);
        let mut cm = UnicodeCmap::<u16>::new(Name(b"Custom"), sys);
        for (g, ch) in &uni[i] {
            cm.pair(*g, *ch);
        }
        pdf.cmap(cmap, &cm.finish());
        font_refs[i] = Some(t0);
    }

    // images
    let mut image_refs = vec![];
    for im in &l.images {
        let r = alloc.bump();
        let mask = im.alpha.as_ref().map(|a| {
            let m = alloc.bump();
            let mut x = pdf.image_xobject(m, a);
            x.filter(Filter::FlateDecode);
            x.width(im.w as i32).height(im.h as i32).bits_per_component(8);
            x.color_space().device_gray();
            m
        });
        let mut x = pdf.image_xobject(r, &im.data);
        x.filter(if im.dct { Filter::DctDecode } else { Filter::FlateDecode });
        x.width(im.w as i32).height(im.h as i32).bits_per_component(8);
        match im.comps {
            1 => x.color_space().device_gray(),
            4 => x.color_space().device_cmyk(),
            _ => x.color_space().device_rgb(),
        };
        if let Some(m) = mask {
            x.s_mask(m);
        }
        x.finish();
        image_refs.push(r);
    }

    let mut page_ids = vec![];
    for (content, links) in contents {
        let (pid, cid) = (alloc.bump(), alloc.bump());
        let comp = zlib(&content);
        pdf.stream(cid, &comp).filter(Filter::FlateDecode);
        let mut ann_ids = vec![];
        for (rect, link) in links {
            let a = alloc.bump();
            let mut an = pdf.annotation(a);
            an.subtype(AnnotationType::Link).rect(rect).border(0.0, 0.0, 0.0, None);
            an.action().action_type(ActionType::Uri).uri(Str(l.links[link].as_bytes()));
            an.finish();
            ann_ids.push(a);
        }
        let mut page = pdf.page(pid);
        page.media_box(Rect::new(0.0, 0.0, W, H)).parent(tree).contents(cid);
        if !ann_ids.is_empty() {
            page.annotations(ann_ids);
        }
        let mut res = page.resources();
        {
            let mut f = res.fonts();
            for (i, r) in font_refs.iter().enumerate() {
                if let Some(r) = r {
                    f.pair(Name(fnames[i].as_bytes()), *r);
                }
            }
        }
        if !image_refs.is_empty() {
            let mut x = res.x_objects();
            for (i, r) in image_refs.iter().enumerate() {
                x.pair(Name(inames[i].as_bytes()), *r);
            }
        }
        res.finish();
        page.finish();
        page_ids.push(pid);
    }
    pdf.pages(tree).kids(page_ids.iter().copied()).count(page_ids.len() as i32);
    pdf.catalog(catalog).pages(tree);
    let mut di = pdf.document_info(info);
    di.creator(TextStr("Muninn")).producer(TextStr(concat!("Muninn ", env!("CARGO_PKG_VERSION"))));
    if !l.doc.title.is_empty() {
        di.title(TextStr(&l.doc.title));
    }
    if !l.doc.authors.is_empty() {
        di.author(TextStr(&l.doc.authors.join(", ")));
    }
    di.finish();
    Ok(pdf.finish())
}

#[cfg(test)]
mod tests {
    use super::super::{Block, Doc, Inline, sample, t};
    use super::*;

    fn inflate_all(pdf: &[u8]) -> String {
        // decompress every FlateDecode stream so tests can look at the page content
        let mut out = String::new();
        let mut i = 0;
        while let Some(p) = pdf[i..].windows(6).position(|w| w == b"stream") {
            let s = i + p + 6;
            let s = s + usize::from(pdf.get(s) == Some(&b'\r')) + usize::from(pdf.get(s + usize::from(pdf.get(s) == Some(&b'\r'))) == Some(&b'\n'));
            let Some(e) = pdf[s..].windows(9).position(|w| w == b"endstream") else { break };
            let mut d = flate2::read::ZlibDecoder::new(&pdf[s..s + e]);
            let mut v = vec![];
            if std::io::Read::read_to_end(&mut d, &mut v).is_ok() {
                out.push_str(&String::from_utf8_lossy(&v));
            }
            i = s + e + 9;
        }
        out
    }

    #[test]
    fn pdf_structure() {
        let d = sample::doc();
        let b = write(&d).unwrap();
        assert!(b.starts_with(b"%PDF-"));
        let s = String::from_utf8_lossy(&b);
        assert!(s.contains("/Type /Catalog") || s.contains("/Type/Catalog"));
        assert!(s.contains("/FontFile2"), "fonts are embedded");
        assert!(s.contains("+LiberationSerif"), "as subsets");
        assert!(s.contains("/Subtype /Image") || s.contains("/Subtype/Image"), "the PNG is embedded");
        assert!(s.contains("/URI"), "links work");
        assert!(s.contains("(Weekly review & notes)") || s.contains("Weekly review"), "title in the document info");
        let content = inflate_all(&b);
        assert!(s.contains("/ToUnicode") && (s.contains("beginbfchar") || s.contains("beginbfrange")), "text can be copied (ToUnicode)");
        assert!(content.contains(" Tf"), "text is drawn");
        assert!(b.len() < 400_000, "subsetting keeps files small: {}", b.len());
    }

    #[test]
    fn long_documents_page_and_tables_repeat_headers() {
        let rows: Vec<Vec<String>> = (0..300).map(|i| vec![format!("2024-01-{i:03}"), format!("{}.5", 40000 + i), "x".repeat(i % 40)]).collect();
        let d = Doc {
            title: "Bars".into(),
            blocks: vec![
                Block::Table { head: vec!["time".into(), "close".into(), "note".into()], rows },
                Block::Para(vec![t(
                    "averyveryveryverylongwordthatcannotfitonasinglelineofthepageatallbecauseitisjustfartoolongforanylineofanywidthreallyreallyreally",
                )]),
                Block::Code("x".repeat(500)),
                Block::List { ordered: false, items: vec![vec![Block::Para(vec![Inline::Mark(vec![t("marked words here")])])]] },
                Block::Image { key: "gone".into(), caption: String::new() },
            ],
            ..Default::default()
        };
        let b = write(&d).unwrap();
        let s = String::from_utf8_lossy(&b);
        let count =
            s.split("/Count ").nth(1).and_then(|r| r.split(|c: char| !c.is_ascii_digit()).next()).and_then(|n| n.parse::<usize>().ok()).unwrap();
        assert!(count >= 5, "300 rows need several pages: {count}");
        let content = inflate_all(&b);
        // the table header is drawn in bold on every page
        assert!(content.matches("/F1 9 Tf").count() >= count, "{}", content.matches("/F1 9 Tf").count());
        assert!(content.contains("/F4 9 Tf"), "code in mono");
    }
}
