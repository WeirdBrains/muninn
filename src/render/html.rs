//! HTML: one self-contained file (styles inline, images embedded as data URIs).
use super::{Block, Doc, Inline, endnotes};
use base64::Engine;

const STYLE: &str = "body{font-family:Georgia,'Liberation Serif',serif;max-width:46em;margin:2em auto;padding:0 1em;line-height:1.5;color:#222}
h1.title{text-align:center;margin-bottom:.2em}.byline{text-align:center;color:#555;margin:0}.date{text-align:center;color:#777;font-style:italic}
pre{background:#f4f4f4;padding:.6em;overflow-x:auto}code,.math{font-family:'Liberation Mono',monospace}
.math{font-style:italic}.display-math{text-align:center;font-style:italic;font-family:'Liberation Mono',monospace}
blockquote{border-left:3px solid #ccc;margin-left:0;padding-left:1em;color:#444}
table{border-collapse:collapse;margin:1em 0}td,th{border:1px solid #ccc;padding:.25em .6em;text-align:left}th{background:#f0f0f0}
figure{text-align:center;margin:1em 0}figure img{max-width:100%}figcaption{font-style:italic;color:#555}
mark{background:#fff27a}.notes{font-size:.9em;color:#444;border-top:1px solid #ddd}.missing{color:#a00}
";

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            _ => o.push(c),
        }
    }
    o
}

pub fn write(doc: &Doc) -> String {
    let mut s = String::from("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    s.push_str("<meta name=\"generator\" content=\"Muninn\">\n");
    if !doc.authors.is_empty() {
        s.push_str(&format!("<meta name=\"author\" content=\"{}\">\n", esc(&doc.authors.join(", "))));
    }
    s.push_str(&format!("<title>{}</title>\n<style>\n{STYLE}</style>\n</head>\n<body>\n", esc(&doc.title)));
    if !doc.title.is_empty() {
        s.push_str(&format!("<h1 class=\"title\">{}</h1>\n", esc(&doc.title)));
        if !doc.authors.is_empty() {
            s.push_str(&format!("<p class=\"byline\">{}</p>\n", esc(&doc.authors.join(", "))));
        }
        if !doc.date.is_empty() {
            s.push_str(&format!("<p class=\"date\">{}</p>\n", esc(&doc.date)));
        }
    }
    for b in &endnotes(&doc.blocks) {
        block(&mut s, b, doc);
    }
    s.push_str("</body>\n</html>\n");
    s
}

fn block(s: &mut String, b: &Block, doc: &Doc) {
    match b {
        Block::Heading(l, v) => {
            let l = (*l + 1).min(4);
            s.push_str(&format!("<h{l}>{}</h{l}>\n", inlines(v)));
        }
        Block::Para(v) => s.push_str(&format!("<p>{}</p>\n", inlines(v))),
        Block::List { ordered, items } => {
            let tag = if *ordered { "ol" } else { "ul" };
            s.push_str(&format!("<{tag}>\n"));
            for it in items {
                s.push_str("<li>");
                // a single paragraph item renders without <p>
                if let [Block::Para(v)] = it.as_slice() {
                    s.push_str(&inlines(v));
                } else {
                    for b in it {
                        block(s, b, doc);
                    }
                }
                s.push_str("</li>\n");
            }
            s.push_str(&format!("</{tag}>\n"));
        }
        Block::Code(c) => s.push_str(&format!("<pre><code>{}</code></pre>\n", esc(c))),
        Block::Math(m) => s.push_str(&format!("<p class=\"display-math\">{}</p>\n", esc(&super::text::math(m)))),
        Block::Quote(v) => {
            s.push_str("<blockquote>\n");
            for b in v {
                block(s, b, doc);
            }
            s.push_str("</blockquote>\n");
        }
        Block::Table { head, rows } => {
            s.push_str("<table>\n");
            if !head.is_empty() {
                s.push_str("<thead><tr>");
                for h in head {
                    s.push_str(&format!("<th>{}</th>", esc(h)));
                }
                s.push_str("</tr></thead>\n");
            }
            s.push_str("<tbody>\n");
            for r in rows {
                s.push_str("<tr>");
                for c in r {
                    s.push_str(&format!("<td>{}</td>", esc(c)));
                }
                s.push_str("</tr>\n");
            }
            s.push_str("</tbody>\n</table>\n");
        }
        Block::Image { key, caption } => {
            s.push_str("<figure>");
            match doc.images.get(key) {
                Some(im) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&im.data);
                    s.push_str(&format!("<img src=\"data:{};base64,{b64}\" alt=\"{}\">", im.mime, esc(&im.name)));
                }
                None => s.push_str(&format!("<span class=\"missing\">[missing image {}]</span>", esc(key))),
            }
            if !caption.is_empty() {
                s.push_str(&format!("<figcaption>{}</figcaption>", esc(caption)));
            }
            s.push_str("</figure>\n");
        }
        Block::Rule => s.push_str("<hr>\n"),
        Block::Endnotes(v) => {
            s.push_str("<ol class=\"notes\">\n");
            for (n, c) in v {
                s.push_str(&format!("<li value=\"{n}\" id=\"note-{n}\">{}</li>\n", inlines(c)));
            }
            s.push_str("</ol>\n");
        }
    }
}

fn inlines(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(&esc(t)),
            Inline::Bold(c) => s.push_str(&format!("<strong>{}</strong>", inlines(c))),
            Inline::Italic(c) => s.push_str(&format!("<em>{}</em>", inlines(c))),
            Inline::Mono(t) => s.push_str(&format!("<code>{}</code>", esc(t))),
            Inline::Math(m) => s.push_str(&format!("<span class=\"math\">{}</span>", esc(&super::text::math(m)))),
            Inline::Mark(c) => s.push_str(&format!("<mark>{}</mark>", inlines(c))),
            Inline::Link(c, u) => s.push_str(&format!("<a href=\"{}\">{}</a>", esc(u), inlines(c))),
            Inline::Note(c) => s.push_str(&format!("<small>({})</small>", inlines(c))),
            Inline::NoteRef(n) => s.push_str(&format!("<sup><a href=\"#note-{n}\">{n}</a></sup>")),
            Inline::Break => s.push_str("<br>\n"),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_is_self_contained() {
        let h = write(&super::super::sample::doc());
        assert!(h.starts_with("<!DOCTYPE html>"));
        assert!(h.contains("<h1 class=\"title\">Weekly review &amp; notes</h1>"));
        assert!(h.contains("<mark>broke out</mark>"));
        assert!(h.contains("<img src=\"data:image/png;base64,"));
        assert!(h.contains("<sup><a href=\"#note-1\">1</a></sup>"));
        assert!(h.contains("<th>time</th>"));
        assert!(h.contains("x² + α"), "math is shown readably");
        assert!(!h.contains("<script"));
    }
}
