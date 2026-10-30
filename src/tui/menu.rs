//! Menu bar with dropdowns and nested submenus.
use super::Hit;
use super::action::{Action, ContainerMode, FilterKind, Preset as P, ViewKind};
use crate::prefs::ThemeMode as TM;
use crate::theme::Theme;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

#[derive(Debug, Clone)]
pub enum Kind {
    Action(Action),
    Sub(Vec<Item>),
    Sep,
}

#[derive(Debug, Clone)]
pub struct Item {
    pub label: &'static str,
    pub mnemonic: Option<char>,
    pub shortcut: &'static str,
    pub kind: Kind,
}

fn a(label: &'static str, m: char, shortcut: &'static str, act: Action) -> Item {
    Item { label, mnemonic: Some(m), shortcut, kind: Kind::Action(act) }
}
fn sub(label: &'static str, m: char, items: Vec<Item>) -> Item {
    Item { label, mnemonic: Some(m), shortcut: "", kind: Kind::Sub(items) }
}
fn sep() -> Item {
    Item { label: "", mnemonic: None, shortcut: "", kind: Kind::Sep }
}

#[derive(Debug, Clone)]
pub struct Menu {
    pub title: &'static str,
    pub mnemonic: char,
    pub items: Vec<Item>,
}

/// The menu tree (docs/historical/1.0/PLAN.md 1.3).
pub fn menus() -> Vec<Menu> {
    use Action::*;
    vec![
        Menu {
            title: "File",
            mnemonic: 'f',
            items: vec![
                a("Import…", 'i', "i", Import),
                a("Export…", 'e', "", Export),
                sub("Backup", 'b', vec![a("Create", 'c', "", BackupCreate), a("Restore", 'r', "", BackupRestore)]),
                a("Authors…", 'a', "A", Authors),
                a("Preferences…", 'p', "", Preferences),
                sep(),
                a("Quit", 'q', "q", Quit),
            ],
        },
        Menu {
            title: "View",
            mnemonic: 'v',
            items: vec![
                sub("New tile", 'n', vec![a("List", 'l', "", NewTile(ViewKind::List)), a("Chart", 'c', "", NewTile(ViewKind::Chart))]),
                sub("Split", 's', vec![a("Right", 'r', "C-w v", SplitRight), a("Below", 'b', "C-w s", SplitBelow)]),
                a("Close tile", 'c', "C-w q", CloseTile),
                a("Zoom tile", 'z', "C-w z", ZoomTile),
                a("Equalize", 'e', "C-w =", Equalize),
                a("Change view", 'v', "C-w t", ChangeView),
                a("Link tile", 'k', "C-w y", LinkTile),
                a("Chart layers…", 'a', "L", ChartLayers),
                sub(
                    "Container",
                    'o',
                    vec![
                        a("Split", 's', "", Container(ContainerMode::Split)),
                        a("Tabbed", 't', "", Container(ContainerMode::Tabbed)),
                        a("Stacked", 'k', "", Container(ContainerMode::Stacked)),
                    ],
                ),
                sub(
                    "Layouts",
                    'l',
                    vec![
                        a("2-up", '2', "", Preset(P::TwoUp)),
                        a("3-column", '3', "", Preset(P::ThreeColumn)),
                        a("Main + stack", 'm', "", Preset(P::MainStack)),
                        a("2×2", 'g', "", Preset(P::Grid2x2)),
                        sep(),
                        a("Saved…", 'o', "", LayoutsSaved),
                        a("Save current…", 's', "", LayoutSave),
                    ],
                ),
                sep(),
                a("Toggle sidebar", 't', "C-b", ToggleSidebar),
                sub(
                    "History",
                    'h',
                    vec![
                        a("Show/hide history bar", 'h', "Y", ToggleHistory),
                        a("Step back", 'b', "u", HistoryBack),
                        a("Step forward", 'f', "U", HistoryForward),
                    ],
                ),
            ],
        },
        Menu {
            title: "Data",
            mnemonic: 'd',
            items: vec![
                a("Sources…", 's', "", Sources),
                a("Symbols…", 'y', "", Symbols),
                sub(
                    "Filters",
                    'f',
                    vec![
                        a("Source", 's', "", Filter(FilterKind::Source)),
                        a("Symbol", 'm', "", Filter(FilterKind::Symbol)),
                        a("Time range…", 't', "T", Filter(FilterKind::Time)),
                        a("Price", 'p', "", Filter(FilterKind::Price)),
                        a("Period", 'e', "", Filter(FilterKind::Period)),
                        a("Year", 'y', "", Filter(FilterKind::Year)),
                        sep(),
                        a("Clear all", 'c', "X", ClearFilters),
                    ],
                ),
                sub("Sort", 'o', vec![a("Year", 'y', "", SortYear), a("Name", 'n', "", SortName), a("Asc/Desc", 'a', "o", ToggleOrder)]),
                a("Coverage & gaps", 'c', "", Coverage),
                sep(),
                a("Model designer…", 'm', "D", Models),
                sep(),
                a("Refresh", 'r', "F5", Refresh),
            ],
        },
        Menu {
            title: "Projects",
            mnemonic: 'p',
            items: vec![
                a("Projects…", 'p', "P", Projects),
                a("New project…", 'n', "", NewProject),
                sep(),
                a("Live searches…", 'l', "F", LiveSearches),
                a("Save search as live search…", 's', "B", SaveSearch),
                sep(),
                a("Journals…", 'j', "J", Journals),
                a("New journal…", 'e', "", NewJournal),
                sep(),
                a("Reports…", 'r', "O", Reports),
                a("New report…", 'w', "", NewReport),
                sep(),
                a("Rules…", 'u', "R", Rules),
                a("Code spaces…", 'c', "C", Code),
                sep(),
                a("Add selected series to project…", 'a', "a", AddToProject),
            ],
        },
        Menu {
            title: "Network",
            mnemonic: 'n',
            items: vec![
                a("Node status…", 'n', "K", NetStatus),
                a("Start node", 's', "", NetStart),
                a("Stop node", 't', "", NetStop),
                sep(),
                a("Peers…", 'p', "", NetPeers),
                a("Relays…", 'r', "", NetRelays),
                a("Key exchanges…", 'k', "", NetExchanges),
                a("Shared data…", 'd', "", NetShared),
                a("Dark shards…", 'h', "", NetShards),
                a("Network search…", 'e', "", NetSearch),
                sep(),
                a("Settings…", 'g', "", NetSettings),
            ],
        },
        Menu {
            title: "Organizations",
            mnemonic: 'o',
            items: vec![
                a("Organizations…", 'o', "G", Orgs),
                a("New organization…", 'n', "", OrgNew),
                sep(),
                a("Members…", 'm', "", OrgMembers),
                a("Roles…", 'r', "", OrgRoles),
                a("Resources…", 'e', "", OrgResources),
                a("Joined & browse…", 'j', "", OrgJoined),
                sep(),
                a("Site & certificate…", 's', "", OrgSite),
                a("Start site", 't', "", OrgHostStart),
                a("Stop site", 'p', "", OrgHostStop),
            ],
        },
        Menu {
            title: "Keys",
            mnemonic: 'k',
            items: vec![
                a("Initialize…", 'i', "", KeysInit),
                a("Lock", 'l', "", Lock),
                a("Rotate service key", 'r', "", RotateKey),
                a("Rekey database", 'k', "", Rekey),
                a("Export…", 'e', "", KeysExport),
                sub("Recover", 'v', vec![a("From words", 'w', "", RecoverWords), a("From shards", 's', "", RecoverShards)]),
                a("Re-issue shards", 's', "", ReissueShards),
            ],
        },
        Menu {
            title: "Theme",
            mnemonic: 't',
            items: vec![
                a("Select…", 's', "", ThemeSelect),
                sub(
                    "Mode",
                    'm',
                    vec![
                        a("Light", 'l', "", ThemeMode(TM::Light)),
                        a("Dark", 'd', "", ThemeMode(TM::Dark)),
                        a("System", 's', "", ThemeMode(TM::System)),
                    ],
                ),
                a("Edit current…", 'e', "", ThemeEdit),
                a("New from current…", 'n', "", ThemeNew),
                a("Import…", 'i', "", ThemeImport),
                a("Export…", 'x', "", ThemeExport),
            ],
        },
        Menu {
            title: "Help",
            mnemonic: 'h',
            items: vec![
                a("Help topics", 'h', "? F1", Help),
                a("Search help…", 's', "", HelpSearch),
                a("Keybindings", 'k', "", Keybindings),
                a("About Muninn", 'a', "", About),
            ],
        },
    ]
}

/// Every action reachable from the menus (for tests and the keybinding reference).
pub fn all_actions() -> Vec<Action> {
    fn walk(items: &[Item], out: &mut Vec<Action>) {
        for i in items {
            match &i.kind {
                Kind::Action(a) => out.push(*a),
                Kind::Sub(s) => walk(s, out),
                Kind::Sep => {}
            }
        }
    }
    let mut out = vec![];
    for m in menus() {
        walk(&m.items, &mut out);
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct MenuState {
    /// Open top-level menu.
    pub open: Option<usize>,
    /// Selected item per open level (level 0 = dropdown).
    pub path: Vec<usize>,
}

pub enum Outcome {
    Consumed,
    Dispatch(Action),
    Closed,
}

impl MenuState {
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }
    pub fn open_top(&mut self, i: usize) {
        self.open = Some(i);
        self.path = vec![first_selectable(&menus()[i].items, 0, 1)];
    }
    pub fn close(&mut self) {
        self.open = None;
        self.path.clear();
    }

    fn level_items(&self, menus: &[Menu], level: usize) -> Option<Vec<Item>> {
        let mut items = menus.get(self.open?)?.items.clone();
        for l in 0..level {
            match &items.get(*self.path.get(l)?)?.kind {
                Kind::Sub(s) => items = s.clone(),
                _ => return None,
            }
        }
        Some(items)
    }

    /// Functional wrapper used by the app (avoids borrowing it twice).
    pub fn on_key_with(mut self, k: KeyEvent, enabled: &dyn Fn(Action) -> bool) -> (Self, Outcome) {
        let o = self.on_key(k, enabled);
        (self, o)
    }

    pub fn on_key(&mut self, k: KeyEvent, enabled: &dyn Fn(Action) -> bool) -> Outcome {
        let ms = menus();
        let Some(top) = self.open else { return Outcome::Closed };
        let depth = self.path.len() - 1;
        let items = self.level_items(&ms, depth).unwrap_or_default();
        let sel = self.path[depth];
        // Mnemonics win over h/j/k/l when an item uses that letter.
        if let KeyCode::Char(c) = k.code {
            let c = c.to_ascii_lowercase();
            if let Some(i) = items.iter().position(|it| it.mnemonic == Some(c)) {
                return self.activate(i, enabled);
            }
        }
        match k.code {
            KeyCode::Esc => {
                if depth == 0 {
                    self.close();
                    return Outcome::Closed;
                }
                self.path.pop();
            }
            KeyCode::Down | KeyCode::Char('j') => self.path[depth] = first_selectable(&items, (sel + 1) % items.len(), 1),
            KeyCode::Up | KeyCode::Char('k') => self.path[depth] = first_selectable(&items, (sel + items.len() - 1) % items.len(), -1),
            KeyCode::Left | KeyCode::Char('h') => {
                if depth > 0 {
                    self.path.pop();
                } else {
                    self.open_top((top + ms.len() - 1) % ms.len());
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if matches!(items.get(sel).map(|i| &i.kind), Some(Kind::Sub(_))) {
                    return self.activate(sel, enabled);
                }
                self.open_top((top + 1) % ms.len());
            }
            KeyCode::Enter | KeyCode::Char(' ') => return self.activate(sel, enabled),
            _ => {}
        }
        Outcome::Consumed
    }

    /// Activate item `i` on the deepest open level.
    pub fn activate(&mut self, i: usize, enabled: &dyn Fn(Action) -> bool) -> Outcome {
        let ms = menus();
        let depth = self.path.len() - 1;
        let items = self.level_items(&ms, depth).unwrap_or_default();
        self.path[depth] = i;
        match items.get(i).map(|x| x.kind.clone()) {
            Some(Kind::Action(a)) if enabled(a) => {
                self.close();
                Outcome::Dispatch(a)
            }
            Some(Kind::Sub(s)) => {
                self.path.push(first_selectable(&s, 0, 1));
                Outcome::Consumed
            }
            _ => Outcome::Consumed,
        }
    }

    /// Mouse click on item `idx` of level `depth`.
    pub fn click(&mut self, depth: usize, idx: usize, enabled: &dyn Fn(Action) -> bool) -> Outcome {
        self.path.truncate(depth + 1);
        self.activate(idx, enabled)
    }
}

fn first_selectable(items: &[Item], start: usize, step: i32) -> usize {
    let n = items.len();
    if n == 0 {
        return 0;
    }
    let mut i = start % n;
    for _ in 0..n {
        if !matches!(items[i].kind, Kind::Sep) {
            return i;
        }
        i = ((i as i32 + step).rem_euclid(n as i32)) as usize;
    }
    start
}

/// Label with its mnemonic letter underlined.
fn label_spans(label: &str, m: Option<char>, st: Style) -> Vec<Span<'static>> {
    let pos = m.and_then(|m| label.char_indices().find(|(_, c)| c.to_ascii_lowercase() == m).map(|(i, _)| i));
    match pos {
        Some(p) => {
            let c = label[p..].chars().next().unwrap();
            vec![
                Span::styled(label[..p].to_string(), st),
                Span::styled(c.to_string(), st.add_modifier(Modifier::UNDERLINED)),
                Span::styled(label[p + c.len_utf8()..].to_string(), st),
            ]
        }
        None => vec![Span::styled(label.to_string(), st)],
    }
}

/// Draw the bar; returns the x of each top label (for dropdown placement).
pub fn draw_bar(state: &MenuState, f: &mut Frame, area: Rect, t: &Theme, hits: &mut Vec<(Rect, Hit)>) -> Vec<u16> {
    let base = Style::new().fg(t.fg).bg(t.selection_bg);
    f.render_widget(Block::new().style(base), area);
    let mut spans = vec![Span::styled(" ", base)];
    let mut x = area.x + 1;
    let mut xs = vec![];
    for (i, m) in menus().iter().enumerate() {
        let st = if state.open == Some(i) { Style::new().fg(t.bg).bg(t.accent).add_modifier(Modifier::BOLD) } else { base };
        let w = m.title.chars().count() as u16 + 2;
        xs.push(x);
        hits.push((Rect { x, y: area.y, width: w, height: 1 }, Hit::MenuTop(i)));
        spans.push(Span::styled(" ", st));
        spans.extend(label_spans(m.title, Some(m.mnemonic), st));
        spans.push(Span::styled(" ", st));
        spans.push(Span::styled(" ", base));
        x += w + 1;
    }
    spans.push(Span::styled("  F10 menu · Alt+letter", Style::new().fg(t.muted).bg(t.selection_bg)));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
    xs
}

/// Draw open dropdowns on top of everything.
#[allow(clippy::too_many_arguments)]
pub fn draw_dropdowns(
    state: &MenuState,
    f: &mut Frame,
    screen: Rect,
    bar: Rect,
    xs: &[u16],
    t: &Theme,
    enabled: &dyn Fn(Action) -> bool,
    hits: &mut Vec<(Rect, Hit)>,
) {
    let Some(top) = state.open else { return };
    let ms = menus();
    let mut x = xs.get(top).copied().unwrap_or(bar.x);
    let mut y = bar.y + 1;
    for depth in 0..state.path.len() {
        let Some(items) = state.level_items(&ms, depth) else { break };
        let lw = items.iter().map(|i| i.label.chars().count()).max().unwrap_or(4);
        let sw = items.iter().map(|i| i.shortcut.chars().count()).max().unwrap_or(0);
        let width = (lw + sw + 7) as u16;
        let height = items.len() as u16 + 2;
        let rx = x.min(screen.right().saturating_sub(width));
        let ry = y.min(screen.bottom().saturating_sub(height));
        let r = Rect { x: rx, y: ry, width: width.min(screen.width), height: height.min(screen.height) };
        f.render_widget(Clear, r);
        let block = Block::bordered().border_style(Style::new().fg(t.border_focus)).style(Style::new().bg(t.bg).fg(t.fg));
        let inner = block.inner(r);
        f.render_widget(block, r);
        for (i, it) in items.iter().enumerate() {
            let row = Rect { x: inner.x, y: inner.y + i as u16, width: inner.width, height: 1 };
            if row.y >= inner.bottom() {
                break;
            }
            if matches!(it.kind, Kind::Sep) {
                f.render_widget(Paragraph::new("─".repeat(inner.width as usize)).style(Style::new().fg(t.border)), row);
                continue;
            }
            let on = match it.kind {
                Kind::Action(a) => enabled(a),
                _ => true,
            };
            let selected = state.path[depth] == i;
            let mut st = Style::new().fg(if on { t.fg } else { t.muted }).bg(t.bg);
            if selected {
                st = Style::new().fg(t.selection_fg).bg(t.selection_bg).add_modifier(Modifier::BOLD);
            }
            let mut spans = vec![Span::styled(" ", st)];
            spans.extend(label_spans(it.label, if on { it.mnemonic } else { None }, st));
            let used = 1 + it.label.chars().count();
            let tail = match it.kind {
                Kind::Sub(_) => "▸".to_string(),
                _ => it.shortcut.to_string(),
            };
            let pad = (inner.width as usize).saturating_sub(used + tail.chars().count() + 1);
            spans.push(Span::styled(" ".repeat(pad), st));
            spans.push(Span::styled(tail, if selected { st } else { Style::new().fg(t.muted).bg(t.bg) }));
            spans.push(Span::styled(" ", st));
            f.render_widget(Paragraph::new(Line::from(spans)), row);
            hits.push((row, Hit::MenuItem(depth, i)));
        }
        x = r.right();
        y = r.y + 1 + state.path[depth] as u16;
    }
}
