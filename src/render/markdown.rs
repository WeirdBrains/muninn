//! Markdown entries into the document model (CommonMark-ish subset).
//!
//! Blocks: ATX headings, paragraphs, `-`/`*`/`+` and `1.` lists (nested by indent), fenced
//! code, `$$` display math, `>` quotes, `---` rules, pipe tables, images on their own line.
//! Inline: `**bold**`, `*italic*`/`_italic_`, `` `code` ``, `$math$`, `==highlight==`,
//! `[text](url)`, `<url>`, `\` escapes and hard breaks (two trailing spaces or `\`).
use super::{Block, Inline};

/// Parse Markdown into blocks.
pub fn parse(src: &str) -> Vec<Block> {
    let lines: Vec<&str> = src.lines().collect();
    blocks(&lines)
}

fn indent(l: &str) -> usize {
    l.chars().take_while(|c| *c == ' ').count() + l.chars().take_while(|c| *c == '\t').count() * 4
}

/// `- x` / `1. x`: (ordered, content start in bytes).
fn list_marker(l: &str) -> Option<(bool, usize)> {
    let t = l.trim_start();
    let lead = l.len() - t.len();
    for m in ["- ", "* ", "+ "] {
        if t.starts_with(m) && !is_rule(t) {
            return Some((false, lead + 2));
        }
    }
    let digits = t.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && digits < 10 {
        let rest = &t[digits..];
        if rest.starts_with(". ") || rest.starts_with(") ") {
            return Some((true, lead + digits + 2));
        }
    }
    None
}

fn is_rule(t: &str) -> bool {
    let t = t.trim();
    let c: Vec<char> = t.chars().filter(|c| !c.is_whitespace()).collect();
    c.len() >= 3 && (c.iter().all(|x| *x == '-') || c.iter().all(|x| *x == '*') || c.iter().all(|x| *x == '_'))
}

fn heading(t: &str) -> Option<(u8, &str)> {
    let n = t.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&n) && (t.len() == n || t[n..].starts_with(' ')) {
        Some(((n as u8).min(3), t[n..].trim().trim_end_matches('#').trim()))
    } else {
        None
    }
}

fn table_sep(l: &str) -> bool {
    let t = l.trim();
    t.contains('-') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

fn cells(l: &str) -> Vec<String> {
    let t = l.trim().trim_start_matches('|');
    let t = t.strip_suffix('|').unwrap_or(t);
    let mut out = vec![];
    let mut cur = String::new();
    let mut esc = false;
    for c in t.chars() {
        if c == '|' && !esc {
            out.push(std::mem::take(&mut cur));
        } else if c == '\\' && !esc {
            esc = true;
            continue;
        } else {
            cur.push(c);
        }
        esc = false;
    }
    out.push(cur);
    out.into_iter().map(|c| super::plain(&inlines(c.trim()))).collect()
}

/// `![alt](target)` alone on a line.
fn image_line(t: &str) -> Option<(String, String)> {
    let t = t.trim();
    let rest = t.strip_prefix("![")?;
    let close = rest.find("](")?;
    let alt = &rest[..close];
    let target = rest[close + 2..].strip_suffix(')')?;
    if target.contains(' ') && !target.contains('"') {
        return None;
    }
    let target = target.split_whitespace().next().unwrap_or("");
    Some((alt.to_string(), target.strip_prefix("attachment:").unwrap_or(target).to_string()))
}

fn blocks(lines: &[&str]) -> Vec<Block> {
    let mut out = vec![];
    let mut i = 0;
    let mut para: Vec<&str> = vec![];
    let flush = |out: &mut Vec<Block>, para: &mut Vec<&str>| {
        if !para.is_empty() {
            out.push(Block::Para(para_inlines(para)));
            para.clear();
        }
    };
    while i < lines.len() {
        let l = lines[i];
        let t = l.trim();
        if t.is_empty() {
            flush(&mut out, &mut para);
            i += 1;
            continue;
        }
        if t.starts_with("```") || t.starts_with("~~~") {
            flush(&mut out, &mut para);
            let fence = &t[..3];
            let ind = indent(l);
            let mut code = vec![];
            i += 1;
            while i < lines.len() && !lines[i].trim().starts_with(fence) {
                let s = lines[i];
                let lead = s.len() - s.trim_start_matches(' ').len();
                code.push(&s[lead.min(ind)..]);
                i += 1;
            }
            i += 1;
            out.push(Block::Code(code.join("\n")));
            continue;
        }
        if t.starts_with("$$") {
            flush(&mut out, &mut para);
            let first = t.trim_start_matches("$$");
            if let Some(inner) = first.strip_suffix("$$") {
                out.push(Block::Math(inner.trim().to_string()));
                i += 1;
                continue;
            }
            let mut m = vec![first];
            i += 1;
            while i < lines.len() && !lines[i].trim().ends_with("$$") {
                m.push(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                m.push(lines[i].trim().trim_end_matches("$$"));
            }
            i += 1;
            out.push(Block::Math(m.join("\n").trim().to_string()));
            continue;
        }
        if let Some((lvl, text)) = heading(t) {
            flush(&mut out, &mut para);
            out.push(Block::Heading(lvl, inlines(text)));
            i += 1;
            continue;
        }
        if is_rule(t) && para.is_empty() {
            out.push(Block::Rule);
            i += 1;
            continue;
        }
        if t.starts_with('>') {
            flush(&mut out, &mut para);
            let mut q = vec![];
            while i < lines.len() && lines[i].trim().starts_with('>') {
                let s = lines[i].trim().trim_start_matches('>');
                q.push(s.strip_prefix(' ').unwrap_or(s));
                i += 1;
            }
            out.push(Block::Quote(blocks(&q)));
            continue;
        }
        if t.starts_with('|') && i + 1 < lines.len() && table_sep(lines[i + 1]) {
            flush(&mut out, &mut para);
            let head = cells(t);
            i += 2;
            let mut rows = vec![];
            while i < lines.len() && lines[i].trim().starts_with('|') {
                rows.push(cells(lines[i]));
                i += 1;
            }
            out.push(Block::Table { head, rows });
            continue;
        }
        if let Some((alt, key)) = image_line(t) {
            flush(&mut out, &mut para);
            out.push(Block::Image { key, caption: alt });
            i += 1;
            continue;
        }
        if let Some((ordered, _)) = list_marker(l) {
            flush(&mut out, &mut para);
            let base = indent(l);
            let mut items: Vec<Vec<Block>> = vec![];
            while i < lines.len() {
                let l = lines[i];
                let Some((o, start)) = list_marker(l) else { break };
                if o != ordered || indent(l) != base {
                    break;
                }
                let mut item = vec![&l[start..]];
                let cont = start;
                i += 1;
                while i < lines.len() {
                    let s = lines[i];
                    if s.trim().is_empty() {
                        // a blank line continues the item only if indented text follows
                        if i + 1 < lines.len() && indent(lines[i + 1]) > base && !lines[i + 1].trim().is_empty() {
                            item.push("");
                            i += 1;
                            continue;
                        }
                        break;
                    }
                    if indent(s) > base {
                        let cut = cont.min(indent(s));
                        item.push(&s[cut.min(s.len())..]);
                        i += 1;
                    } else {
                        break;
                    }
                }
                items.push(blocks(&item));
            }
            out.push(Block::List { ordered, items });
            continue;
        }
        para.push(l);
        i += 1;
    }
    flush(&mut out, &mut para);
    out
}

fn para_inlines(lines: &[&str]) -> Vec<Inline> {
    let mut out: Vec<Inline> = vec![];
    for (n, l) in lines.iter().enumerate() {
        let last = n + 1 == lines.len();
        let hard = !last && (l.ends_with("  ") || l.trim_end().ends_with('\\') && !l.trim_end().ends_with("\\\\"));
        let text = if hard { l.trim_end().trim_end_matches('\\') } else { l.trim_end() };
        let text = text.trim_start();
        for x in inlines(text) {
            push(&mut out, x);
        }
        if hard {
            out.push(Inline::Break);
        } else if !last {
            push(&mut out, Inline::Text(" ".into()));
        }
    }
    out
}

fn push(out: &mut Vec<Inline>, x: Inline) {
    if let (Some(Inline::Text(a)), Inline::Text(b)) = (out.last_mut(), &x) {
        a.push_str(b);
        return;
    }
    out.push(x);
}

/// Inline Markdown.
pub fn inlines(s: &str) -> Vec<Inline> {
    let c: Vec<char> = s.chars().collect();
    let mut out = vec![];
    parse_inline(&c, &mut out);
    out
}

fn find_close(c: &[char], from: usize, delim: &[char], word: bool) -> Option<usize> {
    let n = delim.len();
    let mut j = from;
    while j + n <= c.len() {
        if c[j] == '\\' {
            j += 2;
            continue;
        }
        if c[j] == '`' && delim != ['`'] {
            // skip code spans
            if let Some(k) = c[j + 1..].iter().position(|x| *x == '`') {
                j += k + 2;
                continue;
            }
        }
        if c[j..j + n] == *delim && j > from && !c[j - 1].is_whitespace() {
            let after = c.get(j + n);
            if !word || after.is_none_or(|a| !a.is_alphanumeric()) {
                return Some(j);
            }
        }
        j += 1;
    }
    None
}

fn parse_inline(c: &[char], out: &mut Vec<Inline>) {
    let mut i = 0;
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut Vec<Inline>| {
        if !text.is_empty() {
            push(out, Inline::Text(std::mem::take(text)));
        }
    };
    while i < c.len() {
        let ch = c[i];
        let next = c.get(i + 1).copied();
        let prev = if i > 0 { Some(c[i - 1]) } else { None };
        match ch {
            '\\' if next.is_some_and(|n| n.is_ascii_punctuation()) => {
                text.push(next.unwrap_or('\\'));
                i += 2;
                continue;
            }
            '`' => {
                let n = c[i..].iter().take_while(|x| **x == '`').count();
                let delim: Vec<char> = vec!['`'; n];
                if let Some(k) = (i + n..=c.len().saturating_sub(n)).find(|&k| c[k..k + n] == *delim && c.get(k + n) != Some(&'`')) {
                    flush(&mut text, out);
                    let code: String = c[i + n..k].iter().collect();
                    out.push(Inline::Mono(code.trim().to_string()));
                    i = k + n;
                    continue;
                }
            }
            '$' if next.is_some_and(|n| !n.is_whitespace() && n != '$') => {
                if let Some(k) = (i + 1..c.len()).find(|&k| c[k] == '$' && c[k - 1] != '\\' && !c[k - 1].is_whitespace())
                    && !c.get(k + 1).is_some_and(|x| x.is_ascii_digit())
                {
                    flush(&mut text, out);
                    out.push(Inline::Math(c[i + 1..k].iter().collect()));
                    i = k + 1;
                    continue;
                }
            }
            '*' | '_' if next == Some(ch) => {
                let word = ch == '_';
                if c.get(i + 2).is_some_and(|x| !x.is_whitespace())
                    && (!word || prev.is_none_or(|p| !p.is_alphanumeric()))
                    && let Some(k) = find_close(c, i + 2, &[ch, ch], word)
                {
                    flush(&mut text, out);
                    let mut inner = vec![];
                    parse_inline(&c[i + 2..k], &mut inner);
                    out.push(Inline::Bold(inner));
                    i = k + 2;
                    continue;
                }
            }
            '*' | '_' => {
                let word = ch == '_';
                if next.is_some_and(|x| !x.is_whitespace())
                    && (!word || prev.is_none_or(|p| !p.is_alphanumeric()))
                    && let Some(k) = find_close(c, i + 1, &[ch], word)
                    && c.get(k + 1) != Some(&ch)
                {
                    flush(&mut text, out);
                    let mut inner = vec![];
                    parse_inline(&c[i + 1..k], &mut inner);
                    out.push(Inline::Italic(inner));
                    i = k + 1;
                    continue;
                }
            }
            '=' if next == Some('=') && c.get(i + 2).is_some_and(|x| !x.is_whitespace()) => {
                if let Some(k) = find_close(c, i + 2, &['=', '='], false) {
                    flush(&mut text, out);
                    let mut inner = vec![];
                    parse_inline(&c[i + 2..k], &mut inner);
                    out.push(Inline::Mark(inner));
                    i = k + 2;
                    continue;
                }
            }
            '!' if next == Some('[') => {
                // inline image: keep its text
                if let Some((alt, end)) = link_at(c, i + 1) {
                    text.push_str(&format!("[image: {}]", alt.0));
                    i = end;
                    continue;
                }
            }
            '[' => {
                if let Some(((label, url), end)) = link_at(c, i) {
                    flush(&mut text, out);
                    let mut inner = vec![];
                    parse_inline(&label.chars().collect::<Vec<_>>(), &mut inner);
                    out.push(Inline::Link(inner, url));
                    i = end;
                    continue;
                }
            }
            '<' => {
                if let Some(k) = c[i + 1..].iter().position(|x| *x == '>') {
                    let u: String = c[i + 1..i + 1 + k].iter().collect();
                    if (u.starts_with("http://") || u.starts_with("https://") || u.starts_with("mailto:")) && !u.contains(' ') {
                        flush(&mut text, out);
                        out.push(Inline::Link(vec![Inline::Text(u.clone())], u));
                        i += k + 2;
                        continue;
                    }
                }
            }
            _ => {}
        }
        text.push(ch);
        i += 1;
    }
    flush(&mut text, out);
}

/// `[label](url)` at `i`: ((label, url), index after).
fn link_at(c: &[char], i: usize) -> Option<((String, String), usize)> {
    if c.get(i) != Some(&'[') {
        return None;
    }
    let mut depth = 0;
    let mut j = i;
    while j < c.len() {
        match c[j] {
            '\\' => j += 1,
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        j += 1;
    }
    if c.get(j + 1) != Some(&'(') {
        return None;
    }
    let close = c[j + 2..].iter().position(|x| *x == ')')? + j + 2;
    let label: String = c[i + 1..j].iter().collect();
    let url: String = c[j + 2..close].iter().collect();
    let url = url.split_whitespace().next().unwrap_or("").to_string();
    Some(((label, url), close + 1))
}

#[cfg(test)]
mod tests {
    use super::super::t;
    use super::*;

    #[test]
    fn blocks_and_inlines() {
        let src = "# Title\n\nSome **bold**, *it* and _also_ `x_y` with $a^2$ and ==mark== [link](https://x.y) snake_case 5$ and $6.\nnext line  \nafter break\n\n- one\n- two\n  - deep\n1. first\n2. second\n\n```rust\nfn x() {}\n```\n\n$$\nE=mc^2\n$$\n\n> quoted **text**\n\n---\n\n| a | b |\n|---|--:|\n| 1 | **2** |\n\n![chart.png](attachment:abc)\n";
        let b = parse(src);
        assert_eq!(b[0], Block::Heading(1, vec![t("Title")]));
        let Block::Para(p) = &b[1] else { panic!() };
        assert_eq!(p[0], t("Some "));
        assert_eq!(p[1], Inline::Bold(vec![t("bold")]));
        assert_eq!(p[3], Inline::Italic(vec![t("it")]));
        assert_eq!(p[5], Inline::Italic(vec![t("also")]));
        assert_eq!(p[7], Inline::Mono("x_y".into()));
        assert_eq!(p[9], Inline::Math("a^2".into()));
        assert_eq!(p[11], Inline::Mark(vec![t("mark")]));
        assert_eq!(p[13], Inline::Link(vec![t("link")], "https://x.y".into()));
        assert_eq!(p[14], t(" snake_case 5$ and $6. next line"));
        assert_eq!(p[15], Inline::Break);
        assert_eq!(
            b[2],
            Block::List {
                ordered: false,
                items: vec![
                    vec![Block::Para(vec![t("one")])],
                    vec![Block::Para(vec![t("two")]), Block::List { ordered: false, items: vec![vec![Block::Para(vec![t("deep")])]] }]
                ]
            }
        );
        assert!(matches!(&b[3], Block::List { ordered: true, items } if items.len() == 2));
        assert_eq!(b[4], Block::Code("fn x() {}".into()));
        assert_eq!(b[5], Block::Math("E=mc^2".into()));
        assert_eq!(b[6], Block::Quote(vec![Block::Para(vec![t("quoted "), Inline::Bold(vec![t("text")])])]));
        assert_eq!(b[7], Block::Rule);
        assert_eq!(b[8], Block::Table { head: vec!["a".into(), "b".into()], rows: vec![vec!["1".into(), "2".into()]] });
        assert_eq!(b[9], Block::Image { key: "abc".into(), caption: "chart.png".into() });
        assert_eq!(b.len(), 10);
    }
}
