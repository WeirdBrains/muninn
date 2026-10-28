//! The TUI help screen: topic view with link navigation, history and live search.
use super::{inline_links, resolve_href, search, topic, Topic};
use crate::theme::Theme;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

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
        super::links(self.current.id, self.current.body)
            .into_iter()
            .filter_map(|l| l.target.map(|t| (l.text, t)))
            .collect()
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

    pub fn draw(&mut self, f: &mut Frame, area: Rect, t: &Theme) {
        let [main, side] = Layout::horizontal([Constraint::Min(0), Constraint::Length(34)]).areas(area);
        let lines = render(self.current.id, self.current.body, t, self.link);
        let title = format!(" Help: {} ", self.current.title);
        f.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: false })
                .scroll((self.scroll, 0))
                .block(crate::tui::block(&title, !self.searching, t)),
            main,
        );
        let [search_box, list, keys] = Layout::vertical([Constraint::Length(3), Constraint::Min(0), Constraint::Length(8)]).areas(side);
        let q = if self.searching { format!("{}▏", self.query) } else if self.query.is_empty() { "/ to search".into() } else { self.query.clone() };
        f.render_widget(Paragraph::new(q).block(crate::tui::block("Search", self.searching, t)), search_box);
        let items: Vec<ListItem> = if self.searching || !self.query.is_empty() {
            self.results.iter().map(|r| ListItem::new(r.title.clone())).collect()
        } else {
            self.history.iter().rev().map(|(id, _)| ListItem::new(format!("← {id}"))).collect()
        };
        let title = if self.searching || !self.query.is_empty() { "Results" } else { "History" };
        f.render_stateful_widget(
            List::new(items)
                .block(crate::tui::block(title, false, t))
                .highlight_style(Style::new().bg(t.selection_bg).fg(t.selection_fg)),
            list,
            &mut self.result_sel,
        );
        let help = vec![
            Line::from("Tab/→ ←  select link"),
            Line::from("Enter    follow link"),
            Line::from("Bksp/Alt-← back"),
            Line::from("/        search"),
            Line::from("i        index"),
            Line::from("Esc/q/F1 close"),
        ];
        f.render_widget(Paragraph::new(help).style(Style::new().fg(t.muted)).block(crate::tui::block("Keys", false, t)), keys);
    }
}

/// Render markdown with theme colors: headings, lists, code spans/blocks and links.
pub fn render(id: &str, body: &str, t: &Theme, selected_link: Option<usize>) -> Vec<Line<'static>> {
    let mut out = vec![];
    let mut in_code = false;
    let mut link_idx = 0usize;
    for raw in body.lines() {
        if raw.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            out.push(Line::styled(format!("    {raw}"), Style::new().fg(t.info)));
            continue;
        }
        if let Some(h) = raw.strip_prefix("# ") {
            out.push(Line::styled(h.to_uppercase(), Style::new().fg(t.header).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)));
            continue;
        }
        if let Some(h) = raw.strip_prefix("## ").or_else(|| raw.strip_prefix("### ")) {
            out.push(Line::styled(h.to_string(), Style::new().fg(t.header).add_modifier(Modifier::BOLD)));
            continue;
        }
        let (prefix, rest) = if let Some(r) = raw.strip_prefix("- ").or_else(|| raw.strip_prefix("* ")) {
            ("  • ".to_string(), r)
        } else if let Some(r) = raw.strip_prefix("  - ") {
            ("      ◦ ".to_string(), r)
        } else {
            (String::new(), raw)
        };
        let mut spans = vec![Span::styled(prefix, Style::new().fg(t.accent))];
        spans.extend(inline(id, rest, t, selected_link, &mut link_idx));
        out.push(Line::from(spans));
    }
    out
}

fn inline(id: &str, s: &str, t: &Theme, selected: Option<usize>, link_idx: &mut usize) -> Vec<Span<'static>> {
    let mut spans = vec![];
    let links = inline_links(s);
    let mut li = links.into_iter().peekable();
    let mut buf = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let flush = |buf: &mut String, spans: &mut Vec<Span<'static>>| {
        if !buf.is_empty() {
            spans.push(Span::styled(std::mem::take(buf), Style::new().fg(t.fg)));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '`'
            && let Some(end) = (i + 1..chars.len()).find(|&j| chars[j] == '`') {
                flush(&mut buf, &mut spans);
                let code: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(code, Style::new().fg(t.accent)));
                i = end + 1;
                continue;
            }
        if c == '['
            && let Some((text, href)) = li.peek().cloned() {
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
                        *link_idx += 1;
                        spans.push(Span::styled(text, st));
                    } else {
                        spans.push(Span::styled(format!("{text} <{href}>"), st));
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
