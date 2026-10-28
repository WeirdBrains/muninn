//! Modal dialogs drawn over the content area.
use super::{block, centered, forms, themes, App};
use crate::help::HelpView;
use crate::ingest::time::fmt_ts;
use crate::store::Store;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    ThemeImport,
    ThemeExport,
    LayoutSave,
    JumpDate,
    ChartStart,
    ChartEnd,
}

pub struct Prompt {
    pub title: String,
    pub input: String,
    pub kind: PromptKind,
}

pub enum Modal {
    Help(HelpView),
    Export,
    Keys,
    Themes,
    Prefs { sel: usize },
    Info { title: String, lines: Vec<String>, scroll: u16 },
    Prompt(Prompt),
    /// Pick from a list of strings (e.g. saved layouts).
    Pick { title: String, items: Vec<String>, sel: usize, kind: PickKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickKind {
    SavedLayout,
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
            Modal::Keys => "keys",
            Modal::Themes => "themes",
            Modal::Prefs { .. } => "preferences",
            Modal::Info { .. } | Modal::Pick { .. } => "tui",
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
            Modal::Keys => app.keys.typing(),
            Modal::Themes => app.themes.typing(),
            Modal::Prompt(_) => true,
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
            let mut out = vec![format!("{:<12} {:<12} {:>4} {:>9} {:>8}  {:<19}  {:<19}", "source", "symbol", "dur", "bars", "missing", "first", "last")];
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

const PREF_ROWS: usize = 3;

pub fn on_key(app: &mut App, k: KeyEvent) {
    let Some(m) = app.modal.as_mut() else { return };
    match m {
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
                    _ => app.prefs.ingest.drop_zero_volume = !app.prefs.ingest.drop_zero_volume,
                }
                app.save_prefs();
            }
            KeyCode::Esc | KeyCode::Char('q') => app.modal = None,
            _ => {}
        },
        Modal::Pick { items, sel, kind, .. } => match k.code {
            KeyCode::Down | KeyCode::Char('j') => *sel = (*sel + 1).min(items.len().saturating_sub(1)),
            KeyCode::Up | KeyCode::Char('k') => *sel = sel.saturating_sub(1),
            KeyCode::Esc => app.modal = None,
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
        if kind == PromptKind::LayoutSave {
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

pub fn draw(app: &mut App, f: &mut Frame, area: Rect) {
    let t = app.theme.clone();
    let Some(m) = app.modal.as_ref() else { return };
    let (w, h) = match m {
        Modal::Prompt(_) => (70, 5),
        Modal::Prefs { .. } => (60, 9),
        Modal::Info { lines, .. } => ((lines.iter().map(|l| l.chars().count()).max().unwrap_or(20) as u16 + 6).max(40), lines.len() as u16 + 4),
        Modal::Pick { items, .. } => (50, items.len() as u16 + 4),
        _ => (area.width.saturating_sub(4), area.height.saturating_sub(2)),
    };
    let r = centered(area, w, h);
    f.render_widget(Clear, r);
    match app.modal.as_mut().unwrap() {
        Modal::Help(hv) => hv.draw(f, r, &t),
        Modal::Export => forms::draw_export(app, f, r),
        Modal::Keys => forms::draw_keys(app, f, r),
        Modal::Themes => themes::draw(app, f, r),
        Modal::Info { title, lines, scroll } => {
            let body: Vec<Line> = lines.iter().map(|l| Line::raw(l.clone())).collect();
            let title = format!("{title} · Esc closes");
            f.render_widget(Paragraph::new(body).scroll((*scroll, 0)).wrap(Wrap { trim: false }).block(block(&title, true, &t)), r);
        }
        Modal::Prompt(p) => {
            let title = format!("{} · Enter ok · Esc cancel", p.title);
            let line = Line::from(vec![Span::raw(p.input.clone()), Span::styled("▏", Style::new().fg(t.accent))]);
            f.render_widget(Paragraph::new(vec![Line::raw(""), line]).block(block(&title, true, &t)), r);
        }
        Modal::Prefs { sel } => {
            let rows = [
                format!("header           {:?}", app.prefs.tui.header).to_lowercase(),
                format!("sidebar          {}", if app.prefs.tui.sidebar_collapsed { "collapsed" } else { "shown" }),
                format!("drop zero volume {}", app.prefs.ingest.drop_zero_volume),
            ];
            let mut lines = vec![Line::raw("")];
            for (i, row) in rows.iter().enumerate() {
                let st = if i == *sel { Style::new().bg(t.selection_bg).fg(t.selection_fg).add_modifier(Modifier::BOLD) } else { Style::new() };
                lines.push(Line::styled(format!(" {row} "), st));
            }
            lines.push(Line::raw(""));
            lines.push(Line::styled(" Enter changes · saved to preferences.toml", Style::new().fg(t.muted)));
            f.render_widget(Paragraph::new(lines).block(block("Preferences · Esc closes", true, &t)), r);
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
            let title = format!("{title} · Enter · Esc");
            f.render_widget(Paragraph::new(lines).block(block(&title, true, &t)), r);
        }
    }
}
