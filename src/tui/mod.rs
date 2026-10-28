//! Terminal UI (ratatui + crossterm). Thin layer over the library.
pub mod forms;
pub mod nav;
pub mod themes;
pub mod widgets;

use crate::crypto::Key32;
use crate::paths::Paths;
use crate::prefs::Preferences;
use crate::store::Store;
use crate::theme::{self, Theme, ThemeFile, Variant};
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Tabs};
use ratatui::Frame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Navigator,
    Ingest,
    Export,
    Keys,
    Themes,
}

impl Screen {
    pub const ALL: [Screen; 5] = [Screen::Navigator, Screen::Ingest, Screen::Export, Screen::Keys, Screen::Themes];
    pub fn title(self) -> &'static str {
        match self {
            Screen::Navigator => "Navigator",
            Screen::Ingest => "Ingest",
            Screen::Export => "Export",
            Screen::Keys => "Keys & Backup",
            Screen::Themes => "Themes",
        }
    }
    /// Help topic for context-sensitive help.
    pub fn help_topic(self) -> &'static str {
        match self {
            Screen::Navigator => "tui",
            Screen::Ingest => "ingest-csv",
            Screen::Export => "export",
            Screen::Keys => "keys",
            Screen::Themes => "themes",
        }
    }
}

pub struct App {
    pub paths: Paths,
    pub store: Store,
    pub dek: Key32,
    pub prefs: Preferences,
    pub theme_file: ThemeFile,
    pub variant: Variant,
    pub theme: Theme,
    pub screen: Screen,
    pub status: String,
    pub quit: bool,
    pub nav: nav::NavState,
    pub ingest: forms::IngestForm,
    pub export: forms::ExportForm,
    pub keys: forms::KeysForm,
    pub themes: themes::ThemeState,
    pub help: Option<crate::help::HelpView>,
}

impl App {
    pub fn new(paths: Paths, store: Store, dek: Key32, prefs: Preferences, mode_override: Option<crate::prefs::ThemeMode>) -> crate::Result<Self> {
        theme::materialize(&paths)?;
        let (theme_file, warn) = theme::active(&paths, &prefs.theme.name);
        let variant = theme::resolve_variant(mode_override.unwrap_or(prefs.theme.mode));
        let theme = theme_file.resolved(variant);
        let mut app = App {
            nav: nav::NavState::default(),
            ingest: forms::IngestForm::default(),
            export: forms::ExportForm::default(),
            keys: forms::KeysForm::default(),
            themes: themes::ThemeState::new(&paths, &prefs.theme.name),
            help: None,
            paths,
            store,
            dek,
            prefs,
            theme_file,
            variant,
            theme,
            screen: Screen::Navigator,
            status: warn.map(|w| format!("theme error, using default: {w}")).unwrap_or_else(|| "? or F1 for help, Tab to switch screens, Ctrl-C to quit".into()),
            quit: false,
        };
        app.nav.reload(&app.store, &mut app.status);
        Ok(app)
    }

    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
    }

    pub fn apply_theme(&mut self, t: ThemeFile, variant: Variant) {
        self.theme = t.resolved(variant);
        self.theme_file = t;
        self.variant = variant;
    }

    /// Whether a text field currently has focus (so letters are typed, not commands).
    fn typing(&self) -> bool {
        if let Some(h) = &self.help {
            return h.searching;
        }
        match self.screen {
            Screen::Navigator => false,
            Screen::Ingest => self.ingest.typing(),
            Screen::Export => self.export.typing(),
            Screen::Keys => self.keys.typing(),
            Screen::Themes => self.themes.typing(),
        }
    }

    pub fn open_help(&mut self, topic: &str) {
        self.help = Some(crate::help::HelpView::open(topic));
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        if k.kind != KeyEventKind::Press {
            return;
        }
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        if let Some(h) = &mut self.help {
            if h.on_key(k) {
                self.help = None;
            }
            return;
        }
        if k.code == KeyCode::F(1) || (k.code == KeyCode::Char('?') && !self.typing()) {
            let topic = if self.screen == Screen::Themes && self.themes.editing() { "themes" } else { self.screen.help_topic() };
            self.open_help(topic);
            return;
        }
        match k.code {
            KeyCode::Tab => {
                let i = Screen::ALL.iter().position(|s| *s == self.screen).unwrap_or(0);
                self.screen = Screen::ALL[(i + 1) % Screen::ALL.len()];
                return;
            }
            KeyCode::BackTab => {
                let i = Screen::ALL.iter().position(|s| *s == self.screen).unwrap_or(0);
                self.screen = Screen::ALL[(i + Screen::ALL.len() - 1) % Screen::ALL.len()];
                return;
            }
            KeyCode::Char('q') if !self.typing() => {
                self.quit = true;
                return;
            }
            KeyCode::Char(c @ '1'..='6') if !self.typing() => {
                let i = c as usize - '1' as usize;
                if i < Screen::ALL.len() {
                    self.screen = Screen::ALL[i];
                } else {
                    self.open_help("help");
                }
                return;
            }
            _ => {}
        }
        match self.screen {
            Screen::Navigator => nav::on_key(self, k),
            Screen::Ingest => forms::ingest_key(self, k),
            Screen::Export => forms::export_key(self, k),
            Screen::Keys => forms::keys_key(self, k),
            Screen::Themes => themes::on_key(self, k),
        }
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        let t = self.theme.clone();
        f.render_widget(Block::new().style(Style::new().bg(t.bg).fg(t.fg)), area);
        let [top, body, bottom] = Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).areas(area);
        let mut titles: Vec<Line> = Screen::ALL.iter().enumerate().map(|(i, s)| Line::from(format!("{} {}", i + 1, s.title()))).collect();
        titles.push(Line::from("6 Help"));
        let sel = Screen::ALL.iter().position(|s| *s == self.screen).unwrap_or(0);
        let tabs = Tabs::new(titles)
            .select(if self.help.is_some() { Screen::ALL.len() } else { sel })
            .style(Style::new().fg(t.muted).bg(t.bg))
            .highlight_style(Style::new().fg(t.accent).add_modifier(Modifier::BOLD))
            .divider(Span::styled("│", Style::new().fg(t.border)));
        f.render_widget(tabs, top);
        if let Some(h) = &mut self.help {
            h.draw(f, body, &t);
        } else {
            match self.screen {
                Screen::Navigator => nav::draw(self, f, body),
                Screen::Ingest => forms::draw_ingest(self, f, body),
                Screen::Export => forms::draw_export(self, f, body),
                Screen::Keys => forms::draw_keys(self, f, body),
                Screen::Themes => themes::draw(self, f, body),
            }
        }
        let status = Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} ", self.theme_file.display_name()), Style::new().fg(t.bg).bg(t.accent)),
            Span::styled(format!(" {} ", self.variant.name()), Style::new().fg(t.bg).bg(t.info)),
            Span::styled(format!(" {}", self.status), Style::new().fg(t.muted)),
        ]));
        f.render_widget(status, bottom);
    }
}

/// Bordered block in theme colors.
pub fn block<'a>(title: &'a str, focused: bool, t: &Theme) -> Block<'a> {
    Block::bordered()
        .title(Span::styled(format!(" {title} "), Style::new().fg(t.header).add_modifier(Modifier::BOLD)))
        .border_style(Style::new().fg(if focused { t.border_focus } else { t.border }))
        .style(Style::new().bg(t.bg).fg(t.fg))
}

pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

/// Run the TUI until quit.
pub fn run(mut app: App) -> crate::Result<()> {
    let mut terminal = ratatui::init();
    let res = (|| -> crate::Result<()> {
        while !app.quit {
            terminal.draw(|f| app.draw(f))?;
            if event::poll(std::time::Duration::from_millis(250))? {
                match event::read()? {
                    Event::Key(k) => app.on_key(k),
                    Event::FocusGained if app.prefs.theme.mode == crate::prefs::ThemeMode::System => {
                        let v = theme::resolve_variant(crate::prefs::ThemeMode::System);
                        let tf = app.theme_file.clone();
                        app.apply_theme(tf, v);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    })();
    ratatui::restore();
    res
}

#[cfg(test)]
mod tests;
