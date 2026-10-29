//! Plain text (previews in the TUI and `mnnn report preview`), and readable math.
use super::{Block, Doc, Inline, endnotes};

/// LaTeX math shown readably: common commands become symbols, `^2` superscripts.
pub fn math(m: &str) -> String {
    const SYM: &[(&str, &str)] = &[
        ("\\alpha", "α"),
        ("\\beta", "β"),
        ("\\gamma", "γ"),
        ("\\delta", "δ"),
        ("\\Delta", "Δ"),
        ("\\epsilon", "ε"),
        ("\\varepsilon", "ε"),
        ("\\theta", "θ"),
        ("\\lambda", "λ"),
        ("\\mu", "μ"),
        ("\\pi", "π"),
        ("\\rho", "ρ"),
        ("\\sigma", "σ"),
        ("\\Sigma", "Σ"),
        ("\\tau", "τ"),
        ("\\phi", "φ"),
        ("\\omega", "ω"),
        ("\\Omega", "Ω"),
        ("\\times", "×"),
        ("\\cdot", "·"),
        ("\\div", "÷"),
        ("\\pm", "±"),
        ("\\leq", "≤"),
        ("\\geq", "≥"),
        ("\\le", "≤"),
        ("\\ge", "≥"),
        ("\\neq", "≠"),
        ("\\approx", "≈"),
        ("\\infty", "∞"),
        ("\\sum", "∑"),
        ("\\prod", "∏"),
        ("\\int", "∫"),
        ("\\sqrt", "√"),
        ("\\partial", "∂"),
        ("\\rightarrow", "→"),
        ("\\to", "→"),
        ("\\leftarrow", "←"),
        ("\\Rightarrow", "⇒"),
        ("\\in", "∈"),
        ("\\%", "%"),
        ("\\,", " "),
        ("\\;", " "),
        ("\\quad", "  "),
        ("\\left", ""),
        ("\\right", ""),
    ];
    let mut s = m.trim().to_string();
    for (a, b) in SYM {
        // whole command names only (\in must not eat \infty)
        let mut out = String::new();
        let mut rest = s.as_str();
        while let Some(p) = rest.find(a) {
            let after = rest[p + a.len()..].chars().next();
            out.push_str(&rest[..p]);
            if a.chars().last().is_some_and(|c| c.is_ascii_alphabetic()) && after.is_some_and(|c| c.is_ascii_alphabetic()) {
                out.push_str(a);
            } else {
                out.push_str(b);
            }
            rest = &rest[p + a.len()..];
        }
        out.push_str(rest);
        s = out;
    }
    let sup = |c: char| match c {
        '0' => Some('⁰'),
        '1' => Some('¹'),
        '2' => Some('²'),
        '3' => Some('³'),
        '4' => Some('⁴'),
        '5' => Some('⁵'),
        '6' => Some('⁶'),
        '7' => Some('⁷'),
        '8' => Some('⁸'),
        '9' => Some('⁹'),
        'n' => Some('ⁿ'),
        _ => None,
    };
    let c: Vec<char> = s.chars().collect();
    let mut o = String::new();
    let mut i = 0;
    while i < c.len() {
        if c[i] == '^'
            && let Some(x) = c.get(i + 1).and_then(|x| sup(*x))
        {
            o.push(x);
            i += 2;
            continue;
        }
        if c[i] == '\\' && c.get(i + 1).is_some_and(|x| x.is_ascii_alphabetic()) {
            // \frac etc: drop the command, keep its arguments
            i += 1;
            while c.get(i).is_some_and(|x| x.is_ascii_alphabetic()) {
                i += 1;
            }
            continue;
        }
        o.push(c[i]);
        i += 1;
    }
    o
}

/// The document as wrapped plain text.
pub fn write(doc: &Doc, width: usize) -> String {
    lines(doc, width).join("\n") + "\n"
}

/// The document as lines of at most `width` characters.
pub fn lines(doc: &Doc, width: usize) -> Vec<String> {
    let w = width.max(20);
    let mut out = vec![];
    if !doc.title.is_empty() {
        out.push(doc.title.clone());
        out.push("=".repeat(doc.title.chars().count().min(w)));
        if !doc.authors.is_empty() {
            out.push(doc.authors.join(", "));
        }
        if !doc.date.is_empty() {
            out.push(doc.date.clone());
        }
        out.push(String::new());
    }
    for b in &endnotes(&doc.blocks) {
        block(&mut out, b, doc, w, "");
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    out
}

fn wrap(text: &str, width: usize, first: &str, rest: &str) -> Vec<String> {
    let mut out = vec![];
    for para in text.split('\n') {
        let mut cur = first.to_string();
        let mut lead = if out.is_empty() { first } else { rest }.chars().count();
        if !out.is_empty() {
            cur = rest.to_string();
        }
        let mut empty = true;
        for word in para.split_whitespace() {
            let n = word.chars().count();
            if !empty && lead + 1 + n > width {
                out.push(std::mem::replace(&mut cur, rest.to_string()));
                lead = rest.chars().count();
                empty = true;
            }
            if !empty {
                cur.push(' ');
                lead += 1;
            }
            cur.push_str(word);
            lead += n;
            empty = false;
        }
        out.push(cur);
    }
    out
}

fn block(out: &mut Vec<String>, b: &Block, doc: &Doc, w: usize, pre: &str) {
    match b {
        Block::Heading(l, v) => {
            let t = inlines(v);
            out.push(format!("{pre}{}", t));
            let ch = match l {
                1 => '=',
                2 => '-',
                _ => '~',
            };
            out.push(format!("{pre}{}", ch.to_string().repeat(t.chars().count().min(w))));
            out.push(String::new());
        }
        Block::Para(v) => {
            out.extend(wrap(&inlines(v), w, pre, pre));
            out.push(String::new());
        }
        Block::List { ordered, items } => {
            for (i, it) in items.iter().enumerate() {
                let mark = if *ordered { format!("{}. ", i + 1) } else { "• ".into() };
                let mut sub = vec![];
                let inner = format!("{pre}{}", " ".repeat(mark.chars().count()));
                for b in it {
                    block(&mut sub, b, doc, w, &inner);
                }
                sub.retain(|l| !l.trim().is_empty());
                if let Some(f) = sub.first_mut() {
                    *f = format!("{pre}{mark}{}", &f[inner.len().min(f.len())..]);
                }
                out.extend(sub);
            }
            out.push(String::new());
        }
        Block::Code(c) => {
            for l in c.lines() {
                out.push(format!("{pre}    {l}"));
            }
            out.push(String::new());
        }
        Block::Math(m) => {
            out.push(format!("{pre}    {}", math(m)));
            out.push(String::new());
        }
        Block::Quote(v) => {
            let p = format!("{pre}> ");
            for b in v {
                block(out, b, doc, w, &p);
            }
        }
        Block::Table { head, rows } => {
            let n = rows.iter().map(|r| r.len()).chain([head.len()]).max().unwrap_or(0);
            let mut widths = vec![0; n];
            for r in std::iter::once(head).chain(rows) {
                for (i, c) in r.iter().enumerate() {
                    widths[i] = widths[i].max(c.chars().count()).min(40);
                }
            }
            let fmt = |r: &[String]| {
                let cells: Vec<String> = (0..n)
                    .map(|i| {
                        let c = r.get(i).map(String::as_str).unwrap_or("");
                        let c: String = c.chars().take(40).collect();
                        format!("{c:<w$}", w = widths[i])
                    })
                    .collect();
                format!("{pre}{}", cells.join("  ").trim_end())
            };
            if !head.is_empty() {
                out.push(fmt(head));
                out.push(format!("{pre}{}", widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("  ")));
            }
            for r in rows {
                out.push(fmt(r));
            }
            out.push(String::new());
        }
        Block::Image { key, caption } => {
            let name = doc.images.get(key).map(|i| i.name.clone()).unwrap_or_else(|| key.clone());
            let c = if caption.is_empty() || *caption == name { String::new() } else { format!(": {caption}") };
            out.push(format!("{pre}[image {name}{c}]"));
            out.push(String::new());
        }
        Block::Rule => {
            out.push(format!("{pre}{}", "─".repeat(w.min(60))));
            out.push(String::new());
        }
        Block::Endnotes(v) => {
            for (n, c) in v {
                out.extend(wrap(&inlines(c), w, &format!("{pre}[{n}] "), &format!("{pre}    ")));
            }
            out.push(String::new());
        }
    }
}

fn inlines(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Bold(c) => s.push_str(&inlines(c)),
            Inline::Italic(c) => s.push_str(&inlines(c)),
            Inline::Mono(t) => s.push_str(t),
            Inline::Math(m) => s.push_str(&math(m)),
            Inline::Mark(c) => s.push_str(&format!("[{}]", inlines(c))),
            Inline::Link(c, u) => {
                let t = inlines(c);
                if t == *u { s.push_str(u) } else { s.push_str(&format!("{t} <{u}>")) }
            }
            Inline::Note(c) => s.push_str(&format!(" ({})", inlines(c))),
            Inline::NoteRef(n) => s.push_str(&format!("[{n}]")),
            Inline::Break => s.push('\n'),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readable_math_and_text() {
        assert_eq!(math("x^2 + \\alpha \\cdot \\infty \\in S"), "x² + α · ∞ ∈ S");
        assert_eq!(math("\\frac{a}{b}"), "{a}{b}");
        let t = write(&super::super::sample::doc(), 60);
        assert!(t.starts_with("Weekly review & notes\n====="));
        assert!(t.contains("BTC [broke out] on thin volume"));
        assert!(t.contains("1. buy"));
        assert!(t.contains("[1] Checked twice — Ada"));
        assert!(t.lines().all(|l| l.chars().count() <= 60 || l.starts_with("    ")), "{t}");
    }
}
