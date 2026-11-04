//! LaTeX: the canonical source of every rendered document.
//!
//! [`write`] produces a complete, standard LaTeX document (article class, common packages)
//! that compiles with any TeX distribution. [`read`] parses LaTeX back into a [`Doc`]: what
//! `write` produces (so `read(write(d)) == d`), plus what journal entries commonly use:
//! sections, `\textbf`, `\emph`, `\texttt`, `\hl`, `\href`, `\url`, `\footnote`, lists,
//! `verbatim`, `quote`, `tabular`/`longtable`, `figure` with `\includegraphics`, inline and
//! display math (kept as LaTeX source), and the usual escapes. Unknown commands keep their
//! text; unknown environments keep their content. PDF and Docx are rendered from the result.
use super::{Block, Doc, Image, Inline, plain};
use std::collections::BTreeMap;

const PREAMBLE: &str = "\\documentclass[11pt,a4paper]{article}
\\usepackage[utf8]{inputenc}
\\usepackage[T1]{fontenc}
\\usepackage{lmodern}
\\usepackage[margin=2.5cm]{geometry}
\\usepackage{graphicx}
\\usepackage{longtable}
\\usepackage{amsmath}
\\usepackage{xcolor}
\\usepackage{soul}
\\usepackage{hyperref}
";

/// Escape text for LaTeX.
pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 8);
    let cs: Vec<char> = s.chars().collect();
    for (i, &c) in cs.iter().enumerate() {
        let next = cs.get(i + 1).copied();
        match c {
            '\\' => o.push_str("\\textbackslash{}"),
            '{' | '}' | '$' | '&' | '#' | '_' | '%' => {
                o.push('\\');
                o.push(c);
            }
            '^' => o.push_str("\\^{}"),
            '~' => o.push_str("\\textasciitilde{}"),
            '-' | '`' | '\'' if next == Some(c) => {
                o.push(c);
                o.push_str("{}");
            }
            '\n' | '\r' | '\t' => o.push(' '),
            _ => o.push(c),
        }
    }
    o
}

fn esc_url(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        if matches!(c, '\\' | '#' | '%' | '{' | '}' | '~' | '&' | '_' | '$' | '^') {
            o.push('\\');
        }
        o.push(c);
    }
    o
}

/// The document as LaTeX. Images are referenced as `<image_dir>/<key>.<ext>`.
pub fn write(doc: &Doc, image_dir: &str) -> String {
    let mut s = String::from(PREAMBLE);
    if !doc.title.is_empty() {
        s.push_str(&format!("\\title{{{}}}\n", esc(&doc.title)));
        s.push_str(&format!("\\author{{{}}}\n", doc.authors.iter().map(|a| esc(a)).collect::<Vec<_>>().join(" \\and ")));
        s.push_str(&format!("\\date{{{}}}\n", esc(&doc.date)));
    }
    s.push_str("\n\\begin{document}\n");
    if !doc.title.is_empty() {
        s.push_str("\\maketitle\n");
    }
    s.push('\n');
    blocks(&mut s, &doc.blocks, &doc.images, image_dir);
    s.push_str("\\end{document}\n");
    s
}

/// Blocks only (a fragment, e.g. for one entry).
pub fn fragment(v: &[Block]) -> String {
    let mut s = String::new();
    blocks(&mut s, v, &BTreeMap::new(), "");
    s
}

fn blocks(s: &mut String, v: &[Block], images: &BTreeMap<String, Image>, dir: &str) {
    for b in v {
        block(s, b, images, dir);
    }
}

fn block(s: &mut String, b: &Block, images: &BTreeMap<String, Image>, dir: &str) {
    match b {
        Block::Heading(l, v) => {
            let cmd = match l {
                1 => "section",
                2 => "subsection",
                _ => "subsubsection",
            };
            s.push_str(&format!("\\{cmd}*{{{}}}\n\n", inlines(v)));
        }
        Block::Para(v) => {
            s.push_str(&inlines(v));
            s.push_str("\n\n");
        }
        Block::List { ordered, items } => {
            let env = if *ordered { "enumerate" } else { "itemize" };
            s.push_str(&format!("\\begin{{{env}}}\n"));
            for it in items {
                s.push_str("\\item ");
                blocks(s, it, images, dir);
            }
            s.push_str(&format!("\\end{{{env}}}\n\n"));
        }
        Block::Code(c) => s.push_str(&format!("\\begin{{verbatim}}\n{}\n\\end{{verbatim}}\n\n", c.replace("\\end{verbatim}", "\\end {verbatim}"))),
        Block::Math(m) => s.push_str(&format!("\\[\n{}\n\\]\n\n", m.trim())),
        Block::Quote(v) => {
            s.push_str("\\begin{quote}\n");
            blocks(s, v, images, dir);
            s.push_str("\\end{quote}\n\n");
        }
        Block::Table { head, rows } => {
            let n = rows.iter().map(|r| r.len()).chain([head.len()]).max().unwrap_or(1).max(1);
            s.push_str(&format!("\\begin{{longtable}}{{{}}}\n\\hline\n", "l".repeat(n)));
            let row = |r: &[String]| r.iter().map(|c| esc(c)).collect::<Vec<_>>().join(" & ") + " \\\\\n";
            if !head.is_empty() {
                s.push_str(&row(head));
                s.push_str("\\hline\n\\endhead\n");
            }
            for r in rows {
                s.push_str(&row(r));
            }
            s.push_str("\\hline\n\\end{longtable}\n\n");
        }
        Block::Image { key, caption } => {
            let file = match images.get(key) {
                Some(im) => format!("{key}.{}", im.ext()),
                None => key.clone(),
            };
            let path = if dir.is_empty() { file } else { format!("{dir}/{file}") };
            s.push_str("\\begin{figure}[h]\n\\centering\n");
            s.push_str(&format!("\\includegraphics[width=\\linewidth,height=0.45\\textheight,keepaspectratio]{{{path}}}\n"));
            if !caption.is_empty() {
                s.push_str(&format!("\\caption{{{}}}\n", esc(caption)));
            }
            s.push_str("\\end{figure}\n\n");
        }
        Block::Rule => s.push_str("\\noindent\\rule{\\linewidth}{0.4pt}\n\n"),
        Block::Endnotes(v) => {
            s.push_str("\\begin{itemize}\n");
            for (n, c) in v {
                s.push_str(&format!("\\item[\\textsuperscript{{{n}}}] {}\n", inlines(c)));
            }
            s.push_str("\\end{itemize}\n\n");
        }
    }
}

fn inlines(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(x) => s.push_str(&esc(x)),
            Inline::Bold(c) => s.push_str(&format!("\\textbf{{{}}}", inlines(c))),
            Inline::Italic(c) => s.push_str(&format!("\\emph{{{}}}", inlines(c))),
            Inline::Mono(x) => s.push_str(&format!("\\texttt{{{}}}", esc(x))),
            Inline::Math(x) => s.push_str(&format!("${}$", x.trim())),
            Inline::Mark(c) => s.push_str(&format!("\\hl{{{}}}", inlines(c))),
            Inline::Link(c, u) => s.push_str(&format!("\\href{{{}}}{{{}}}", esc_url(u), inlines(c))),
            Inline::Note(c) => s.push_str(&format!("\\footnote{{{}}}", inlines(c))),
            Inline::NoteRef(n) => s.push_str(&format!("\\textsuperscript{{{n}}}")),
            Inline::Break => s.push_str("\\\\{}\n"),
        }
    }
    s
}

// ---------------------------------------------------------------- reader

/// Parse LaTeX (a whole document or a fragment) into a document. `images` resolves
/// `\includegraphics` paths: `attachment:KEY`, `KEY`, `dir/KEY.ext` all refer to `KEY`.
pub fn read(src: &str, images: &BTreeMap<String, Image>) -> Doc {
    let mut p = Parser { c: src.chars().collect(), i: 0, keys: images.keys().cloned().collect(), caption: None, meta: Meta::default() };
    // A whole document: metadata before \begin{document}, content inside.
    if let Some(at) = find(&p.c, "\\begin{document}") {
        let mut pre = Parser { c: p.c[..at].to_vec(), i: 0, keys: vec![], caption: None, meta: Meta::default() };
        pre.preamble();
        p.meta = pre.meta;
        p.i = at + "\\begin{document}".len();
    }
    let (blocks, _) = p.blocks(&End::Doc);
    let m = p.meta;
    Doc { title: m.title, authors: m.authors, date: m.date, blocks, images: images.clone() }
}

/// Parse a LaTeX fragment (an entry body) into blocks.
pub fn read_blocks(src: &str, images: &BTreeMap<String, Image>) -> Vec<Block> {
    read(src, images).blocks
}

fn find(hay: &[char], needle: &str) -> Option<usize> {
    let n: Vec<char> = needle.chars().collect();
    hay.windows(n.len()).position(|w| w == n.as_slice())
}

#[derive(Default)]
struct Meta {
    title: String,
    authors: Vec<String>,
    date: String,
}

enum End {
    Doc,
    Env(String),
    /// Inside a list: stop before `\item` or at `\end{env}`.
    Item(String),
}

#[derive(PartialEq)]
enum Ended {
    Eof,
    Env,
    Item,
}

struct Parser {
    c: Vec<char>,
    i: usize,
    keys: Vec<String>,
    caption: Option<String>,
    meta: Meta,
}

const IGNORE: &[&str] = &[
    "maketitle",
    "tableofcontents",
    "noindent",
    "centering",
    "raggedright",
    "raggedleft",
    "newpage",
    "clearpage",
    "pagebreak",
    "small",
    "footnotesize",
    "scriptsize",
    "tiny",
    "large",
    "Large",
    "LARGE",
    "huge",
    "Huge",
    "normalsize",
    "bfseries",
    "itshape",
    "ttfamily",
    "rmfamily",
    "sffamily",
    "medskip",
    "bigskip",
    "smallskip",
    "hline",
    "toprule",
    "midrule",
    "bottomrule",
    "endhead",
    "endfirsthead",
    "indent",
    "label",
    "vspace",
    "hspace",
    "setlength",
    "documentclass",
    "usepackage",
    "newcommand",
    "renewcommand",
    "pagestyle",
    "thispagestyle",
    "geometry",
    "ref",
    "cite",
];

impl Parser {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }
    fn at(&self, s: &str) -> bool {
        (self.i..).zip(s.chars()).all(|(j, ch)| self.c.get(j) == Some(&ch))
    }
    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }
    /// Skip spaces and at most one newline (what LaTeX eats after a control word).
    fn skip_space_after_cmd(&mut self) {
        let mut nl = false;
        while let Some(c) = self.peek() {
            if c == '\n' {
                if nl {
                    break;
                }
                // a blank line still ends the paragraph
                let mut j = self.i + 1;
                while self.c.get(j).is_some_and(|c| *c == ' ' || *c == '\t') {
                    j += 1;
                }
                if self.c.get(j) == Some(&'\n') {
                    break;
                }
                nl = true;
                self.i += 1;
            } else if c == ' ' || c == '\t' {
                self.i += 1;
            } else {
                break;
            }
        }
    }
    fn skip_comment(&mut self) {
        while let Some(c) = self.peek() {
            self.i += 1;
            if c == '\n' {
                break;
            }
        }
        // the newline after a comment does not count as a space or blank line
        while self.peek().is_some_and(|c| c == ' ' || c == '\t') {
            self.i += 1;
        }
    }
    /// A control sequence name after `\`.
    fn cmd_name(&mut self) -> String {
        let s = self.i;
        if self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            while self.peek().is_some_and(|c| c.is_ascii_alphabetic() || c == '@') {
                self.i += 1;
            }
            let n: String = self.c[s..self.i].iter().collect();
            if self.peek() == Some('*') {
                self.i += 1;
            }
            self.skip_space_after_cmd();
            n
        } else {
            self.i += usize::from(self.peek().is_some());
            self.c[s..self.i].iter().collect()
        }
    }
    /// Raw text of `{...}` (after optional whitespace), or "" when there is no group.
    fn group_raw(&mut self) -> String {
        let save = self.i;
        self.skip_ws();
        if self.peek() != Some('{') {
            self.i = save;
            return String::new();
        }
        self.i += 1;
        let s = self.i;
        let mut depth = 1;
        while let Some(c) = self.peek() {
            match c {
                '\\' => self.i += 1,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let r: String = self.c[s..self.i].iter().collect();
                        self.i += 1;
                        return r;
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
        self.c[s..].iter().collect()
    }
    /// Skip `[...]` if present.
    fn opt_arg(&mut self) -> Option<String> {
        let save = self.i;
        self.skip_ws();
        if self.peek() != Some('[') {
            self.i = save;
            return None;
        }
        let s = self.i + 1;
        let mut depth = 0;
        while let Some(c) = self.peek() {
            match c {
                '[' | '{' => depth += 1,
                '}' => depth -= 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        let r: String = self.c[s..self.i].iter().collect();
                        self.i += 1;
                        return Some(r);
                    }
                }
                _ => {}
            }
            self.i += 1;
        }
        None
    }
    fn sub(&self, src: &str) -> Parser {
        Parser { c: src.chars().collect(), i: 0, keys: self.keys.clone(), caption: None, meta: Meta::default() }
    }
    fn group_inlines(&mut self) -> Vec<Inline> {
        let raw = self.group_raw();
        let mut p = self.sub(&raw);
        p.inlines_all()
    }
    fn group_text(&mut self) -> String {
        plain(&self.group_inlines()).trim().to_string()
    }
    fn inlines_all(&mut self) -> Vec<Inline> {
        let mut out = vec![];
        while self.peek().is_some() {
            if self.at("\\begin") || self.at("\\par") && !self.at("\\paragraph") {
                // a block inside an inline group: keep its text
                let (bs, _) = self.blocks(&End::Doc);
                for b in bs {
                    if let Block::Para(v) = b {
                        push_space(&mut out);
                        out.extend(v);
                    }
                }
                continue;
            }
            self.inline_step(&mut out);
        }
        trim_end(&mut out);
        out
    }

    fn preamble(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                '%' => self.skip_comment(),
                '\\' => {
                    self.i += 1;
                    match self.cmd_name().as_str() {
                        "title" => self.meta.title = self.group_text(),
                        "author" => {
                            let raw = self.group_raw();
                            self.meta.authors =
                                raw.split("\\and").map(|a| plain(&self.sub(a).inlines_all()).trim().to_string()).filter(|a| !a.is_empty()).collect();
                        }
                        "date" => self.meta.date = self.group_text(),
                        _ => {}
                    }
                }
                _ => self.i += 1,
            }
        }
    }

    /// Blocks until the end of input or `end`.
    fn blocks(&mut self, end: &End) -> (Vec<Block>, Ended) {
        let mut out: Vec<Block> = vec![];
        let mut para: Vec<Inline> = vec![];
        let flush = |out: &mut Vec<Block>, para: &mut Vec<Inline>| {
            trim_end(para);
            trim_start(para);
            if !para.is_empty() {
                out.push(Block::Para(std::mem::take(para)));
            }
        };
        loop {
            let Some(c) = self.peek() else {
                flush(&mut out, &mut para);
                return (out, Ended::Eof);
            };
            match c {
                '\n' => {
                    let mut j = self.i + 1;
                    while self.c.get(j).is_some_and(|c| *c == ' ' || *c == '\t' || *c == '\r') {
                        j += 1;
                    }
                    if self.c.get(j) == Some(&'\n') {
                        flush(&mut out, &mut para);
                        self.i = j + 1;
                    } else {
                        push_space(&mut para);
                        self.i += 1;
                    }
                }
                '\\' if self.at("\\[") => {
                    flush(&mut out, &mut para);
                    self.i += 2;
                    let m = self.raw_until("\\]");
                    out.push(Block::Math(m.trim().to_string()));
                }
                '$' if self.at("$$") => {
                    flush(&mut out, &mut para);
                    self.i += 2;
                    let m = self.raw_until("$$");
                    out.push(Block::Math(m.trim().to_string()));
                }
                '\\' => {
                    let save = self.i;
                    self.i += 1;
                    let name = self.cmd_name();
                    match name.as_str() {
                        "begin" => {
                            let env = self.group_raw();
                            if let Some(b) = self.env(&env, &mut out, &mut para) {
                                flush(&mut out, &mut para);
                                out.extend(b);
                            }
                        }
                        "end" => {
                            let env = self.group_raw();
                            match end {
                                End::Env(e) | End::Item(e) if *e == env => {
                                    flush(&mut out, &mut para);
                                    return (out, Ended::Env);
                                }
                                _ => {}
                            }
                        }
                        "item" => {
                            if let End::Item(_) = end {
                                self.i = save;
                                flush(&mut out, &mut para);
                                return (out, Ended::Item);
                            }
                            self.opt_arg();
                        }
                        "section" | "subsection" | "subsubsection" | "paragraph" | "chapter" | "part" => {
                            flush(&mut out, &mut para);
                            self.opt_arg();
                            let lvl = match name.as_str() {
                                "part" | "chapter" | "section" => 1,
                                "subsection" => 2,
                                _ => 3,
                            };
                            let v = self.group_inlines();
                            out.push(Block::Heading(lvl, v));
                        }
                        "par" => flush(&mut out, &mut para),
                        "rule" => {
                            flush(&mut out, &mut para);
                            self.opt_arg();
                            self.group_raw();
                            self.group_raw();
                            out.push(Block::Rule);
                        }
                        "hrule" | "hrulefill" => {
                            flush(&mut out, &mut para);
                            out.push(Block::Rule);
                        }
                        "includegraphics" => {
                            flush(&mut out, &mut para);
                            self.opt_arg();
                            let path = self.group_raw();
                            out.push(Block::Image { key: self.image_key(&path), caption: String::new() });
                        }
                        "caption" => {
                            self.opt_arg();
                            self.caption = Some(self.group_text());
                        }
                        "title" => self.meta.title = self.group_text(),
                        "author" => self.meta.authors = vec![self.group_text()],
                        "date" => self.meta.date = self.group_text(),
                        _ => {
                            self.i = save;
                            self.inline_step(&mut para);
                        }
                    }
                }
                _ => self.inline_step(&mut para),
            }
        }
    }

    fn raw_until(&mut self, close: &str) -> String {
        let s = self.i;
        match find(&self.c[s..], close) {
            Some(k) => {
                self.i = s + k + close.chars().count();
                self.c[s..s + k].iter().collect()
            }
            None => {
                self.i = self.c.len();
                self.c[s..].iter().collect()
            }
        }
    }

    fn image_key(&self, path: &str) -> String {
        let p = path.trim();
        let p = p.strip_prefix("attachment:").unwrap_or(p);
        let file = p.rsplit('/').next().unwrap_or(p);
        if self.keys.iter().any(|k| k == file) {
            return file.to_string();
        }
        let stem = file.rsplit_once('.').map(|x| x.0).unwrap_or(file);
        if self.keys.iter().any(|k| k == stem) {
            return stem.to_string();
        }
        p.to_string()
    }

    /// An environment (after `\begin{env}`). Returns blocks, or None when its content was
    /// added to the current paragraph.
    fn env(&mut self, env: &str, out: &mut Vec<Block>, para: &mut Vec<Inline>) -> Option<Vec<Block>> {
        match env {
            "itemize" | "enumerate" | "description" => {
                let mut items = vec![];
                loop {
                    self.skip_ws();
                    if self.at("\\item") {
                        self.i += 1;
                        self.cmd_name();
                        let label = self.opt_arg();
                        let (mut bs, ended) = self.blocks(&End::Item(env.to_string()));
                        if let Some(l) = label.filter(|_| env == "description") {
                            let lab = self.sub(&l).inlines_all();
                            match bs.first_mut() {
                                Some(Block::Para(v)) => {
                                    let mut n = vec![Inline::Bold(lab)];
                                    push_text(&mut n, " ");
                                    for x in v.drain(..) {
                                        match x {
                                            Inline::Text(s) => push_text(&mut n, &s),
                                            o => n.push(o),
                                        }
                                    }
                                    *v = n;
                                }
                                _ => bs.insert(0, Block::Para(vec![Inline::Bold(lab)])),
                            }
                        }
                        items.push(bs);
                        if ended != Ended::Item {
                            break;
                        }
                    } else {
                        // text before the first \item, or the end
                        let (_, ended) = self.blocks(&End::Item(env.to_string()));
                        if ended != Ended::Item {
                            break;
                        }
                    }
                }
                Some(vec![Block::List { ordered: env == "enumerate", items }])
            }
            "verbatim" | "lstlisting" | "minted" | "Verbatim" => {
                if env == "minted" {
                    self.group_raw();
                } else {
                    self.opt_arg();
                }
                let mut s = self.raw_until(&format!("\\end{{{env}}}"));
                if s.starts_with('\n') {
                    s.remove(0);
                }
                if s.ends_with('\n') {
                    s.pop();
                }
                Some(vec![Block::Code(s.replace("\\end {verbatim}", "\\end{verbatim}"))])
            }
            "equation" | "equation*" | "align" | "align*" | "displaymath" | "gather" | "gather*" | "multline" | "multline*" | "eqnarray"
            | "eqnarray*" => {
                let m = self.raw_until(&format!("\\end{{{env}}}"));
                Some(vec![Block::Math(m.trim().to_string())])
            }
            "math" => {
                let m = self.raw_until("\\end{math}");
                para.push(Inline::Math(m.trim().to_string()));
                None
            }
            "quote" | "quotation" | "verse" | "abstract" => {
                let (bs, _) = self.blocks(&End::Env(env.to_string()));
                Some(vec![Block::Quote(bs)])
            }
            "tabular" | "tabular*" | "longtable" | "tabularx" | "array" => {
                if env == "tabular*" || env == "tabularx" {
                    self.group_raw();
                }
                self.opt_arg();
                self.group_raw();
                let raw = self.raw_until(&format!("\\end{{{env}}}"));
                Some(vec![self.table(&raw)])
            }
            "figure" | "figure*" | "table" | "table*" | "center" | "flushleft" | "flushright" | "minipage" => {
                self.opt_arg();
                if env == "minipage" {
                    self.group_raw();
                }
                let outer = self.caption.take();
                let (mut bs, _) = self.blocks(&End::Env(env.to_string()));
                if let Some(c) = self.caption.take() {
                    match bs.iter_mut().find(|b| matches!(b, Block::Image { .. })) {
                        Some(Block::Image { caption, .. }) => *caption = c,
                        _ => bs.push(Block::Para(vec![Inline::Italic(vec![Inline::Text(c)])])),
                    }
                }
                self.caption = outer;
                Some(bs)
            }
            "document" => None,
            _ => {
                let (bs, _) = self.blocks(&End::Env(env.to_string()));
                let _ = out;
                Some(bs)
            }
        }
    }

    fn table(&self, raw: &str) -> Block {
        // drop comments and rules
        let mut body = String::new();
        for line in raw.lines() {
            let l = strip_comment(line);
            body.push_str(l);
            body.push('\n');
        }
        let has_endhead = body.contains("\\endhead") || body.contains("\\endfirsthead");
        let rows_raw: Vec<String> = split_rows(&body);
        let mut rows: Vec<(Vec<String>, bool)> = vec![];
        for r in rows_raw {
            // a row is "rule-only" when nothing is left after removing rules
            let (cells_src, after_rule, head_mark) = clean_row(&r);
            if let Some(last) = rows.last_mut() {
                last.1 |= after_rule || head_mark && has_endhead;
                if head_mark {
                    last.1 = true;
                }
            }
            if cells_src.trim().is_empty() {
                continue;
            }
            let cells = split_cells(&cells_src).iter().map(|c| plain(&self.sub(c).inlines_all()).trim().to_string()).collect();
            rows.push((cells, false));
        }
        let mut head = vec![];
        if let Some(first) = rows.first() {
            let is_head = if has_endhead { first.1 } else { first.1 && rows.len() > 1 };
            if is_head {
                head = rows.remove(0).0;
            }
        }
        Block::Table { head, rows: rows.into_iter().map(|r| r.0).collect() }
    }

    /// One inline construct (or one character of text).
    fn inline_step(&mut self, out: &mut Vec<Inline>) {
        let Some(c) = self.peek() else { return };
        match c {
            '%' => self.skip_comment(),
            '$' => {
                self.i += 1;
                let m = self.raw_until("$");
                out.push(Inline::Math(m.trim().to_string()));
            }
            '{' => {
                let v = self.group_inlines();
                out.extend(v);
            }
            '}' => self.i += 1,
            '~' => {
                self.i += 1;
                push_text(out, " ");
            }
            '\\' if self.at("\\(") => {
                self.i += 2;
                let m = self.raw_until("\\)");
                out.push(Inline::Math(m.trim().to_string()));
            }
            '\\' => {
                self.i += 1;
                let name = self.cmd_name();
                self.inline_cmd(&name, out);
            }
            c if c.is_whitespace() => {
                self.i += 1;
                push_space(out);
            }
            '-' if self.at("---") => {
                self.i += 3;
                push_text(out, "—");
            }
            '-' if self.at("--") => {
                self.i += 2;
                push_text(out, "–");
            }
            '`' if self.at("``") => {
                self.i += 2;
                push_text(out, "“");
            }
            '\'' if self.at("''") => {
                self.i += 2;
                push_text(out, "”");
            }
            _ => {
                self.i += 1;
                push_text(out, &c.to_string());
            }
        }
    }

    fn inline_cmd(&mut self, name: &str, out: &mut Vec<Inline>) {
        match name {
            "textbf" | "bf" => out.push(Inline::Bold(self.group_inlines())),
            "emph" | "textit" | "textsl" | "it" | "em" => out.push(Inline::Italic(self.group_inlines())),
            "texttt" | "verb" => out.push(Inline::Mono(plain(&self.group_inlines()))),
            "hl" | "colorbox" | "highlight" => {
                if name == "colorbox" {
                    self.group_raw();
                }
                out.push(Inline::Mark(self.group_inlines()))
            }
            "href" => {
                let url = unescape(&self.group_raw());
                out.push(Inline::Link(self.group_inlines(), url));
            }
            "url" => {
                let url = unescape(&self.group_raw());
                out.push(Inline::Link(vec![Inline::Text(url.clone())], url));
            }
            "footnote" => {
                self.opt_arg();
                out.push(Inline::Note(self.group_inlines()));
            }
            "textsuperscript" => {
                let v = self.group_inlines();
                match plain(&v).trim().parse::<usize>() {
                    Ok(n) => out.push(Inline::NoteRef(n)),
                    Err(_) => out.extend(v),
                }
            }
            "\\" | "newline" | "linebreak" => {
                if self.peek() == Some('[') {
                    self.opt_arg();
                }
                if self.at("{}") {
                    self.i += 2;
                }
                while self.peek().is_some_and(char::is_whitespace) {
                    self.i += 1;
                }
                trim_end(out);
                out.push(Inline::Break);
            }
            "textbackslash" => {
                self.eat_empty();
                push_text(out, "\\");
            }
            "textasciitilde" => {
                self.eat_empty();
                push_text(out, "~");
            }
            "textasciicircum" | "^" => {
                self.eat_empty();
                push_text(out, "^");
            }
            "{" | "}" | "$" | "&" | "#" | "_" | "%" => push_text(out, name),
            "~" => {
                self.eat_empty();
                push_text(out, "~");
            }
            " " | "," | ";" | ":" | "quad" | "qquad" | "enspace" => push_text(out, " "),
            "-" | "/" | "@" => {}
            "ldots" | "dots" => {
                self.eat_empty();
                push_text(out, "…");
            }
            "LaTeX" | "TeX" => {
                self.eat_empty();
                push_text(out, name);
            }
            "textendash" => push_text(out, "–"),
            "textemdash" => push_text(out, "—"),
            "today" => push_text(out, &chrono::Utc::now().format("%Y-%m-%d").to_string()),
            n if IGNORE.contains(&n) => {
                if matches!(
                    n,
                    "label" | "vspace" | "hspace" | "ref" | "cite" | "geometry" | "pagestyle" | "thispagestyle" | "usepackage" | "documentclass"
                ) {
                    self.opt_arg();
                    self.group_raw();
                } else if matches!(n, "newcommand" | "renewcommand" | "setlength") {
                    self.group_raw();
                    self.opt_arg();
                    self.group_raw();
                }
            }
            "textrm" | "textsf" | "textnormal" | "mbox" | "text" | "textup" | "textmd" | "textsc" | "underline" | "uline" => {
                out.extend(self.group_inlines());
            }
            "textcolor" | "color" => {
                self.group_raw();
                if name == "textcolor" {
                    out.extend(self.group_inlines());
                }
            }
            _ => {
                // unknown command: keep the text of its arguments
                while self.peek() == Some('[') {
                    if self.opt_arg().is_none() {
                        break;
                    }
                }
                let save = self.i;
                self.skip_ws();
                if self.peek() == Some('{') {
                    out.extend(self.group_inlines());
                } else {
                    self.i = save;
                }
            }
        }
    }

    fn eat_empty(&mut self) {
        if self.at("{}") {
            self.i += 2;
        }
    }
}

fn unescape(s: &str) -> String {
    let mut o = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            if let Some(n) = it.next() {
                o.push(n);
            }
        } else {
            o.push(c);
        }
    }
    o.trim().to_string()
}

fn strip_comment(line: &str) -> &str {
    let mut esc = false;
    for (i, c) in line.char_indices() {
        if c == '%' && !esc {
            return &line[..i];
        }
        esc = c == '\\' && !esc;
    }
    line
}

/// Split a table body at `\\` (row ends).
fn split_rows(s: &str) -> Vec<String> {
    let c: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut cur = String::new();
    let mut i = 0;
    let mut depth = 0;
    while i < c.len() {
        if c[i] == '\\' && c.get(i + 1) == Some(&'\\') && depth == 0 {
            out.push(std::mem::take(&mut cur));
            i += 2;
            continue;
        }
        if c[i] == '\\' && i + 1 < c.len() {
            cur.push(c[i]);
            cur.push(c[i + 1]);
            i += 2;
            continue;
        }
        if c[i] == '{' {
            depth += 1;
        } else if c[i] == '}' {
            depth -= 1;
        }
        cur.push(c[i]);
        i += 1;
    }
    out.push(cur);
    out
}

/// Remove rule commands from a row: (cells source, a rule came before the cells, `\endhead`).
fn clean_row(r: &str) -> (String, bool, bool) {
    let mut s = r.to_string();
    let rules = ["\\hline", "\\toprule", "\\midrule", "\\bottomrule"];
    let lead = s.trim_start();
    let starts_rule = rules.iter().any(|x| lead.starts_with(x));
    let head = s.contains("\\endhead") || s.contains("\\endfirsthead");
    for x in rules.iter().chain(&["\\endhead", "\\endfirsthead", "\\endfoot", "\\endlastfoot"]) {
        s = s.replace(x, "");
    }
    (s, starts_rule, head)
}

fn split_cells(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut prev = ' ';
    let mut depth = 0;
    for c in s.chars() {
        if c == '&' && prev != '\\' && depth == 0 {
            out.push(std::mem::take(&mut cur));
        } else {
            if c == '{' && prev != '\\' {
                depth += 1;
            } else if c == '}' && prev != '\\' {
                depth -= 1;
            }
            cur.push(c);
        }
        prev = if prev == '\\' && c == '\\' { ' ' } else { c };
    }
    out.push(cur);
    out
}

fn push_text(out: &mut Vec<Inline>, s: &str) {
    if let Some(Inline::Text(t)) = out.last_mut() {
        t.push_str(s);
    } else {
        out.push(Inline::Text(s.to_string()));
    }
}

fn push_space(out: &mut Vec<Inline>) {
    match out.last() {
        None | Some(Inline::Break) => {}
        Some(Inline::Text(t)) if t.ends_with(' ') => {}
        _ => push_text(out, " "),
    }
}

fn trim_end(v: &mut Vec<Inline>) {
    while let Some(Inline::Text(t)) = v.last_mut() {
        let n = t.trim_end().len();
        t.truncate(n);
        if t.is_empty() {
            v.pop();
        } else {
            break;
        }
    }
}

fn trim_start(v: &mut Vec<Inline>) {
    while let Some(Inline::Text(t)) = v.first_mut() {
        let s = t.trim_start().to_string();
        if s.is_empty() {
            v.remove(0);
        } else {
            *t = s;
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{sample, t};
    use super::*;

    #[test]
    fn round_trip() {
        let d = sample::doc();
        let tex = write(&d, "imgs");
        assert!(tex.contains("\\begin{document}") && tex.contains("\\maketitle"));
        assert!(tex.contains("imgs/img1.png"));
        assert!(tex.contains("\\hl{broke out}"));
        assert!(tex.contains("\\footnote{Checked twice — Ada}"));
        let back = read(&tex, &d.images);
        assert_eq!(back.title, d.title);
        assert_eq!(back.authors, d.authors);
        assert_eq!(back.date, d.date);
        for (a, b) in back.blocks.iter().zip(&d.blocks) {
            assert_eq!(a, b);
        }
        assert_eq!(back, d);
    }

    #[test]
    fn reads_handwritten_latex() {
        let src = r#"
\section{Setup} % the plan
We use \textbf{bold}, \textit{italic} and ``quotes'' --- with $a_1$~and \(b\).
Line\\[2pt]
next.

\begin{description}
\item[Risk] small
\end{description}
\begin{tabular}{|l|r|}
\toprule
A & B \\ \midrule
1 & 2 \\
3 & \textbf{4} \\ \bottomrule
\end{tabular}
\begin{equation}
E = mc^2
\end{equation}
\includegraphics[width=3cm]{attachment:abc123}
\unknown{kept} \weird
"#;
        let mut im = BTreeMap::new();
        im.insert("abc123".into(), Image { name: "x.png".into(), mime: "image/png".into(), data: vec![] });
        let b = read_blocks(src, &im);
        assert_eq!(b[0], Block::Heading(1, vec![t("Setup")]));
        let Block::Para(p) = &b[1] else { panic!("{:?}", b[1]) };
        assert_eq!(plain(p), "We use bold, italic and “quotes” — with a_1 and b. Line next.");
        assert!(p.contains(&Inline::Break));
        assert_eq!(b[2], Block::List { ordered: false, items: vec![vec![Block::Para(vec![Inline::Bold(vec![t("Risk")]), t(" small")])]] });
        assert_eq!(b[3], Block::Table { head: vec!["A".into(), "B".into()], rows: vec![vec!["1".into(), "2".into()], vec!["3".into(), "4".into()]] });
        assert_eq!(b[4], Block::Math("E = mc^2".into()));
        assert_eq!(b[5], Block::Image { key: "abc123".into(), caption: String::new() });
        assert_eq!(b[6], Block::Para(vec![t("kept")]));
    }

    #[test]
    fn escapes() {
        let s = "a\\b {c} $5 & #1 _x 50% ^ ~ -- `` ''";
        let doc = Doc { blocks: vec![Block::Para(vec![t(s)])], ..Default::default() };
        assert_eq!(read(&write(&doc, ""), &BTreeMap::new()).blocks, doc.blocks);
    }
}
