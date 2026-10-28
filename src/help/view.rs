//! The TUI help screen: topic view with link navigation, history and live search.
use super::{Topic, inline_links, resolve_href, search, topic};
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};

pub struct HelpView {
    pub current: Topic,
    pub history: Vec<(String, u16)>,
    pub scroll: u16,
    /// Index of the selected internal link in the current topic.
    pub link: Option<usize>,
    pub searching: bool,
    pub query: String,
    pub results: Vec<Topic>,
    pub result_sel: ListState,
}

impl HelpView {
    pub fn open(id: &str) -> Self {
        let current = topic(id).or_else(|| topic("README")).expect("docs/README.md is embedded");
        let mut v = Self {
            current,
            history: vec![],
            scroll: 0,
            link: None,
            searching: false,
            query: String::new(),
            results: vec![],
            result_sel: ListState::default(),
        };
        v.reset_link();
        v
    }

    /// Internal links of the current topic, in order: (text, target id).
    pub fn internal_links(&self) -> Vec<(String, String)> {
        super::links(self.current.id, self.current.body).into_iter().filter_map(|l| l.target.map(|t| (l.text, t))).collect()
    }

    fn reset_link(&mut self) {
        self.link = if self.internal_links().is_empty() { None } else { Some(0) };
    }

    /// Navigate to a topic, pushing the current one on the history stack.
    pub fn go(&mut self, id: &str) -> bool {
        match topic(id) {
            Some(t) => {
                if t.id != self.current.id {
                    self.history.push((self.current.id.to_string(), self.scroll));
                }
                self.current = t;
                self.scroll = 0;
                self.reset_link();
                true
            }
            None => false,
        }
    }

    pub fn back(&mut self) -> bool {
        match self.history.pop() {
            Some((id, scroll)) => {
                if let Some(t) = topic(&id) {
                    self.current = t;
                    self.scroll = scroll;
                    self.reset_link();
                }
                true
            }
            None => false,
        }
    }

    pub fn follow(&mut self) -> bool {
        let links = self.internal_links();
        match self.link.and_then(|i| links.get(i)) {
            Some((_, target)) => {
                let target = target.clone();
                self.go(&target)
            }
            None => false,
        }
    }

    pub fn update_search(&mut self) {
        self.results = search(&self.query);
        self.result_sel.select(if self.results.is_empty() { None } else { Some(0) });
    }

    /// Handle a key; returns true when help should close.
    pub fn on_key(&mut self, k: KeyEvent) -> bool {
        if self.searching {
            match k.code {
                KeyCode::Esc => self.searching = false,
                KeyCode::Enter => {
                    if let Some(t) = self.result_sel.selected().and_then(|i| self.results.get(i)).map(|t| t.id) {
                        self.searching = false;
                        self.go(t);
                    }
                }
                KeyCode::Down => {
                    let n = self.results.len();
                    if n > 0 {
                        self.result_sel.select(Some((self.result_sel.selected().unwrap_or(0) + 1).min(n - 1)));
                    }
                }
                KeyCode::Up => {
                    self.result_sel.select(Some(self.result_sel.selected().unwrap_or(0).saturating_sub(1)));
                }
                KeyCode::Backspace => {
                    self.query.pop();
                    self.update_search();
                }
                KeyCode::Char(c) => {
                    self.query.push(c);
                    self.update_search();
                }
                _ => {}
            }
            return false;
        }
        match k.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(1) => return true,
            KeyCode::Char('/') | KeyCode::Char('s') => {
                self.searching = true;
                self.update_search();
            }
            KeyCode::Left if k.modifiers.contains(KeyModifiers::ALT) => {
                self.back();
            }
            KeyCode::Backspace => {
                self.back();
            }
            KeyCode::Enter => {
                self.follow();
            }
            KeyCode::Tab | KeyCode::Right => {
                let n = self.internal_links().len();
                if n > 0 {
                    self.link = Some(self.link.map(|i| (i + 1) % n).unwrap_or(0));
                }
            }
            KeyCode::BackTab | KeyCode::Left => {
                let n = self.internal_links().len();
                if n > 0 {
                    self.link = Some(self.link.map(|i| (i + n - 1) % n).unwrap_or(0));
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.scroll = self.scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(15),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(15),
            KeyCode::Home => self.scroll = 0,
            KeyCode::Char('i') => {
                self.go("README");
            }
            _ => {}
        }
        false
    }

    /// Click on a row of the side list: a search result, or a history entry (newest first).
    pub fn click_row(&mut self, i: usize) {
        if self.searching || !self.query.is_empty() {
            if let Some(id) = self.results.get(i).map(|t| t.id) {
                self.searching = false;
                self.go(id);
            }
        } else if i < self.history.len() {
            for _ in 0..=i {
                self.back();
            }
        }
    }

    /// Click on the n-th internal link of the current topic: select and follow it.
    pub fn click_link(&mut self, n: usize) {
        self.searching = false;
        self.link = Some(n);
        self.follow();
    }

    /// Keys shown in the footer bar; every one is clickable.
    fn footer_keys(&self) -> Vec<(&'static str, KeyCode)> {
        if self.searching {
            vec![("Enter open", KeyCode::Enter), ("↑ prev", KeyCode::Up), ("↓ next", KeyCode::Down), ("Esc stop search", KeyCode::Esc)]
        } else {
            vec![
                ("Tab next link", KeyCode::Tab),
                ("Enter follow", KeyCode::Enter),
                ("Bksp back", KeyCode::Backspace),
                ("/ search", KeyCode::Char('/')),
                ("Esc close", KeyCode::Esc),
                ("i index", KeyCode::Char('i')),
                ("S-Tab prev link", KeyCode::BackTab),
                ("PgUp", KeyCode::PageUp),
                ("PgDn", KeyCode::PageDown),
            ]
        }
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, t: &Theme, hits: &mut Vec<(Rect, crate::tui::Hit)>) {
        use crate::tui::Hit;
        let [body, footer] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        let [main, side] = Layout::horizontal([Constraint::Min(0), Constraint::Length(34)]).areas(body);
        let title = format!(" Help: {} ", self.current.title);
        let blk = crate::tui::block(&title, !self.searching, t);
        let inner = blk.inner(main);
        f.render_widget(blk, main);
        let (lines, links) = layout(render_tagged(self.current.id, self.current.body, t, self.link), inner.width.max(1));
        // keep the selected link on screen
        if let Some((row, ..)) = self.link.and_then(|n| links.iter().find(|l| l.3 == n)) {
            let h = inner.height.max(1) as usize;
            if *row < self.scroll as usize {
                self.scroll = *row as u16;
            } else if *row >= self.scroll as usize + h {
                self.scroll = (*row + 1 - h) as u16;
            }
        }
        let max = lines.len().saturating_sub(1) as u16;
        self.scroll = self.scroll.min(max);
        f.render_widget(Paragraph::new(lines).scroll((self.scroll, 0)), inner);
        for (row, x, w, n) in links {
            let Some(y) = row.checked_sub(self.scroll as usize) else { continue };
            if y < inner.height as usize {
                hits.push((Rect { x: inner.x + x, y: inner.y + y as u16, width: w, height: 1 }, Hit::HelpLink(n)));
            }
        }
        let [search_box, list] = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).areas(side);
        let q = if self.searching {
            format!("{}▏", self.query)
        } else if self.query.is_empty() {
            "/ to search".into()
        } else {
            self.query.clone()
        };
        f.render_widget(Paragraph::new(q).block(crate::tui::block("Search", self.searching, t)), search_box);
        hits.push((search_box, Hit::Press(KeyCode::Char('/'))));
        let showing_results = self.searching || !self.query.is_empty();
        let items: Vec<ListItem> = if showing_results {
            self.results.iter().map(|r| ListItem::new(r.title.clone())).collect()
        } else {
            self.history.iter().rev().map(|(id, _)| ListItem::new(format!("← {id}"))).collect()
        };
        let n = items.len();
        let title = if showing_results { "Results" } else { "History" };
        f.render_stateful_widget(
            List::new(items).block(crate::tui::block(title, false, t)).highlight_style(Style::new().bg(t.selection_bg).fg(t.selection_fg)),
            list,
            &mut self.result_sel,
        );
        let off = if showing_results { self.result_sel.offset() } else { 0 };
        for i in off..n {
            let y = list.y + 1 + (i - off) as u16;
            if y + 1 >= list.bottom() {
                break;
            }
            hits.push((Rect { x: list.x + 1, y, width: list.width.saturating_sub(2), height: 1 }, Hit::Row(i)));
        }
        let keys = self.footer_keys();
        crate::tui::widgets::key_bar(f, Rect { x: footer.x + 1, width: footer.width.saturating_sub(2), ..footer }, &keys, t, hits);
    }
}

/// Where a link landed on screen: (row, x, width, link index).
pub type LinkPos = (usize, u16, u16, usize);

/// Word-wrap tagged lines to `width`; returns the lines and each link's (row, x, width, link index).
pub fn layout(tagged: Vec<Vec<(Span<'static>, Option<usize>)>>, width: u16) -> (Vec<Line<'static>>, Vec<LinkPos>) {
    let w = width as usize;
    let mut out: Vec<Line<'static>> = vec![];
    let mut links = vec![];
    for line in tagged {
        let mut cur: Vec<Span<'static>> = vec![];
        let mut x = 0usize;
        for (span, tag) in line {
            let style = span.style;
            // split into words, keeping the trailing spaces
            let mut words: Vec<String> = vec![];
            for (i, part) in span.content.split(' ').enumerate() {
                if i > 0 {
                    if let Some(l) = words.last_mut() {
                        l.push(' ');
                    } else {
                        words.push(" ".into());
                    }
                }
                if !part.is_empty() {
                    words.push(part.to_string());
                }
            }
            for word in words {
                let ww = word.chars().count();
                let trimmed = word.trim_end().chars().count();
                if x > 0 && x + trimmed > w {
                    out.push(Line::from(std::mem::take(&mut cur)));
                    x = 0;
                }
                if let Some(n) = tag {
                    let lw = trimmed.min(w.saturating_sub(x)).max(1);
                    match links.last_mut() {
                        Some((r, lx, lwid, ln)) if *ln == n && *r == out.len() && (*lx as usize + *lwid as usize) <= x => {
                            *lwid = (x + lw - *lx as usize) as u16;
                        }
                        _ => links.push((out.len(), x as u16, lw as u16, n)),
                    }
                }
                x += ww;
                cur.push(Span::styled(word, style));
            }
        }
        out.push(Line::from(cur));
    }
    (out, links)
}

/// Render markdown with theme colors: headings, lists, code spans/blocks and links.
#[cfg(test)]
pub fn render(id: &str, body: &str, t: &Theme, selected_link: Option<usize>) -> Vec<Line<'static>> {
    render_tagged(id, body, t, selected_link).into_iter().map(|l| Line::from(l.into_iter().map(|(s, _)| s).collect::<Vec<_>>())).collect()
}

/// Like [`render`], but every span carries the index of the internal link it belongs to.
pub fn render_tagged(id: &str, body: &str, t: &Theme, selected_link: Option<usize>) -> Vec<Vec<(Span<'static>, Option<usize>)>> {
    let mut out: Vec<Vec<(Span<'static>, Option<usize>)>> = vec![];
    let mut in_code = false;
    let mut link_idx = 0usize;
    for raw in body.lines() {
        if raw.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push(vec![(Span::styled(format!("    {raw}"), Style::new().fg(t.info)), None)]);
            continue;
        }
        if let Some(h) = raw.strip_prefix("# ") {
            out.push(vec![(Span::styled(h.to_uppercase(), Style::new().fg(t.header).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)), None)]);
            continue;
        }
        if let Some(h) = raw.strip_prefix("## ").or_else(|| raw.strip_prefix("### ")) {
            out.push(vec![(Span::styled(h.to_string(), Style::new().fg(t.header).add_modifier(Modifier::BOLD)), None)]);
            continue;
        }
        let (prefix, rest) = if let Some(r) = raw.strip_prefix("- ").or_else(|| raw.strip_prefix("* ")) {
            ("  • ".to_string(), r)
        } else if let Some(r) = raw.strip_prefix("  - ") {
            ("      ◦ ".to_string(), r)
        } else {
            (String::new(), raw)
        };
        let mut spans = vec![(Span::styled(prefix, Style::new().fg(t.accent)), None)];
        spans.extend(inline(id, rest, t, selected_link, &mut link_idx));
        out.push(spans);
    }
    out
}

fn inline(id: &str, s: &str, t: &Theme, selected: Option<usize>, link_idx: &mut usize) -> Vec<(Span<'static>, Option<usize>)> {
    let mut spans = vec![];
    let links = inline_links(s);
    let mut li = links.into_iter().peekable();
    let mut buf = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let flush = |buf: &mut String, spans: &mut Vec<(Span<'static>, Option<usize>)>| {
        if !buf.is_empty() {
            spans.push((Span::styled(std::mem::take(buf), Style::new().fg(t.fg)), None));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '`'
            && let Some(end) = (i + 1..chars.len()).find(|&j| chars[j] == '`')
        {
            flush(&mut buf, &mut spans);
            let code: String = chars[i + 1..end].iter().collect();
            spans.push((Span::styled(code, Style::new().fg(t.accent)), None));
            i = end + 1;
            continue;
        }
        if c == '['
            && let Some((text, href)) = li.peek().cloned()
        {
            let pat: Vec<char> = format!("[{text}]({href})").chars().collect();
            if chars[i..].starts_with(&pat) {
                li.next();
                flush(&mut buf, &mut spans);
                let internal = resolve_href(id, &href).is_some();
                let mut st = Style::new().fg(t.info).add_modifier(Modifier::UNDERLINED);
                if internal {
                    if selected == Some(*link_idx) {
                        st = Style::new().bg(t.selection_bg).fg(t.selection_fg).add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
                    }
                    spans.push((Span::styled(text, st), Some(*link_idx)));
                    *link_idx += 1;
                } else {
                    spans.push((Span::styled(format!("{text} <{href}>"), st), None));
                }
                i += pat.len();
                continue;
            }
        }
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            i += 2;
            continue;
        }
        buf.push(c);
        i += 1;
    }
    flush(&mut buf, &mut spans);
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyEventKind, KeyEventState};

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent { code: c, modifiers: KeyModifiers::NONE, kind: KeyEventKind::Press, state: KeyEventState::NONE }
    }

    #[test]
    fn follow_links_and_back() {
        let mut v = HelpView::open("README");
        let links = v.internal_links();
        assert!(!links.is_empty());
        let first = links[0].1.clone();
        v.on_key(key(KeyCode::Enter));
        assert_eq!(v.current.id, first);
        v.on_key(key(KeyCode::Backspace));
        assert_eq!(v.current.id, "README");
        let alt_left = KeyEvent { code: KeyCode::Left, modifiers: KeyModifiers::ALT, kind: KeyEventKind::Press, state: KeyEventState::NONE };
        v.go("keys");
        v.go("encryption");
        v.on_key(alt_left);
        assert_eq!(v.current.id, "keys");
        assert_eq!(v.history.len(), 1);
    }

    #[test]
    fn live_search() {
        let mut v = HelpView::open("README");
        v.on_key(key(KeyCode::Char('/')));
        for c in "redis".chars() {
            v.on_key(key(KeyCode::Char(c)));
        }
        assert_eq!(v.results[0].id, "services/redis");
        v.on_key(key(KeyCode::Enter));
        assert_eq!(v.current.id, "services/redis");
        assert!(v.on_key(key(KeyCode::Esc)));
    }

    #[test]
    fn every_topic_renders() {
        let t = crate::theme::builtin("tokyo-night").unwrap().resolved(crate::theme::Variant::Dark);
        for tp in super::super::topics() {
            assert!(!render(tp.id, tp.body, &t, Some(0)).is_empty());
        }
    }
}
