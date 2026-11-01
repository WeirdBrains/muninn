//! Modal dialogs drawn over the content area.
use super::{App, block, centered, forms, themes};
use crate::help::HelpView;
use crate::ingest::time::fmt_ts;
use crate::store::Store;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    ThemeImport,
    ThemeExport,
    LayoutSave,
    JumpDate,
    ChartStart,
    ChartEnd,
    Mas,
    /// A data tile's columns or series.
    FrameColumns,
}

pub struct Prompt {
    pub title: String,
    pub input: String,
    pub kind: PromptKind,
}

pub enum Modal {
    Help(HelpView),
    Export,
    Import,
    Keys,
    Themes,
    /// File ▸ Authors…
    Authors,
    /// Projects ▸ Live searches…
    Searches,
    /// Projects ▸ Projects…
    Projects,
    /// Projects ▸ Journals…
    Journals,
    /// Projects ▸ Reports…
    Reports,
    /// Projects ▸ Rules…
    Rules,
    /// Projects ▸ Code spaces…
    Code,
    /// Projects ▸ Pipes (live)…
    Pipes,
    /// Net ▸ …
    Network,
    /// Orgs ▸ …
    Orgs,
    Reviews,
    /// Data ▸ Model designer…
    Models,
    /// Export a journal, entry, report or project as a document.
    DocExport,
    /// Choose a project (add to project, fork a journal into one).
    PickProject,
    Prefs {
        sel: usize,
    },
    Info {
        title: String,
        lines: Vec<String>,
        scroll: u16,
    },
    Prompt(Prompt),
    /// Date and time picker (period/list range).
    DateTime(super::picker::DatePicker),
    /// Sidebar filter editor.
    Filter {
        kind: super::action::FilterKind,
        sel: usize,
        edit: Option<String>,
    },
    /// Chart layer checklist for the focused chart tile.
    Layers {
        sel: usize,
    },
    /// Pick from a list of strings (e.g. saved layouts).
    Pick {
        title: String,
        items: Vec<String>,
        sel: usize,
        kind: PickKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickKind {
    SavedLayout,
    /// A share target (`super::share`).
    Share,
    /// Where a code run's output goes next (`super::code`): a code space, or a save.
    Pipe,
}

impl Modal {
    pub fn info(title: &str, lines: Vec<String>) -> Self {
        Modal::Info { title: title.into(), lines, scroll: 0 }
    }
    pub fn prompt(title: &str, kind: PromptKind) -> Self {
        Modal::Prompt(Prompt { title: title.into(), input: String::new(), kind })
    }
    pub fn help_topic(&self) -> &'static str {
        match self {
            Modal::Help(_) => "help",
            Modal::Export => "export",
            Modal::Import => "import",
            Modal::Keys => "keys",
            Modal::Themes => "themes",
            Modal::Authors => "authors",
            Modal::Searches => "live-searches",
            Modal::Projects | Modal::PickProject => "projects",
            Modal::Journals => "journals",
            Modal::Reports => "reports",
            Modal::Rules => "rules",
            Modal::Network => "network",
            Modal::Orgs => "organizations",
            Modal::Reviews => "reviews",
            Modal::Code => "code-spaces",
            Modal::Pipes => "pipes",
            Modal::Models => "models",
            Modal::DocExport => "export-formats",
            Modal::Prefs { .. } => "preferences",
            Modal::Info { .. } | Modal::Pick { .. } => "tui",
            Modal::Layers { .. } => "charts",
            Modal::DateTime(p) => match p.target {
                super::picker::Target::ListFrom | super::picker::Target::ListTo => "tiling",
                super::picker::Target::RangeFrom | super::picker::Target::RangeTo => "sidebar",
                _ => "charts",
            },
            Modal::Filter { .. } => "sidebar",
            Modal::Prompt(p) => match p.kind {
                PromptKind::ThemeImport | PromptKind::ThemeExport => "themes",
                PromptKind::LayoutSave => "tiling",
                _ => "charts",
            },
        }
    }
    pub fn typing(&self, app: &App) -> bool {
        match self {
            Modal::Help(h) => h.searching,
            Modal::Export => app.export.typing(),
            Modal::Import => app.import.typing(),
            Modal::Keys => app.keys.typing(),
            Modal::Themes => app.themes.typing(),
            Modal::Authors => app.authors.typing(),
            Modal::Searches => app.searches.typing(),
            Modal::Projects => app.projects.typing(),
            Modal::Journals => app.journals.typing(),
            Modal::Reports => app.reports.typing(),
            Modal::Rules => app.rules.typing(),
            Modal::Network => app.network.typing(),
            Modal::Orgs => app.orgs.typing(),
            Modal::Reviews => app.reviews.typing(),
            Modal::Code => app.code.typing(),
            Modal::Models => app.models.typing(),
            Modal::DocExport => app.doc_export.typing(),
            Modal::PickProject => app.pick_project.typing(),
            Modal::Prompt(_) => true,
            Modal::Filter { edit, .. } => edit.is_some(),
            _ => false,
        }
    }
}

pub fn about_lines() -> Vec<String> {
    vec![
        format!("Muninn v{}", env!("CARGO_PKG_VERSION")),
        String::new(),
        "Local, encrypted store for OHLCV market bars.".into(),
        "Odin's raven of memory: it remembers every bar.".into(),
        String::new(),
        "Help: ? or F1 · Menus: F10 or Alt+letter · Quit: q".into(),
    ]
}

pub fn cli_hint(cmd: &str, why: &str) -> Vec<String> {
    vec![why.into(), String::new(), "Run:".into(), format!("  mnnn {cmd}"), String::new(), "See `mnnn help keys` for details.".into()]
}

pub fn sources_lines(store: &Store) -> Vec<String> {
    match store.sources() {
        Ok(v) if !v.is_empty() => v.iter().map(|s| format!("{:<16} {:<8} {}", s.name, s.kind, s.locator.clone().unwrap_or_default())).collect(),
        Ok(_) => vec!["no sources yet: File ▸ Import…".into()],
        Err(e) => vec![format!("error: {e}")],
    }
}

pub fn symbols_lines(store: &Store) -> Vec<String> {
    match store.symbols() {
        Ok(v) => v.iter().map(|s| s.display()).collect(),
        Err(e) => vec![format!("error: {e}")],
    }
}

pub fn coverage_lines(store: &Store) -> Vec<String> {
    match store.series() {
        Ok(v) if !v.is_empty() => {
            let mut out =
                vec![format!("{:<12} {:<12} {:>4} {:>9} {:>8}  {:<19}  {:<19}", "source", "symbol", "dur", "bars", "missing", "first", "last")];
            for s in v {
                out.push(format!(
                    "{:<12} {:<12} {:>4} {:>9} {:>8}  {:<19}  {:<19}",
                    s.source,
                    s.symbol.display(),
                    s.duration.to_string(),
                    s.count,
                    s.missing(),
                    fmt_ts(s.first),
                    fmt_ts(s.last)
                ));
            }
            out
        }
        Ok(_) => vec!["no data yet: File ▸ Import…".into()],
        Err(e) => vec![format!("error: {e}")],
    }
}

const PREF_ROWS: usize = 4;

/// A click inside the open popup. Rows select and activate (like Enter), key hints
/// press their key, form rows focus a field or run an action.
pub fn on_click(app: &mut App, h: super::Hit) {
    use super::Hit;
    let enter = KeyEvent::from(KeyCode::Enter);
    match h {
        Hit::Press(code) => {
            // A key button in the import picker acts on the file list, not a text input.
            if matches!(app.modal, Some(Modal::Import)) && app.import.typing() && !matches!(code, KeyCode::Enter | KeyCode::Esc) {
                app.import.pane = super::import::Pane::Files;
            }
            on_key(app, KeyEvent::from(code))
        }
        Hit::Chord(code, m) => on_key(app, KeyEvent::new(code, m)),
        Hit::FormRow(i) => {
            let fm = match app.modal {
                Some(Modal::Export) => &mut app.export.form,
                Some(Modal::Keys) => &mut app.keys.form,
                _ => return,
            };
            fm.sel = i;
            fm.editing = false;
            on_key(app, enter);
        }
        Hit::Picker(p) => super::picker::on_click(app, p),
        Hit::Url(i) => app.open_url(i),
        Hit::ShareView => {}
        Hit::HelpLink(n) => {
            if let Some(Modal::Help(h)) = app.modal.as_mut() {
                h.click_link(n);
            }
        }
        Hit::Row(i) => match app.modal.as_mut() {
            Some(Modal::Help(h)) => h.click_row(i),
            Some(Modal::Prefs { sel } | Modal::Layers { sel } | Modal::Pick { sel, .. }) => {
                *sel = i;
                on_key(app, enter);
            }
            Some(Modal::Filter { sel, edit, .. }) if edit.is_none() => {
                *sel = i;
                on_key(app, enter);
            }
            Some(Modal::Authors) => super::authors::on_click(app, i),
            Some(Modal::Searches) => super::searches::on_click(app, i),
            Some(Modal::Projects) => super::projects::on_click(app, i),
            Some(Modal::Journals) => super::journals::on_click(app, i),
            Some(Modal::Reports) => super::reports::on_click(app, i),
            Some(Modal::Rules) => super::rules::on_click(app, i),
            Some(Modal::Network) => super::network::on_click(app, i),
            Some(Modal::Orgs) => super::orgs::on_click(app, i),
            Some(Modal::Reviews) => super::reviews::on_click(app, i),
            Some(Modal::Code) => super::code::on_click(app, i),
            Some(Modal::Models) => super::models::on_click(app, i),
            Some(Modal::DocExport) => super::docexport::on_click(app, i),
            Some(Modal::PickProject) => super::workspace::on_click(app, i),
            Some(Modal::Themes) => {
                if let Some(ed) = app.themes.editor.as_mut() {
                    ed.role = i.min(crate::theme::ROLES.len() - 1);
                } else if app.themes.list.selected() == Some(i) {
                    on_key(app, enter);
                } else if i < app.themes.names.len() {
                    app.themes.list.select(Some(i));
                }
            }
            _ => {}
        },
        h => super::import::on_click(app, h),
    }
}

/// Row hits for a bordered list popup whose rows start at `r.y + 1 + first`.
fn row_hits(hits: &mut Vec<(Rect, super::Hit)>, r: Rect, first: u16, n: usize) {
    for i in 0..n {
        let y = r.y + 1 + first + i as u16;
        if y + 1 >= r.bottom() {
            break;
        }
        hits.push((Rect { x: r.x + 1, y, width: r.width.saturating_sub(2), height: 1 }, super::Hit::Row(i)));
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if let Some(Modal::Filter { kind, sel, edit }) = &app.modal {
        let (kind, sel, edit) = (*kind, *sel, edit.clone());
        return filter_key(app, k, kind, sel, edit);
    }
    if matches!(app.modal, Some(Modal::Import)) {
        return super::import::on_key(app, k);
    }
    if matches!(app.modal, Some(Modal::DateTime(_))) {
        return super::picker::on_key(app, k);
    }
    if k.code == KeyCode::Char('S')
        && !k.modifiers.intersects(ratatui::crossterm::event::KeyModifiers::CONTROL | ratatui::crossterm::event::KeyModifiers::ALT)
        && !app.modal.as_ref().is_some_and(|m| m.typing(app))
        && super::share::open(app)
    {
        return;
    }
    let Some(m) = app.modal.as_mut() else { return };
    match m {
        Modal::DateTime(_) => {}
        Modal::Import => {}
        Modal::Help(h) => {
            if h.on_key(k) {
                app.modal = None;
            }
        }
        Modal::Export => {
            if k.code == KeyCode::Esc && !app.export.typing() {
                app.modal = None;
            } else {
                forms::export_key(app, k);
            }
        }
        Modal::Keys => {
            if k.code == KeyCode::Esc && !app.keys.typing() {
                app.keys.secret_lines.clear();
                app.keys.reveal = false;
                app.modal = None;
            } else {
                forms::keys_key(app, k);
            }
        }
        Modal::Authors => super::authors::on_key(app, k),
        Modal::Searches => super::searches::on_key(app, k),
        Modal::Projects => super::projects::on_key(app, k),
        Modal::Journals => super::journals::on_key(app, k),
        Modal::Reports => super::reports::on_key(app, k),
        Modal::Rules => super::rules::on_key(app, k),
        Modal::Network => super::network::on_key(app, k),
        Modal::Orgs => super::orgs::on_key(app, k),
        Modal::Reviews => super::reviews::on_key(app, k),
        Modal::Code => super::code::on_key(app, k),
        Modal::Pipes => super::pipes::on_key(app, k),
        Modal::Models => super::models::on_key(app, k),
        Modal::DocExport => super::docexport::on_key(app, k),
        Modal::PickProject => super::workspace::on_key(app, k),
        Modal::Themes => {
            if k.code == KeyCode::Esc && app.themes.editor.is_none() {
                app.modal = None;
            } else {
                themes::on_key(app, k);
            }
        }
        Modal::Info { scroll, .. } => match k.code {
            KeyCode::Down | KeyCode::Char('j') => *scroll = scroll.saturating_add(1),
            KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
            KeyCode::PageDown => *scroll = scroll.saturating_add(10),
            KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => app.modal = None,
            _ => {}
        },
        Modal::Prefs { sel } => match k.code {
            KeyCode::Down | KeyCode::Char('j') => *sel = (*sel + 1) % PREF_ROWS,
            KeyCode::Up | KeyCode::Char('k') => *sel = (*sel + PREF_ROWS - 1) % PREF_ROWS,
            KeyCode::Enter | KeyCode::Char(' ') => {
                match *sel {
                    0 => app.prefs.tui.header = app.prefs.tui.header.next(),
                    1 => app.prefs.tui.sidebar_collapsed = !app.prefs.tui.sidebar_collapsed,
                    2 => app.prefs.ingest.drop_zero_volume = !app.prefs.ingest.drop_zero_volume,
                    _ => app.prefs.tui.history_open = !app.prefs.tui.history_open,
                }
                app.save_prefs();
            }
            KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
            _ => {}
        },
        Modal::Filter { .. } => unreachable!(),
        Modal::Layers { sel } => {
            let n = super::chart::LAYERS.len();
            match k.code {
                KeyCode::Down | KeyCode::Char('j') => *sel = (*sel + 1) % n,
                KeyCode::Up | KeyCode::Char('k') => *sel = (*sel + n - 1) % n,
                KeyCode::Enter | KeyCode::Char(' ') => {
                    let key = super::chart::LAYERS[*sel].0;
                    app.layout.focused_mut().chart.layers.toggle(key);
                    app.layout_changed();
                }
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('L') => app.modal = None,
                KeyCode::Char(c) => {
                    if let Some(i) = super::chart::LAYERS.iter().position(|(k, _)| *k == c) {
                        *sel = i;
                        app.layout.focused_mut().chart.layers.toggle(c);
                        app.layout_changed();
                    }
                }
                _ => {}
            }
        }
        Modal::Pick { items, sel, kind, .. } => match k.code {
            KeyCode::Down | KeyCode::Char('j') => *sel = (*sel + 1).min(items.len().saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => *sel = sel.saturating_sub(1),
            KeyCode::Esc => {
                let share = matches!(*kind, PickKind::Share | PickKind::Pipe);
                app.modal = if share { app.share_back.take().map(|b| *b) } else { None };
                app.share_item = None;
            }
            KeyCode::Enter => {
                let (item, kind) = (items.get(*sel).cloned(), *kind);
                app.modal = None;
                if let Some(item) = item {
                    app.on_pick(kind, &item);
                }
            }
            _ => {}
        },
        Modal::Prompt(p) => match k.code {
            KeyCode::Esc => app.modal = None,
            KeyCode::Backspace => {
                p.input.pop();
            }
            KeyCode::Char(c) => p.input.push(c),
            KeyCode::Enter => {
                let (kind, input) = (p.kind, p.input.trim().to_string());
                app.modal = None;
                app.on_prompt(kind, &input);
            }
            _ => {}
        },
    }
}

impl App {
    pub fn on_prompt(&mut self, kind: PromptKind, input: &str) {
        match kind {
            PromptKind::ThemeImport => {
                let r = std::fs::read_to_string(forms::expand(input))
                    .map_err(crate::Error::from)
                    .and_then(|s| crate::theme::ThemeFile::parse(&s))
                    .and_then(|t| crate::theme::save(&self.paths, &t).map(|_| t.meta.name.clone()));
                match r {
                    Ok(n) => {
                        self.themes = themes::ThemeState::new(&self.paths, &self.prefs.theme.name);
                        self.set_status(format!("imported theme {n}"));
                    }
                    Err(e) => self.set_status(format!("import failed: {e}")),
                }
            }
            PromptKind::ThemeExport => {
                let r = self.theme_file.to_toml().and_then(|s| std::fs::write(forms::expand(input), s).map_err(Into::into));
                match r {
                    Ok(()) => self.set_status(format!("wrote {input}")),
                    Err(e) => self.set_status(format!("export failed: {e}")),
                }
            }
            other => self.on_prompt_ext(other, input),
        }
    }

    /// Prompts handled by later features (layouts, charts).
    pub fn on_prompt_ext(&mut self, kind: PromptKind, input: &str) {
        if kind == PromptKind::FrameColumns {
            return super::frametile::set_columns(self, input);
        }
        if kind != PromptKind::LayoutSave {
            return self.chart_prompt(kind, input);
        }
        {
            if input.is_empty() {
                return self.set_status("layout name is empty");
            }
            self.prefs.tui.layouts.insert(input.to_string(), self.layout.clone());
            self.save_prefs();
            self.set_status(format!("saved layout {input}"));
        }
    }

    pub fn on_pick(&mut self, kind: PickKind, item: &str) {
        match kind {
            PickKind::Share => super::share::picked(self, item),
            PickKind::Pipe => super::code::pipe_picked(self, item),
            PickKind::SavedLayout => {
                if let Some(l) = self.prefs.tui.layouts.get(item).cloned() {
                    self.layout = l;
                    self.caches.clear();
                    self.layout_changed();
                    self.set_status(format!("layout {item}"));
                }
            }
        }
    }
}

fn put(app: &mut App, kind: super::action::FilterKind, sel: usize, edit: Option<String>) {
    if matches!(app.modal, Some(Modal::Filter { .. })) {
        app.modal = Some(Modal::Filter { kind, sel, edit });
    }
}

fn filter_key(app: &mut App, k: KeyEvent, kind: super::action::FilterKind, mut sel: usize, mut edit: Option<String>) {
    let items = filter_items(app, kind);
    let n = items.len().max(1);
    if let Some(buf) = edit.as_mut() {
        match k.code {
            KeyCode::Esc => edit = None,
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) if c.is_ascii_digit() || c == '.' || c == '-' => buf.push(c),
            KeyCode::Enter => {
                let v = buf.trim().parse::<f64>().ok();
                let which = sel;
                edit = None;
                let pf = &mut app.prefs.tui.sidebar.filters.price;
                if which == 1 {
                    pf.min = v
                } else {
                    pf.max = v
                }
                app.filters_changed();
            }
            _ => {}
        }
        put(app, kind, sel, edit);
        return;
    }
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => sel = (sel + 1) % n,
        KeyCode::Up | KeyCode::Char('k') => sel = (sel + n - 1) % n,
        KeyCode::Esc | KeyCode::Char('q') => {
            app.modal = None;
            return;
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            use super::action::FilterKind::*;
            let i = sel;
            let fl = &mut app.prefs.tui.sidebar.filters;
            match kind {
                Price => match i {
                    0 => fl.price.metric = fl.price.metric.next(),
                    1 | 2 => {
                        let cur = if i == 1 { fl.price.min } else { fl.price.max };
                        edit = Some(cur.map(|v| v.to_string()).unwrap_or_default());
                        put(app, kind, sel, edit);
                        return;
                    }
                    _ => {
                        fl.price.min = None;
                        fl.price.max = None;
                    }
                },
                Time => {}
                Source | Symbol | Period | Year => {
                    let Some(item) = items.get(i).map(|x| x.0.clone()) else { return };
                    match kind {
                        Source => toggle(&mut fl.sources, item),
                        Symbol => toggle(&mut fl.symbols, item),
                        Period => toggle(&mut fl.durations, item),
                        _ => toggle(&mut fl.years, item.parse().unwrap_or(0)),
                    }
                }
            }
            app.filters_changed();
        }
        _ => {}
    }
    put(app, kind, sel, edit);
}

fn toggle<T: Ord>(set: &mut std::collections::BTreeSet<T>, v: T) {
    if !set.remove(&v) {
        set.insert(v);
    }
}

/// Rows of the filter editor: (value, label, checked).
pub fn filter_items(app: &App, kind: super::action::FilterKind) -> Vec<(String, String, bool)> {
    use super::action::FilterKind::*;
    use super::sidebar::year_of;
    let fl = &app.prefs.tui.sidebar.filters;
    match kind {
        Source => {
            let set: std::collections::BTreeSet<String> = app.series.iter().map(|s| s.source.clone()).collect();
            set.into_iter().map(|s| (s.clone(), s.clone(), fl.sources.contains(&s))).collect()
        }
        Symbol => {
            let set: std::collections::BTreeSet<String> = app.series.iter().map(|s| s.symbol.display()).collect();
            set.into_iter().map(|s| (s.clone(), s.clone(), fl.symbols.contains(&s))).collect()
        }
        Time => vec![],
        Period => {
            let mut d: Vec<crate::model::Duration> = app.series.iter().map(|s| s.duration).collect();
            d.sort_by_key(|d| d.0);
            d.dedup();
            d.into_iter().map(|d| (d.to_string(), d.to_string(), fl.durations.contains(&d.to_string()))).collect()
        }
        Year => {
            let mut ys = std::collections::BTreeSet::new();
            for s in &app.series {
                for y in year_of(s.first)..=year_of(s.last) {
                    ys.insert(y);
                }
            }
            ys.into_iter().map(|y| (y.to_string(), y.to_string(), fl.years.contains(&y))).collect()
        }
        Price => {
            let f = |o: Option<f64>| o.map(|v| v.to_string()).unwrap_or_else(|| "–".into());
            vec![
                ("metric".into(), format!("metric  {}", fl.price.metric.label()), false),
                ("min".into(), format!("min     {}", f(fl.price.min)), false),
                ("max".into(), format!("max     {}", f(fl.price.max)), false),
                ("clear".into(), "clear price filter".into(), false),
            ]
        }
    }
}

pub fn draw(app: &mut App, f: &mut Frame, area: Rect) {
    let t = app.theme.clone();
    // the share picker sits on the window it was opened from
    if matches!(app.modal, Some(Modal::Pick { kind: PickKind::Share | PickKind::Pipe, .. })) && app.share_back.is_some() {
        let pick = app.modal.take();
        app.modal = app.share_back.take().map(|b| *b);
        draw(app, f, area);
        app.share_back = app.modal.take().map(Box::new);
        app.modal = pick;
    }
    let Some(m) = app.modal.as_ref() else { return };
    let (w, h) = match m {
        Modal::Prompt(_) => (70, 6),
        Modal::Prefs { .. } => (60, 10),
        Modal::Authors => (76, (app.authors.list.len() as u16 + 6).clamp(10, area.height)),
        Modal::PickProject => (70, (app.pick_project.list.len() as u16 + 7).clamp(10, area.height)),
        Modal::DocExport => (area.width.saturating_sub(4).min(90), (app.doc_export.formats.len() as u16 + 9).clamp(12, area.height)),
        Modal::Info { lines, .. } => ((lines.iter().map(|l| l.chars().count()).max().unwrap_or(20) as u16 + 6).max(40), lines.len() as u16 + 4),
        Modal::Pick { items, .. } => (50, items.len() as u16 + 4),
        Modal::Layers { .. } => (44, super::chart::LAYERS.len() as u16 + 4),
        Modal::Filter { kind, .. } => (46, (filter_items(app, *kind).len() as u16 + 5).min(area.height)),
        _ => {
            // a full window: cover what is behind it, so nothing shows at its edges
            f.render_widget(Clear, area);
            f.render_widget(Block::new().style(Style::new().bg(t.bg).fg(t.fg)), area);
            (area.width.saturating_sub(4), area.height.saturating_sub(2))
        }
    };
    let r = centered(area, w, h);
    f.render_widget(Clear, r);
    app.modal_hits.clear();
    app.modal_area = r;
    let mut extra: Vec<(Rect, super::Hit)> = vec![];
    match app.modal.as_mut().unwrap() {
        Modal::Help(hv) => hv.draw(f, r, &t, &mut extra),
        Modal::DateTime(p) => {
            let pr = super::picker::draw(p, f, area, &t, &mut extra);
            app.modal_hits.extend(extra);
            let x = super::widgets::close_button(f, pr, &t);
            app.modal_hits.push((x, super::Hit::CloseModal));
            return;
        }
        Modal::Export => forms::draw_export(app, f, r),
        Modal::Import => super::import::draw(app, f, r),
        Modal::Keys => forms::draw_keys(app, f, r),
        Modal::Themes => themes::draw(app, f, r),
        Modal::Authors => super::authors::draw(app, f, r, &mut extra),
        Modal::Searches => super::searches::draw(app, f, r, &mut extra),
        Modal::Projects => super::projects::draw(app, f, r, &mut extra),
        Modal::Journals => super::journals::draw(app, f, r, &mut extra),
        Modal::Reports => super::reports::draw(app, f, r, &mut extra),
        Modal::Rules => super::rules::draw(app, f, r, &mut extra),
        Modal::Network => super::network::draw(app, f, r, &mut extra),
        Modal::Orgs => super::orgs::draw(app, f, r, &mut extra),
        Modal::Reviews => super::reviews::draw(app, f, r, &mut extra),
        Modal::Code => super::code::draw(app, f, r, &mut extra),
        Modal::Pipes => super::pipes::draw(app, f, r, &mut extra),
        Modal::Models => super::models::draw(app, f, r, &mut extra),
        Modal::DocExport => super::docexport::draw(app, f, r, &mut extra),
        Modal::PickProject => super::workspace::draw(app, f, r, &mut extra),
        Modal::Info { title, lines, scroll } => {
            let body: Vec<Line> = lines.iter().map(|l| Line::raw(l.clone())).collect();
            let title = format!("{title} · Esc closes");
            f.render_widget(Paragraph::new(body).scroll((*scroll, 0)).wrap(Wrap { trim: false }).block(block(&title, true, &t)), r);
            let bar = Rect { x: r.x + 2, y: r.bottom().saturating_sub(1), width: 12.min(r.width.saturating_sub(4)), height: 1 };
            super::widgets::key_bar(f, bar, &[("Close", KeyCode::Esc)], &t, &mut extra);
        }
        Modal::Prompt(p) => {
            let title = format!("{} · Enter ok · Esc cancel", p.title);
            let line = Line::from(vec![Span::raw(p.input.clone()), Span::styled("▏", Style::new().fg(t.accent))]);
            f.render_widget(Paragraph::new(vec![Line::raw(""), line]).block(block(&title, true, &t)), r);
            let bar = Rect { x: r.x + 2, y: r.bottom().saturating_sub(2), width: r.width.saturating_sub(4), height: 1 };
            super::widgets::key_bar(f, bar, &[("OK", KeyCode::Enter), ("Cancel", KeyCode::Esc)], &t, &mut extra);
        }
        Modal::Prefs { sel } => {
            let rows = [
                format!("header           {:?}", app.prefs.tui.header).to_lowercase(),
                format!("sidebar          {}", if app.prefs.tui.sidebar_collapsed { "collapsed" } else { "shown" }),
                format!("drop zero volume {}", app.prefs.ingest.drop_zero_volume),
                format!("history bar      {}", if app.prefs.tui.history_open { "shown" } else { "collapsed" }),
            ];
            let mut lines = vec![Line::raw("")];
            for (i, row) in rows.iter().enumerate() {
                let st = if i == *sel { Style::new().bg(t.selection_bg).fg(t.selection_fg).add_modifier(Modifier::BOLD) } else { Style::new() };
                lines.push(Line::styled(format!(" {row} "), st));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(" Enter or click changes · saved to preferences.toml", Style::new().fg(t.muted)));
            f.render_widget(Paragraph::new(lines).block(block("Preferences · Esc closes", true, &t)), r);
            row_hits(&mut extra, r, 1, PREF_ROWS);
        }
        Modal::Filter { kind, sel, edit } => {
            let (kind, sel, edit) = (*kind, *sel, edit.clone());
            let items = filter_items(app, kind);
            let price = kind == super::action::FilterKind::Price;
            let mut lines = vec![];
            for (i, (_, label, on)) in items.iter().enumerate() {
                let st = if i == sel { Style::new().bg(t.selection_bg).fg(t.selection_fg) } else { Style::new().fg(t.fg) };
                let text = match (price, &edit) {
                    (true, Some(buf)) if i == sel => format!(" {}  {buf}▏", &label[..7]),
                    (true, _) => format!(" {label}"),
                    _ => format!(" [{}] {label}", if *on { "x" } else { " " }),
                };
                lines.push(Line::styled(text, st));
            }
            if items.is_empty() {
                lines.push(Line::styled(" no data yet", Style::new().fg(t.muted)));
            }
            lines.push(Line::raw(""));
            let hint = if price { " Enter: cycle metric / edit value · Esc" } else { " Space toggles · several can be on · Esc" };
            lines.push(Line::styled(hint, Style::new().fg(t.muted)));
            let title = format!("Filter: {kind:?}");
            f.render_widget(Paragraph::new(lines).block(block(&title, true, &t)), r);
            if edit.is_none() {
                row_hits(&mut extra, r, 0, items.len());
            }
        }
        Modal::Layers { sel } => {
            let layers = app.layout.focused().chart.layers;
            let mut lines = vec![];
            for (i, (k, name)) in super::chart::LAYERS.iter().enumerate() {
                let on = layers.get(*k);
                let st =
                    if i == *sel { Style::new().bg(t.selection_bg).fg(t.selection_fg) } else { Style::new().fg(if on { t.fg } else { t.muted }) };
                lines.push(Line::styled(format!(" [{}] {name:<22} {k} ", if on { "x" } else { " " }), st));
            }
            lines.push(Line::styled(" N line metric · M moving averages", Style::new().fg(t.muted)));
            f.render_widget(Paragraph::new(lines).block(block("Chart layers · Space or click toggles · Esc", true, &t)), r);
            row_hits(&mut extra, r, 0, super::chart::LAYERS.len());
        }
        Modal::Pick { title, items, sel, .. } => {
            let mut lines = vec![];
            for (i, it) in items.iter().enumerate() {
                let st = if i == *sel { Style::new().bg(t.selection_bg).fg(t.selection_fg) } else { Style::new() };
                lines.push(Line::styled(format!(" {it} "), st));
            }
            if items.is_empty() {
                lines.push(Line::styled(" (none)", Style::new().fg(t.muted)));
            }
            let title = format!("{title} · Enter or click · Esc");
            let n = items.len();
            f.render_widget(Paragraph::new(lines).block(block(&title, true, &t)), r);
            row_hits(&mut extra, r, 0, n);
        }
    }
    app.modal_hits.extend(extra);
    // Every popup gets the same close control in its title bar.
    let x = super::widgets::close_button(f, r, &t);
    app.modal_hits.push((x, super::Hit::CloseModal));
}
