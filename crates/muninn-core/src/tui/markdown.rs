//! Markdown rendered for the terminal with the theme's colours: headings, bold and italic,
//! inline code and code blocks, lists, block quotes, links (with their URL), tables,
//! rules, math and images. Built on the same parser as exports ([`crate::render::markdown`]),
//! used by the journal entry reader and the help viewer.
use crate::render::{Block, Inline, markdown, plain, text::math};
use crate::theme::Theme;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

/// A span and the internal link it belongs to (help topics follow those).
pub type Tagged = (Span<'static>, Option<usize>);

/// Which links are internal (numbered and selectable) and which one is selected.
pub struct Links<'a> {
    pub internal: &'a dyn Fn(&str) -> bool,
    pub selected: Option<usize>,
}

/// Whether a file name looks like Markdown (`.md`, `.markdown`).
pub fn is_markdown_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".md") || n.ends_with(".markdown") || n.ends_with(".mdown") || n.ends_with(".mkd")
}

/// Markdown as styled lines, wrapped to `width` columns.
pub fn lines(src: &str, t: &Theme, width: u16) -> Vec<Line<'static>> {
    let none = |_: &str| false;
    tagged(src, t, width, &Links { internal: &none, selected: None })
        .into_iter()
        .map(|l| Line::from(l.into_iter().map(|(s, _)| s).collect::<Vec<_>>()))
        .collect()
}

/// Like [`lines`], but every span carries the index of the internal link it is part of.
pub fn tagged(src: &str, t: &Theme, width: u16, links: &Links) -> Vec<Vec<Tagged>> {
    let mut r = R { t, links, n: 0 };
    let mut out = vec![];
    r.blocks(&markdown::parse_keep_cells(src), (width as usize).max(10), &mut out);
    out
}

struct R<'a> {
    t: &'a Theme,
    links: &'a Links<'a>,
    n: usize,
}

fn sp(s: impl Into<String>, st: Style) -> Tagged {
    (Span::styled(s.into(), st), None)
}

impl R<'_> {
    fn muted(&self) -> Style {
        Style::new().fg(self.t.muted)
    }

    fn blocks(&mut self, bs: &[Block], w: usize, out: &mut Vec<Vec<Tagged>>) {
        for (i, b) in bs.iter().enumerate() {
            if i > 0 {
                out.push(vec![]);
            }
            self.block(b, w, out);
        }
    }

    fn block(&mut self, b: &Block, w: usize, out: &mut Vec<Vec<Tagged>>) {
        let t = self.t;
        match b {
            Block::Heading(l, v) => {
                let st = match l {
                    1 => Style::new().fg(t.header).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                    2 => Style::new().fg(t.header).add_modifier(Modifier::BOLD),
                    _ => Style::new().fg(t.header).add_modifier(Modifier::BOLD | Modifier::ITALIC),
                };
                let mut s = vec![];
                self.inl(v, st, &mut s);
                out.extend(wrap(s, w));
            }
            Block::Para(v) => {
                let mut s = vec![];
                self.inl(v, Style::new().fg(t.fg), &mut s);
                out.extend(wrap(s, w));
            }
            Block::List { ordered, items } => {
                for (n, item) in items.iter().enumerate() {
                    let marker = if *ordered { format!("{}. ", n + 1) } else { "• ".to_string() };
                    let ind = marker.chars().count();
                    let mut inner = vec![];
                    for (k, b) in item.iter().enumerate() {
                        if k > 0 && !matches!(b, Block::List { .. }) {
                            inner.push(vec![]);
                        }
                        self.block(b, w.saturating_sub(ind).max(4), &mut inner);
                    }
                    for (k, mut l) in inner.into_iter().enumerate() {
                        let p = if k == 0 { marker.clone() } else { " ".repeat(ind) };
                        l.insert(0, sp(p, Style::new().fg(t.accent)));
                        out.push(l);
                    }
                }
            }
            Block::Code(c) => {
                for l in c.lines() {
                    out.push(vec![sp("│ ", self.muted()), sp(l.replace('\t', "    "), Style::new().fg(t.info))]);
                }
            }
            Block::Math(m) => out.push(vec![sp(format!("  {}", math(m)), Style::new().fg(t.accent))]),
            Block::Quote(v) => {
                let mut inner = vec![];
                self.blocks(v, w.saturating_sub(2).max(4), &mut inner);
                for mut l in inner {
                    for s in l.iter_mut() {
                        s.0.style = s.0.style.add_modifier(Modifier::ITALIC);
                    }
                    l.insert(0, sp("▌ ", self.muted()));
                    out.push(l);
                }
            }
            Block::Table { head, rows } => self.table(head, rows, w, out),
            Block::Image { key, caption } => {
                out.push(vec![sp(format!("▣ image {}", if caption.is_empty() { key } else { caption }), Style::new().fg(t.accent))])
            }
            Block::Rule => out.push(vec![sp("─".repeat(w), self.muted())]),
            Block::Endnotes(v) => {
                for (n, il) in v {
                    let mut s = vec![sp(format!("[{n}] "), self.muted())];
                    self.inl(il, self.muted(), &mut s);
                    out.extend(wrap(s, w));
                }
            }
        }
    }

    fn inl(&mut self, v: &[Inline], st: Style, out: &mut Vec<Tagged>) {
        let t = self.t;
        for i in v {
            match i {
                Inline::Text(s) => out.push(sp(s.clone(), st)),
                Inline::Bold(c) => self.inl(c, st.add_modifier(Modifier::BOLD), out),
                Inline::Italic(c) => self.inl(c, st.add_modifier(Modifier::ITALIC), out),
                Inline::Mono(s) => out.push(sp(s.clone(), st.fg(t.accent))),
                Inline::Math(m) => out.push(sp(math(m), st.fg(t.accent))),
                Inline::Mark(c) => self.inl(c, st.bg(t.warning).fg(t.bg), out),
                Inline::Link(c, href) => {
                    let ls = st.fg(t.info).add_modifier(Modifier::UNDERLINED);
                    if (self.links.internal)(href) {
                        let n = self.n;
                        self.n += 1;
                        let ls = if self.links.selected == Some(n) {
                            Style::new().bg(t.selection_bg).fg(t.selection_fg).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
                        } else {
                            ls
                        };
                        let mut tmp = vec![];
                        self.inl(c, ls, &mut tmp);
                        out.extend(tmp.into_iter().map(|(s, _)| (s, Some(n))));
                    } else {
                        self.inl(c, ls, out);
                        if !href.is_empty() && plain(c) != *href {
                            out.push(sp(format!(" <{href}>"), self.muted()));
                        }
                    }
                }
                Inline::Note(c) => {
                    out.push(sp(" [note: ", self.muted()));
                    self.inl(c, self.muted(), out);
                    out.push(sp("]", self.muted()));
                }
                Inline::NoteRef(n) => out.push(sp(format!("[{n}]"), self.muted())),
                Inline::Break => out.push(sp("\n", st)),
            }
        }
    }

    fn table(&mut self, head: &[String], rows: &[Vec<String>], w: usize, out: &mut Vec<Vec<Tagged>>) {
        let t = self.t;
        let mut cell = |s: &str, st: Style| {
            let mut v = vec![];
            self.inl(&markdown::inlines(s), st, &mut v);
            v.retain(|(s, _)| s.content != "\n");
            v
        };
        let head: Vec<Vec<Tagged>> = head.iter().map(|c| cell(c, Style::new().fg(t.header).add_modifier(Modifier::BOLD))).collect();
        let rows: Vec<Vec<Vec<Tagged>>> = rows.iter().map(|r| r.iter().map(|c| cell(c, Style::new().fg(t.fg))).collect()).collect();
        let ncol = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
        if ncol == 0 {
            return;
        }
        let mut widths = vec![0usize; ncol];
        for r in std::iter::once(&head).chain(rows.iter()) {
            for (i, c) in r.iter().enumerate() {
                widths[i] = widths[i].max(span_width(c));
            }
        }
        // shrink the widest columns until the table fits
        let avail = w.saturating_sub(3 * (ncol - 1));
        while widths.iter().sum::<usize>() > avail {
            let (i, m) = widths.iter().enumerate().max_by_key(|(_, w)| **w).map(|(i, w)| (i, *w)).unwrap();
            if m <= 3 {
                break;
            }
            widths[i] = m - 1;
        }
        let sep = Style::new().fg(t.muted);
        // a row is as tall as its tallest cell; cells wrap within their column
        let line = |r: &[Vec<Tagged>], out: &mut Vec<Vec<Tagged>>| {
            let cols: Vec<Vec<Vec<Tagged>>> = widths.iter().enumerate().map(|(i, w)| wrap(r.get(i).cloned().unwrap_or_default(), *w)).collect();
            let h = cols.iter().map(Vec::len).max().unwrap_or(1);
            for k in 0..h {
                let mut l = vec![];
                for (i, w) in widths.iter().enumerate() {
                    if i > 0 {
                        l.push(sp(" │ ", sep));
                    }
                    l.extend(fit(cols[i].get(k).cloned().unwrap_or_default(), *w));
                }
                out.push(l);
            }
        };
        if !head.is_empty() {
            line(&head, out);
            out.push(vec![sp(widths.iter().map(|w| "─".repeat(*w)).collect::<Vec<_>>().join("─┼─"), sep)]);
        }
        for r in &rows {
            line(r, out);
        }
    }
}

fn span_width(v: &[Tagged]) -> usize {
    v.iter().map(|(s, _)| s.width()).sum()
}

/// Cut or pad spans to exactly `w` columns (an ellipsis marks a cut).
fn fit(mut v: Vec<Tagged>, w: usize) -> Vec<Tagged> {
    if let Some((s, _)) = v.last_mut() {
        let t = s.content.trim_end().to_string();
        s.content = t.into();
    }
    let total = span_width(&v);
    let mut out = vec![];
    if total <= w {
        out.extend(v);
        if total < w {
            out.push((Span::raw(" ".repeat(w - total)), None));
        }
        return out;
    }
    let mut left = w.saturating_sub(1);
    let mut last = Style::new();
    for (s, tag) in v {
        if left == 0 {
            break;
        }
        last = s.style;
        let txt: String = s.content.chars().take(left).collect();
        left -= txt.chars().count();
        out.push((Span::styled(txt, s.style), tag));
    }
    out.push((Span::styled("…", last), None));
    out
}

/// Word-wrap spans to `w` columns; a `"\n"` span forces a break.
fn wrap(spans: Vec<Tagged>, w: usize) -> Vec<Vec<Tagged>> {
    let mut out = vec![];
    let mut cur: Vec<Tagged> = vec![];
    let mut x = 0usize;
    for (span, tag) in spans {
        if span.content == "\n" {
            out.push(std::mem::take(&mut cur));
            x = 0;
            continue;
        }
        let style = span.style;
        let text = span.content.replace('\n', " ");
        let mut word = String::new();
        let flush = |word: &mut String, cur: &mut Vec<Tagged>, x: &mut usize, out: &mut Vec<Vec<Tagged>>| {
            if word.is_empty() {
                return;
            }
            let ww = Span::raw(word.as_str()).width();
            let trimmed = Span::raw(word.trim_end()).width();
            if *x > 0 && *x + trimmed > w {
                out.push(std::mem::take(cur));
                *x = 0;
                if word.trim().is_empty() {
                    word.clear();
                    return;
                }
            }
            *x += ww;
            cur.push((Span::styled(std::mem::take(word), style), tag));
        };
        for c in text.chars() {
            word.push(c);
            if c == ' ' {
                flush(&mut word, &mut cur, &mut x, &mut out);
            }
        }
        flush(&mut word, &mut cur, &mut x, &mut out);
    }
    out.push(cur);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> Theme {
        crate::theme::builtin("tokyo-night").unwrap().resolved(crate::theme::Variant::Dark)
    }

    fn text(l: &[Line]) -> Vec<String> {
        l.iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>().trim_end().to_string()).collect()
    }

    #[test]
    fn renders_the_common_markdown_elements() {
        let src = "# Plan\n\nBuy **BTC** and *wait*, see `rsi` and [docs](https://x.io).\n\n- one\n- two\n  - nested\n\n1. first\n\n> careful\n\n```\nlet a = 1;\n```\n\n| a | b |\n|---|---|\n| 1 | 22 |\n\n---\n";
        let t = theme();
        let l = lines(src, &t, 60);
        let s = text(&l);
        assert_eq!(s[0], "Plan");
        assert!(l[0].spans[0].style.add_modifier.contains(Modifier::BOLD | Modifier::UNDERLINED));
        assert!(s.iter().any(|x| x == "Buy BTC and wait, see rsi and docs <https://x.io>."), "{s:?}");
        let para = &l[2];
        let bold = para.spans.iter().find(|x| x.content.trim() == "BTC").unwrap();
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));
        assert!(para.spans.iter().any(|x| x.content.starts_with("wait") && x.style.add_modifier.contains(Modifier::ITALIC)));
        assert!(para.spans.iter().any(|x| x.content.trim() == "rsi" && x.style.fg == Some(t.accent)));
        assert!(s.contains(&"• one".to_string()) && s.contains(&"  • nested".to_string()), "{s:?}");
        assert!(s.contains(&"1. first".to_string()));
        assert!(s.contains(&"▌ careful".to_string()));
        assert!(s.contains(&"│ let a = 1;".to_string()));
        assert!(s.contains(&"a │ b".to_string()) && s.contains(&"1 │ 22".to_string()), "{s:?}");
        assert!(s.iter().any(|x| x.starts_with("──") && x.contains('┼')));
        assert_eq!(s.last().unwrap(), &"─".repeat(60));
        assert!(!s.iter().any(|x| x.contains("**") || x.contains("```") || x.starts_with('#')));
    }

    #[test]
    fn wraps_to_width_and_fits_wide_tables() {
        let t = theme();
        let l = lines(&"word ".repeat(40), &t, 20);
        assert!(l.len() > 5 && l.iter().all(|l| l.width() <= 20));
        let l = lines("| a | b |\n|---|---|\n| aa aa aa aa aa aa | bb bb bb bb bb bb |\n| cccccccccccccccccccccc | d |\n", &t, 20);
        assert!(l.iter().all(|l| l.width() <= 20), "{:?}", text(&l));
        let s = text(&l);
        assert!(s.len() == 5 && s[2] == "aa aa aa  │ bb bb bb" && s.iter().any(|x| x.contains('…')), "cells wrap, long words are cut: {s:?}");
    }

    #[test]
    fn internal_links_are_numbered_and_selected() {
        let t = theme();
        let internal = |h: &str| h.ends_with(".md");
        let v = tagged("[a](a.md) and [web](https://w) then [b](b.md)", &t, 80, &Links { internal: &internal, selected: Some(1) });
        let tags: Vec<usize> = v.iter().flatten().filter_map(|(_, n)| *n).collect();
        assert_eq!(tags, vec![0, 1]);
        let b = v.iter().flatten().find(|(_, n)| *n == Some(1)).unwrap();
        assert_eq!(b.0.style.bg, Some(t.selection_bg));
    }

    #[test]
    fn markdown_names() {
        assert!(is_markdown_name("README.md") && is_markdown_name("notes.MARKDOWN"));
        assert!(!is_markdown_name("bars.csv") && !is_markdown_name("md"));
    }
}
