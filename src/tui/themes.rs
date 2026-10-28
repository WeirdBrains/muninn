//! Theme selector (live preview of both variants, mode toggle) and editor.
use super::{block, widgets::{sample_bars, Candles}, App};
use crate::paths::Paths;
use crate::prefs::ThemeMode;
use crate::theme::{self, contrast_warnings, parse_hex, Theme, ThemeFile, Variant, ROLES};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, List, ListItem, ListState, Paragraph, Row, Table};
use ratatui::Frame;

pub struct Editor {
    pub file: ThemeFile,
    pub variant: Variant,
    pub role: usize,
    pub hex_input: Option<String>,
    pub save_name: Option<String>,
    pub dirty: bool,
}

pub struct ThemeState {
    pub names: Vec<String>,
    pub list: ListState,
    pub editor: Option<Editor>,
}

impl ThemeState {
    pub fn new(paths: &Paths, active: &str) -> Self {
        let names = theme::list(paths).unwrap_or_default();
        let mut list = ListState::default();
        list.select(Some(names.iter().position(|n| n == active).unwrap_or(0)));
        Self { names, list, editor: None }
    }
    pub fn typing(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| e.hex_input.is_some() || e.save_name.is_some())
    }
    pub fn editing(&self) -> bool {
        self.editor.is_some()
    }
    pub fn selected(&self) -> Option<&str> {
        self.list.selected().and_then(|i| self.names.get(i)).map(String::as_str)
    }
}

fn save_prefs(app: &mut App) {
    if let Err(e) = app.prefs.save(&app.paths) {
        app.set_status(format!("could not save preferences: {e}"));
    }
}

pub fn on_key(app: &mut App, k: KeyEvent) {
    if app.themes.editor.is_some() {
        return editor_key(app, k);
    }
    let st = &mut app.themes;
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => {
            let i = (st.list.selected().unwrap_or(0) + 1).min(st.names.len().saturating_sub(1));
            st.list.select(Some(i));
        }
        KeyCode::Up | KeyCode::Char('k') => {
            let i = st.list.selected().unwrap_or(0).saturating_sub(1);
            st.list.select(Some(i));
        }
        KeyCode::Enter => {
            let Some(name) = st.selected().map(str::to_string) else { return };
            match theme::load(&app.paths, &name) {
                Ok(t) => {
                    app.prefs.theme.name = name.clone();
                    save_prefs(app);
                    let v = app.variant;
                    app.apply_theme(t, v);
                    app.set_status(format!("theme set to {name}"));
                }
                Err(e) => app.set_status(format!("theme error: {e}")),
            }
        }
        KeyCode::Char('m') => {
            app.prefs.theme.mode = match app.prefs.theme.mode {
                ThemeMode::System => ThemeMode::Dark,
                ThemeMode::Dark => ThemeMode::Light,
                ThemeMode::Light => ThemeMode::System,
            };
            save_prefs(app);
            let v = theme::resolve_variant(app.prefs.theme.mode);
            let t = app.theme_file.clone();
            app.apply_theme(t, v);
            app.set_status(format!("mode {} (showing {})", app.prefs.theme.mode, v.name()));
        }
        KeyCode::Char('e') => {
            let Some(name) = st.selected().map(str::to_string) else { return };
            match theme::load(&app.paths, &name) {
                Ok(file) => {
                    app.themes.editor = Some(Editor { file, variant: app.variant, role: 0, hex_input: None, save_name: None, dirty: false });
                    app.set_status("editor: ↑↓ role · ←→ palette · Enter hex · v variant · g generate other variant · s save as · R reset · Esc back");
                }
                Err(e) => app.set_status(format!("theme error: {e}")),
            }
        }
        KeyCode::Char('R') => {
            let Some(name) = st.selected().map(str::to_string) else { return };
            match theme::reset(&app.paths, &name) {
                Ok(()) => app.set_status(format!("{name} reset to the built-in version")),
                Err(e) => app.set_status(format!("{e}")),
            }
        }
        _ => {}
    }
}

fn editor_key(app: &mut App, k: KeyEvent) {
    let paths = app.paths.clone();
    let Some(ed) = app.themes.editor.as_mut() else { return };
    if let Some(buf) = ed.hex_input.as_mut() {
        match k.code {
            KeyCode::Esc => ed.hex_input = None,
            KeyCode::Enter => {
                let v = if buf.starts_with('#') { buf.clone() } else { format!("#{buf}") };
                if parse_hex(&v).is_some() {
                    let role = ROLES[ed.role];
                    ed.file.variant_mut(ed.variant).roles.insert(role.into(), v.to_ascii_lowercase());
                    ed.dirty = true;
                    ed.hex_input = None;
                } else {
                    app.status = format!("`{v}` is not #rrggbb");
                }
            }
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) if c.is_ascii_hexdigit() || c == '#' => {
                if buf.len() < 7 {
                    buf.push(c);
                }
            }
            _ => {}
        }
        return;
    }
    if let Some(name) = ed.save_name.as_mut() {
        match k.code {
            KeyCode::Esc => ed.save_name = None,
            KeyCode::Backspace => {
                name.pop();
            }
            KeyCode::Char(c) => name.push(c),
            KeyCode::Enter => {
                let n = name.trim().to_string();
                let mut f = ed.file.clone();
                if f.meta.base.is_none() {
                    f.meta.base = Some(f.meta.name.clone());
                }
                f.meta.name = n.clone();
                f.meta.display_name = Some(n.clone());
                match theme::save(&paths, &f) {
                    Ok(()) => {
                        ed.file = f;
                        ed.save_name = None;
                        ed.dirty = false;
                        app.themes.names = theme::list(&paths).unwrap_or_default();
                        let i = app.themes.names.iter().position(|x| *x == n);
                        app.themes.list.select(i);
                        app.status = format!("saved theme {n}");
                    }
                    Err(e) => app.status = format!("save failed: {e}"),
                }
            }
            _ => {}
        }
        return;
    }
    match k.code {
        KeyCode::Esc => {
            let dirty = ed.dirty;
            app.themes.editor = None;
            app.status = if dirty { "left editor; unsaved changes discarded".into() } else { "left editor".into() };
        }
        KeyCode::Up | KeyCode::Char('k') => ed.role = ed.role.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => ed.role = (ed.role + 1).min(ROLES.len() - 1),
        KeyCode::Char('v') => ed.variant = ed.variant.toggle(),
        KeyCode::Enter => ed.hex_input = Some(String::from("#")),
        KeyCode::Left | KeyCode::Right => {
            let var = ed.file.variant(ed.variant);
            let sw = var.swatches();
            if sw.is_empty() {
                return;
            }
            let role = ROLES[ed.role];
            let cur = var.roles.get(role).cloned().unwrap_or_default();
            let pos = sw.iter().position(|(n, h)| *n == cur || h.eq_ignore_ascii_case(&cur)).unwrap_or(0);
            let next = if k.code == KeyCode::Right { (pos + 1) % sw.len() } else { (pos + sw.len() - 1) % sw.len() };
            let val = sw[next].0.clone();
            ed.file.variant_mut(ed.variant).roles.insert(role.into(), val);
            ed.dirty = true;
        }
        KeyCode::Char('g') => {
            let target = ed.variant.toggle();
            ed.file.generate_variant(target);
            ed.dirty = true;
            app.status = format!("generated a starting {} variant from {}", target.name(), ed.variant.name());
        }
        KeyCode::Char('s') => ed.save_name = Some(format!("{}-custom", ed.file.meta.name)),
        KeyCode::Char('R') => {
            let base = ed.file.meta.base.clone().unwrap_or_default().to_ascii_lowercase().replace(' ', "-");
            let src = theme::builtin(&ed.file.meta.name).or_else(|| theme::builtin(&base));
            match src {
                Some(b) => {
                    let name = ed.file.meta.name.clone();
                    ed.file = b;
                    ed.file.meta.name = name;
                    ed.dirty = true;
                    app.status = "reset to base theme (not saved yet)".into();
                }
                None => app.status = "no built-in base to reset to".into(),
            }
        }
        _ => {}
    }
}

/// A miniature navigator rendered with `t`.
fn preview(f: &mut Frame, area: Rect, title: &str, t: &Theme) {
    let b = block(title, true, t);
    let inner = b.inner(area);
    f.render_widget(b, area);
    let [list, chart, table] = Layout::vertical([Constraint::Length(4), Constraint::Length(8), Constraint::Min(0)]).areas(inner);
    let items = vec![
        ListItem::new("binance   BTC/USDT   5m").style(Style::new().bg(t.selection_bg).fg(t.selection_fg)),
        ListItem::new("binance   ETH/USDT   5m").style(Style::new().fg(t.fg)),
        ListItem::new("coinbase  BTC/USD    1h").style(Style::new().fg(t.muted)),
    ];
    f.render_widget(List::new(items).style(Style::new().bg(t.bg)), list);
    let bars = sample_bars(chart.width as usize);
    f.render_widget(Candles { bars: &bars, theme: t }, chart);
    let rows = vec![
        Row::new(vec!["04:00", "4261.48", "4280.56", "▲"]).style(Style::new().fg(t.up)),
        Row::new(vec!["04:05", "4280.56", "4250.00", "▼"]).style(Style::new().fg(t.down)),
        Row::new(vec![
            Cell::from(Span::styled("warn", Style::new().fg(t.warning))),
            Cell::from(Span::styled("error", Style::new().fg(t.error))),
            Cell::from(Span::styled("ok", Style::new().fg(t.success))),
            Cell::from(Span::styled("info", Style::new().fg(t.info))),
        ]),
        Row::new(t.charts.iter().take(4).map(|c| Span::styled("■■■", Style::new().fg(*c)))),
    ];
    f.render_widget(
        Table::new(rows, [Constraint::Length(7), Constraint::Length(9), Constraint::Length(9), Constraint::Min(3)])
            .header(Row::new(vec!["time", "open", "close", ""]).style(Style::new().fg(t.header).add_modifier(Modifier::BOLD)))
            .style(Style::new().bg(t.bg).fg(t.fg)),
        table,
    );
}

pub fn draw(app: &mut App, f: &mut Frame, area: Rect) {
    let t = app.theme.clone();
    let [left, right] = Layout::horizontal([Constraint::Length(30), Constraint::Min(0)]).areas(area);
    if app.themes.editor.is_some() {
        return draw_editor(app, f, left, right);
    }
    let items: Vec<ListItem> = app
        .themes
        .names
        .iter()
        .map(|n| {
            let mark = if *n == app.prefs.theme.name { "● " } else { "  " };
            ListItem::new(format!("{mark}{n}"))
        })
        .collect();
    let title = format!("Themes · mode {} (m)", app.prefs.theme.mode);
    let list = List::new(items)
        .block(block(&title, true, &t))
        .highlight_style(Style::new().bg(t.selection_bg).fg(t.selection_fg).add_modifier(Modifier::BOLD));
    f.render_stateful_widget(list, left, &mut app.themes.list);
    let Some(name) = app.themes.selected().map(str::to_string) else { return };
    match theme::load(&app.paths, &name) {
        Ok(tf) => {
            let [d, l] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(right);
            let dn = format!("{} · dark{}", tf.display_name(), if app.variant == Variant::Dark { " (active)" } else { "" });
            let ln = format!("{} · light{}", tf.display_name(), if app.variant == Variant::Light { " (active)" } else { "" });
            preview(f, d, &dn, &tf.resolved(Variant::Dark));
            preview(f, l, &ln, &tf.resolved(Variant::Light));
        }
        Err(e) => f.render_widget(Paragraph::new(format!("{e}")).style(Style::new().fg(t.error)), right),
    }
}

fn draw_editor(app: &mut App, f: &mut Frame, left: Rect, right: Rect) {
    let t = app.theme.clone();
    let Some(ed) = app.themes.editor.as_ref() else { return };
    let var = ed.file.variant(ed.variant);
    let mut lines: Vec<Line> = vec![Line::styled(
        format!("{} · {}{}", ed.file.meta.name, ed.variant.name(), if ed.dirty { " *" } else { "" }),
        Style::new().fg(t.header).add_modifier(Modifier::BOLD),
    )];
    for (i, r) in ROLES.iter().enumerate() {
        let val = var.roles.get(*r).cloned().unwrap_or_default();
        let rgb = var.resolve(r).unwrap_or((0, 0, 0));
        let sel = i == ed.role;
        lines.push(Line::from(vec![
            Span::styled("██ ", Style::new().fg(ratatui::style::Color::Rgb(rgb.0, rgb.1, rgb.2))),
            Span::styled(
                format!("{r:<13}{val}"),
                if sel { Style::new().bg(t.selection_bg).fg(t.selection_fg) } else { Style::new().fg(t.fg) },
            ),
        ]));
    }
    if let Some(h) = &ed.hex_input {
        lines.push(Line::styled(format!("hex: {h}▏"), Style::new().fg(t.accent)));
    }
    if let Some(n) = &ed.save_name {
        lines.push(Line::styled(format!("save as: {n}▏"), Style::new().fg(t.accent)));
    }
    for w in contrast_warnings(var) {
        lines.push(Line::styled(format!("⚠ {w}"), Style::new().fg(t.warning)));
    }
    f.render_widget(Paragraph::new(lines).block(block("Editor", true, &t)), left);
    let preview_theme = Theme::from_variant(var);
    preview(f, right, &format!("Live preview · {}", ed.variant.name()), &preview_theme);
}
