//! Terminal UI (ratatui + crossterm). Thin layer over the library.
//!
//! Layout (docs/historical/1.0/PLAN.md 1.3): header, menu bar, content area, status line. Menus, keys and
//! help all dispatch the same [`action::Action`]s through [`App::dispatch`].
pub mod action;
pub mod authors;
pub mod chart;
mod charting;
mod code;
pub mod data;
pub mod designer;
pub mod docexport;
pub mod forms;
pub mod frametile;
pub mod header;
pub mod history;
pub mod import;
pub mod journals;
pub mod layout;
pub mod menu;
pub mod modal;
pub mod models;
pub mod network;
pub mod orgs;
pub mod picker;
pub mod pipes;
pub mod projects;
pub mod reports;
pub mod reviews;
pub mod rules;
pub mod searches;
mod share;
pub mod sidebar;
pub mod themes;
pub mod tiles;
pub mod widgets;
pub mod workspace;

use crate::crypto::Key32;
use crate::paths::Paths;
use crate::prefs::Preferences;
use crate::store::Store;
use crate::theme::{self, Theme, ThemeFile, Variant};
use action::Action;
use modal::Modal;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

/// Clickable regions recorded while drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    MenuTop(usize),
    MenuItem(usize, usize),
    Tile(u32),
    ListHeader(u32, usize),
    TabBar(u32),
    /// Chart period selector arrows: 0 start◂, 1 start▸, 2 end◂, 3 end▸.
    ChartNav(u32, u8),
    ChartPlot(u32),
    Sidebar,
    SidebarSearch,
    SidebarSortKey,
    SidebarSortOrder,
    SidebarFilter(action::FilterKind),
    SidebarRow(usize),
    Chip(usize),
    ClearFilters,
    /// The [X] of the open popup.
    CloseModal,
    ImportCancel,
    ImportConfirm(bool),
    ImportPlace(usize),
    ImportCandidate(usize),
    /// Symbol popup buttons: 0 OK, 1 OK for all, 2 skip file, 3 cancel / [X].
    ImportAsk(u8),
    /// A file/folder row in the import picker (index into the listing).
    ImportEntry(usize),
    /// Import inputs: 0 path, 1 filter, 2.. the source/symbol/duration fields.
    ImportInput(u8),
    /// The n-th internal link of the open help topic.
    HelpLink(usize),
    /// The n-th URL drawn this frame (index into `App::urls`).
    Url(usize),
    /// Share the main view.
    ShareView,
    /// A row of a popup list (Preferences, Layers, Pick, Filter, Themes, Help topics).
    Row(usize),
    /// A field or action button of a form (Export, Keys): index as in `Form::sel`.
    FormRow(usize),
    /// Date/time picker controls.
    Picker(picker::PickHit),
    /// List tile range labels: 0 from, 1 to.
    ListRange(u32, u8),
    /// Chart period dates (open the picker): 0 start, 1 end.
    ChartDate(u32, u8),
    /// A clickable key hint: acts as if that key was pressed.
    Press(ratatui::crossterm::event::KeyCode),
    /// A key named in hint text (with Ctrl or Alt): acts as if it was pressed.
    Chord(ratatui::crossterm::event::KeyCode, ratatui::crossterm::event::KeyModifiers),
    /// A bar row of a list tile (absolute row index).
    ListRow(u32, i64),
    /// The history bar (focus), its `«` collapse button, and a row (0: the start).
    HistoryBar,
    HistoryCollapse,
    HistoryRow(usize),
}

pub struct App {
    pub paths: Paths,
    pub store: Store,
    pub dek: Key32,
    pub prefs: Preferences,
    pub theme_file: ThemeFile,
    pub variant: Variant,
    pub theme: Theme,
    pub status: String,
    pub quit: bool,
    pub menu: menu::MenuState,
    pub modal: Option<Modal>,
    pub layout: layout::TileLayout,
    pub caches: std::collections::HashMap<u32, tiles::TileCache>,
    pub series: Vec<crate::store::SeriesInfo>,
    pub sidebar: sidebar::Sidebar,
    pub import: import::ImportState,
    pub data: data::DataCache,
    /// Click targets inside the open popup (checked before anything else).
    pub modal_hits: Vec<(Rect, Hit)>,
    /// The item being shared and the window to go back to.
    pub share_item: Option<(crate::store::ShareKind, String)>,
    pub share_back: Option<Box<modal::Modal>>,
    /// Where the open popup is drawn (its hint text is clickable).
    pub modal_area: Rect,
    /// `Ctrl-w` was pressed; the next key is a window command.
    pub pending_w: bool,
    /// Content area from the last draw (split geometry, min sizes).
    pub content: Rect,
    pub export: forms::ExportForm,
    pub keys: forms::KeysForm,
    pub themes: themes::ThemeState,
    /// Regions from the last draw, for mouse hit-testing.
    pub hits: Vec<(Rect, Hit)>,
    /// URLs drawn this frame, with where they are (for clicks and OSC 8 links).
    pub urls: Vec<(Rect, String)>,
    /// The URL sets already written as OSC 8 links (repainted when they change).
    pub painted_urls: Vec<(Rect, String)>,
    /// The last URL a click opened.
    pub opened: Option<String>,
    /// Last dispatched action (tests and the status line).
    pub last_action: Option<Action>,
    pub authors: authors::AuthorsState,
    pub history: history::HistoryPane,
    pub searches: searches::SearchesState,
    pub rules: rules::RulesState,
    pub network: network::NetState,
    pub orgs: orgs::OrgsState,
    pub reviews: reviews::ReviewsState,
    pub code: code::CodeState,
    pub pipes: pipes::PipesState,
    pub frames: frametile::FramesState,
    pub designer: designer::DesignerState,
    pub models: models::ModelsState,
    pub projects: projects::ProjectsState,
    pub journals: journals::JournalsState,
    pub reports: reports::ReportsState,
    pub doc_export: docexport::DocExport,
    /// P in Journals only saves the PDF (tests; never sends to a printer).
    pub save_prints_only: bool,
    pub pick_project: workspace::PickProject,
    /// View state as of the last recorded action; a change from it is the next action.
    pub view_base: crate::history::ViewState,
}

impl App {
    pub fn new(paths: Paths, store: Store, dek: Key32, prefs: Preferences, mode_override: Option<crate::prefs::ThemeMode>) -> crate::Result<Self> {
        theme::materialize(&paths)?;
        let (theme_file, warn) = theme::active(&paths, &prefs.theme.name);
        let variant = theme::resolve_variant(mode_override.unwrap_or(prefs.theme.mode));
        let theme = theme_file.resolved(variant);
        let mut app = App {
            layout: layout::TileLayout::default(),
            caches: Default::default(),
            series: vec![],
            pending_w: false,
            sidebar: Default::default(),
            import: Default::default(),
            data: data::DataCache::new(paths.db.clone(), dek.0),
            modal_hits: vec![],
            share_item: None,
            share_back: None,
            modal_area: Rect::default(),
            content: Rect::new(0, 0, 120, 30),
            export: forms::ExportForm::default(),
            keys: forms::KeysForm::default(),
            themes: themes::ThemeState::new(&paths, &prefs.theme.name),
            menu: menu::MenuState::default(),
            modal: None,
            hits: vec![],
            urls: vec![],
            painted_urls: vec![],
            opened: None,
            last_action: None,
            authors: Default::default(),
            history: Default::default(),
            searches: Default::default(),
            rules: Default::default(),
            network: Default::default(),
            orgs: Default::default(),
            reviews: Default::default(),
            code: Default::default(),
            pipes: Default::default(),
            frames: Default::default(),
            designer: Default::default(),
            models: Default::default(),
            projects: Default::default(),
            journals: Default::default(),
            reports: Default::default(),
            doc_export: Default::default(),
            save_prints_only: false,
            pick_project: Default::default(),
            view_base: crate::history::ViewState::capture(&prefs),
            paths,
            store,
            dek,
            prefs,
            theme_file,
            variant,
            theme,
            status: warn
                .map(|w| format!("theme error, using default: {w}"))
                .unwrap_or_else(|| "F10 menu · ? help · Tab/S-Tab tiles · Alt-1..9 tile · q quit".into()),
            quit: false,
        };
        app.series = app.store.series().unwrap_or_default();
        let first = app.series.first().map(layout::SeriesKey::of);
        app.layout = match app.prefs.tui.layout.clone() {
            Some(l) if !l.tiles().is_empty() => l,
            _ => layout::TileLayout::single(first),
        };
        app.refresh_rows();
        rules::poll(&mut app);
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

    pub fn save_prefs(&mut self) {
        if let Err(e) = self.prefs.save(&self.paths) {
            self.set_status(format!("could not save preferences: {e}"));
        }
    }

    pub fn open_help(&mut self, topic: &str) {
        self.modal = Some(Modal::Help(crate::help::HelpView::open(topic)));
    }

    /// Help topic for the current context.
    pub fn help_topic(&self) -> &'static str {
        match &self.modal {
            Some(m) => m.help_topic(),
            None if self.menu.is_open() => "menus",
            None if self.history.focused => "history",
            None if self.sidebar.focused => "sidebar",
            None if self.layout.focused().view == layout::View::Chart => "charts",
            None => "tiling",
        }
    }

    /// Whether an action is available right now (disabled items are greyed out).
    pub fn enabled(&self, a: Action) -> bool {
        use Action::*;
        if matches!(a, KeysInit | RecoverWords | RecoverShards) {
            return false;
        }
        // Not built yet in this release.
        true
    }

    /// Whether a text field has focus (letters are typed, not commands).
    fn typing(&self) -> bool {
        self.sidebar.searching || self.modal.as_ref().is_some_and(|m| m.typing(self))
    }

    /// Map a key to a global action.
    pub fn key_action(&self, k: KeyEvent) -> Option<Action> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        match k.code {
            KeyCode::F(10) => Some(Action::OpenMenu),
            KeyCode::F(1) => Some(Action::Help),
            KeyCode::F(5) => Some(Action::Refresh),
            KeyCode::Tab if self.modal.is_none() && !self.sidebar.focused => Some(Action::FocusNext),
            KeyCode::BackTab if self.modal.is_none() && !self.sidebar.focused => Some(Action::FocusPrev),
            KeyCode::Char(c @ '1'..='9') if alt && self.modal.is_none() => Some(Action::FocusTile(c as u32 - '0' as u32)),
            KeyCode::Char('b') if ctrl => Some(Action::ToggleSidebar),
            KeyCode::Char('/') if !self.typing() && self.modal.is_none() => Some(Action::FocusSidebar),
            KeyCode::Char('o') if !ctrl && !self.typing() && self.modal.is_none() => Some(Action::ToggleOrder),
            KeyCode::Char('X') if !self.typing() && self.modal.is_none() => Some(Action::ClearFilters),
            KeyCode::Char('i') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Import),
            KeyCode::Char('A') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Authors),
            KeyCode::Char('P') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Projects),
            KeyCode::Char('F') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::LiveSearches),
            KeyCode::Char('B') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::SaveSearch),
            KeyCode::Char('J') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Journals),
            KeyCode::Char('O') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Reports),
            KeyCode::Char('R') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Rules),
            KeyCode::Char('K') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::NetStatus),
            KeyCode::Char('G') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Orgs),
            KeyCode::Char('V') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Reviews),
            KeyCode::Char('C') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Code),
            KeyCode::Char('D') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Models),
            KeyCode::Char('Y') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::ToggleHistory),
            KeyCode::Char('u') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::HistoryBack),
            KeyCode::Char('U') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::HistoryForward),
            KeyCode::Char('T') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Filter(action::FilterKind::Time)),
            KeyCode::Char('?') if !self.typing() => Some(Action::Help),
            KeyCode::Char('q') if !ctrl && !alt && !self.typing() && self.modal.is_none() => Some(Action::Quit),
            _ => None,
        }
    }

    /// Run a named action. Menus, keys and help all end up here.
    pub fn dispatch(&mut self, a: Action) {
        self.last_action = Some(a);
        if !self.enabled(a) {
            self.set_status(format!("{} is not available here", a.name()));
            return;
        }
        use Action::*;
        match a {
            Quit => self.quit = true,
            OpenMenu => self.menu.open_top(0),
            Help => {
                let t = self.help_topic();
                self.open_help(t)
            }
            HelpSearch => {
                let mut h = crate::help::HelpView::open("help");
                h.searching = true;
                h.update_search();
                self.modal = Some(Modal::Help(h));
            }
            Keybindings => self.open_help("keybindings"),
            About => self.modal = Some(Modal::info("About Muninn", modal::about_lines())),
            Export => self.modal = Some(Modal::Export),
            BackupCreate | RotateKey | ReissueShards => {
                self.keys.form.sel = match a {
                    BackupCreate => 0,
                    RotateKey => self.keys.form.fields.len() + 1,
                    _ => self.keys.form.fields.len() + 2,
                };
                self.modal = Some(Modal::Keys);
            }
            BackupRestore => {
                self.modal = Some(Modal::info(
                    "Restore",
                    modal::cli_hint(
                        "backup restore --from <archive or dir>",
                        "Restoring replaces the open database, so it runs from the command line with the TUI closed.",
                    ),
                ))
            }
            Rekey => {
                self.modal = Some(Modal::info(
                    "Rekey",
                    modal::cli_hint("keys rekey --words-file <words>", "Rekeying rewrites the whole database; run it with the TUI closed."),
                ))
            }
            KeysExport => {
                self.modal = Some(Modal::info(
                    "Export keys",
                    modal::cli_hint(
                        "keys export --out <dir> --words-file <words>",
                        "Exports age-encrypted key files; the words never pass through the TUI.",
                    ),
                ))
            }
            KeysInit | RecoverWords | RecoverShards => {}
            Lock => {
                self.set_status("locked");
                self.quit = true;
            }
            Preferences => self.modal = Some(Modal::Prefs { sel: 0 }),
            Authors => authors::open(self),
            Projects => projects::open(self),
            NewProject => projects::open_new(self),
            LiveSearches => searches::open(self),
            SaveSearch => searches::open_save(self, None),
            Journals => journals::open(self),
            NewJournal => journals::open_new(self),
            Reports => reports::open(self),
            NewReport => reports::open_new(self),
            Rules => rules::open(self),
            NetStatus => network::open(self, network::Tab::Node),
            NetStart => {
                network::start(self);
                network::open(self, network::Tab::Node);
            }
            NetStop => {
                network::stop(self);
                network::open(self, network::Tab::Node);
            }
            NetPeers => network::open(self, network::Tab::Peers),
            NetRelays => network::open(self, network::Tab::Relays),
            NetExchanges => network::open(self, network::Tab::Exchanges),
            NetConnections => network::open(self, network::Tab::Users),
            NetInvites => network::open(self, network::Tab::Invites),
            NetShared => network::open(self, network::Tab::Shared),
            NetShards => network::open(self, network::Tab::Shards),
            NetEntrust => network::open_entrust(self),
            NetInvite => network::open_invite(self),
            NetSearch => network::open(self, network::Tab::Search),
            NetSettings => network::open(self, network::Tab::Settings),
            Orgs => orgs::open(self, orgs::Tab::Orgs),
            OrgNew => orgs::open_new(self),
            OrgMembers => orgs::open(self, orgs::Tab::Members),
            OrgRoles => orgs::open(self, orgs::Tab::Roles),
            OrgResources => orgs::open(self, orgs::Tab::Resources),
            OrgJoined => orgs::open(self, orgs::Tab::Joined),
            OrgSite => orgs::open(self, orgs::Tab::Site),
            Reviews => reviews::open(self, reviews::Tab::Reviews),
            ReviewNew => reviews::open_new(self),
            ReviewThread => reviews::open(self, reviews::Tab::Thread),
            ReviewsJoined => reviews::open(self, reviews::Tab::Joined),
            OrgHostStart => {
                orgs::start_host(self);
                orgs::open(self, orgs::Tab::Site);
            }
            OrgHostStop => {
                orgs::stop_host(self);
                orgs::open(self, orgs::Tab::Site);
            }
            Code => code::open(self),
            Pipes => pipes::open(self),
            Models => models::open(self),
            AddToProject => match projects::selected_series(self) {
                Some(r) => {
                    let label = r.label();
                    let payload = serde_json::to_value(&r).unwrap_or_default();
                    workspace::pick_project(
                        self,
                        workspace::Purpose::Add(crate::store::ItemKind::Result, r.key(), payload, label),
                        workspace::Back::Close,
                    );
                }
                None => self.set_status("select a series in the sidebar first"),
            },
            ToggleHistory => {
                let open = self.prefs.tui.history_open;
                if open && !self.history.focused {
                    self.history.focused = true;
                    self.sidebar.focused = false;
                    self.sidebar.searching = false;
                } else {
                    self.prefs.tui.history_open = !open;
                    self.history.focused = !open;
                    if !open {
                        self.sidebar.focused = false;
                        self.sidebar.searching = false;
                        self.refresh_history(true);
                    }
                    self.save_prefs();
                }
            }
            HistoryBack => {
                let r = self.store.history_back(1);
                self.apply_move(r);
            }
            HistoryForward => {
                let r = self.store.history_forward(1);
                self.apply_move(r);
            }
            ThemeSelect => self.modal = Some(Modal::Themes),
            ThemeMode(m) => self.set_theme_mode(m),
            ThemeEdit | ThemeNew => {
                let name = self.prefs.theme.name.clone();
                if let Some(i) = self.themes.names.iter().position(|n| *n == name) {
                    self.themes.list.select(Some(i));
                }
                self.modal = Some(Modal::Themes);
                themes::on_key(self, KeyEvent::from(KeyCode::Char('e')));
                if a == ThemeNew {
                    themes::on_key(self, KeyEvent::from(KeyCode::Char('s')));
                }
            }
            Import => import::open(self),
            ThemeImport => self.modal = Some(Modal::prompt("Import theme from file", modal::PromptKind::ThemeImport)),
            ThemeExport => self.modal = Some(Modal::prompt("Export current theme to file", modal::PromptKind::ThemeExport)),
            Sources => self.modal = Some(Modal::info("Sources", modal::sources_lines(&self.store))),
            Symbols => self.modal = Some(Modal::info("Symbols", modal::symbols_lines(&self.store))),
            Coverage => self.modal = Some(Modal::info("Coverage & gaps", modal::coverage_lines(&self.store))),
            Refresh => {
                self.series = self.store.series().unwrap_or_default();
                self.caches.clear();
                self.data.clear();
                self.sidebar.price_cache.clear();
                self.refresh_rows();
                self.set_status(format!("refreshed: {} series", self.series.len()));
            }
            NewTile(v) => {
                let d = self.layout.new_dir;
                self.split(d, Some(view_of(v)));
            }
            SplitRight => self.split(layout::Dir::Horizontal, None),
            SplitBelow => self.split(layout::Dir::Vertical, None),
            CloseTile => {
                let r = self.layout.close();
                self.layout_result(r, "tile closed");
            }
            ZoomTile => {
                self.layout.toggle_zoom();
                self.layout_changed();
            }
            Equalize => {
                self.layout.equalize();
                self.layout_changed();
            }
            Container(m) => {
                let r = self.layout.set_mode(m.into());
                self.layout_result(r, &format!("container {m:?}").to_lowercase());
            }
            Preset(p) => {
                let s = self.layout.focused().series.clone();
                self.layout.preset(p, s);
                self.caches.clear();
                self.layout_changed();
            }
            LayoutSave => self.modal = Some(Modal::prompt("Save layout as", modal::PromptKind::LayoutSave)),
            LayoutsSaved => {
                let items: Vec<String> = self.prefs.tui.layouts.keys().cloned().collect();
                self.modal = Some(Modal::Pick { title: "Saved layouts".into(), items, sel: 0, kind: modal::PickKind::SavedLayout });
            }
            Focus(d) => {
                let c = self.content;
                if self.layout.focus_dir(d, c) {
                    self.layout_changed();
                }
            }
            FocusNext => {
                self.layout.focus_next();
                self.layout_changed();
            }
            FocusPrev => {
                self.layout.focus_prev();
                self.layout_changed();
            }
            FocusTile(n) => {
                if self.layout.tile(n).is_some() {
                    self.layout.set_focus(n);
                    self.layout_changed();
                } else {
                    self.set_status(format!("no tile [{n}]"));
                }
            }
            Resize(d) => {
                let c = self.content;
                let r = self.layout.resize(d, c);
                self.layout_result(r, "resized");
            }
            Swap => {
                let r = self.layout.swap();
                self.layout_result(r, "swapped");
            }
            ChangeView => {
                let v = self.layout.change_view();
                self.layout_changed();
                self.set_status(format!("view: {v:?}").to_lowercase());
            }
            LinkTile => {
                let l = self.layout.cycle_link();
                self.layout_changed();
                let id = self.layout.focus;
                match l {
                    Some(o) => {
                        self.set_status(format!("tile {id} linked to tile {o}"));
                        self.propagate_from_list(id);
                        self.propagate_from_chart(id);
                    }
                    None => self.set_status(format!("tile {id} unlinked")),
                }
            }
            ChartLayers => {
                if self.layout.focused().view != layout::View::Chart {
                    return self.set_status("focus a chart tile first (Ctrl-w t turns a list into a chart)");
                }
                self.modal = Some(Modal::Layers { sel: 0 });
            }
            ToggleSidebar => {
                self.prefs.tui.sidebar_collapsed = !self.prefs.tui.sidebar_collapsed;
                if self.prefs.tui.sidebar_collapsed {
                    self.sidebar.focused = false;
                    self.sidebar.searching = false;
                }
                self.save_prefs();
            }
            FocusSidebar => {
                self.history.focused = false;
                self.prefs.tui.sidebar_collapsed = false;
                self.sidebar.focused = true;
                self.sidebar.searching = true;
            }
            Filter(action::FilterKind::Time) => self.open_range_picker(picker::Target::RangeFrom),
            Filter(kind) => {
                self.prefs.tui.sidebar_collapsed = false;
                self.modal = Some(Modal::Filter { kind, sel: 0, edit: None });
            }
            ClearFilters => {
                self.prefs.tui.sidebar.filters = Default::default();
                self.filters_changed();
                self.set_status("filters cleared");
            }
            SortYear | SortName => {
                self.prefs.tui.sidebar.sort = if a == SortYear { sidebar::SortBy::Year } else { sidebar::SortBy::Name };
                self.sidebar_prefs_changed();
            }
            ToggleOrder => {
                self.prefs.tui.sidebar.desc = !self.prefs.tui.sidebar.desc;
                self.sidebar_prefs_changed();
                self.set_status(if self.prefs.tui.sidebar.desc { "sort: descending" } else { "sort: ascending" });
            }
            ToggleSplitDir => {
                let d = self.layout.toggle_split_dir();
                self.set_status(format!("new tiles split {}", if d == layout::Dir::Horizontal { "right" } else { "below" }));
            }
        }
    }

    pub fn set_theme_mode(&mut self, m: crate::prefs::ThemeMode) {
        self.prefs.theme.mode = m;
        self.save_prefs();
        let v = theme::resolve_variant(m);
        let t = self.theme_file.clone();
        self.apply_theme(t, v);
        self.set_status(format!("mode {m} (showing {})", v.name()));
        self.note_view();
    }

    pub fn on_key(&mut self, k: KeyEvent) {
        if k.kind != KeyEventKind::Press {
            return;
        }
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return;
        }
        if self.menu.is_open() {
            let en = |a| self.enabled(a);
            let out = { self.menu.clone() }.on_key_with(k, &en);
            self.menu = out.0;
            if let menu::Outcome::Dispatch(a) = out.1 {
                self.dispatch(a);
            }
            return;
        }
        if self.pending_w {
            self.pending_w = false;
            if let Some(a) = window_action(k) {
                self.dispatch(a);
            } else {
                self.set_status("unknown window command");
            }
            return;
        }
        // Alt+letter opens a menu from anywhere.
        if k.modifiers.contains(KeyModifiers::ALT)
            && let KeyCode::Char(c) = k.code
            && let Some(i) = menu::menus().iter().position(|m| m.mnemonic == c.to_ascii_lowercase())
        {
            self.menu.open_top(i);
            return;
        }
        if let Some(a) = self.key_action(k)
            && !(a == Action::Help && matches!(self.modal, Some(Modal::Help(_))))
        {
            return self.dispatch(a);
        }
        if self.modal.is_some() {
            return modal::on_key(self, k);
        }
        if k.code == KeyCode::Char('w') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.pending_w = true;
            self.set_status("C-w: v s q h j k l Tab < > + - = x z t e y");
            return;
        }
        if self.history.focused && self.prefs.tui.history_open && history::on_key(self, k) {
            return;
        }
        if self.sidebar.focused && sidebar_key(self, k) {
            return;
        }
        if frametile::on_key(self, k) || self.layout.focused().frame.is_some() {
            return;
        }
        tiles::on_key(self, k);
    }

    /// Close the open popup like Esc ([X] button). An import in progress asks to cancel.
    pub fn close_modal(&mut self) {
        match self.modal {
            Some(Modal::Import) => import::close(self),
            Some(_) => self.modal = None,
            None => {}
        }
    }

    /// Recompute the sidebar rows (search, filters, sort).
    pub fn refresh_rows(&mut self) {
        let p = self.prefs.tui.sidebar.clone();
        let scope = p.filters.scope();
        let metric = p.filters.price.metric;
        if p.filters.price.active() {
            for s in &self.series {
                let k = (layout::SeriesKey::of(s), metric, scope);
                if !self.sidebar.price_cache.contains_key(&k)
                    && let Some(d) = self.data.get(&k.0, &self.store)
                {
                    let r = d.metric_range(d.range(scope), metric);
                    self.sidebar.price_cache.insert(k, r);
                }
            }
            let n = self.data.loading();
            if n > 0 {
                self.set_status(format!("loading {n} series for the price filter…"));
            }
        }
        let cache = &self.sidebar.price_cache;
        let price = |s: &crate::store::SeriesInfo| cache.get(&(layout::SeriesKey::of(s), metric, scope)).copied().flatten();
        self.sidebar.rows = sidebar::rows(&self.series, &self.sidebar.query, &p, &price);
    }

    pub fn sidebar_prefs_changed(&mut self) {
        self.refresh_rows();
        self.save_prefs();
    }

    /// Filters changed: rows and tile scope.
    pub fn filters_changed(&mut self) {
        self.caches.clear();
        self.sidebar_prefs_changed();
        self.note_view();
    }

    /// Show the selected sidebar series in the focused tile.
    pub fn assign_selected(&mut self) {
        if let Some(&i) = self.sidebar.rows.get(self.sidebar.sel) {
            let k = layout::SeriesKey::of(&self.series[i]);
            self.set_series(k);
        }
    }

    fn split(&mut self, d: layout::Dir, v: Option<layout::View>) {
        let c = self.content;
        let r = self.layout.split(d, v, c).map(|_| ());
        self.layout_result(r, "split");
    }

    fn layout_result(&mut self, r: Result<(), String>, ok: &str) {
        match r {
            Ok(()) => {
                self.layout_changed();
                self.set_status(ok.to_string());
            }
            Err(e) => self.set_status(e),
        }
    }

    /// Persist the layout after any change.
    pub fn layout_changed(&mut self) {
        self.prefs.tui.layout = Some(self.layout.clone());
        let _ = self.prefs.save(&self.paths);
    }

    /// Time scope for tiles (sidebar year filter).
    pub fn scope(&self) -> Option<(i64, i64)> {
        self.prefs.tui.sidebar.filters.scope()
    }

    /// Show the previous/next series in the focused tile.
    pub fn cycle_series(&mut self, step: i32) {
        if self.series.is_empty() {
            self.set_status("no data yet: File ▸ Import…");
            return;
        }
        // Follow the sidebar (search, filters, sort); fall back to every series.
        let idx: Vec<usize> = if self.sidebar.rows.is_empty() { (0..self.series.len()).collect() } else { self.sidebar.rows.clone() };
        let keys: Vec<layout::SeriesKey> = idx.iter().map(|i| layout::SeriesKey::of(&self.series[*i])).collect();
        let cur = self.layout.focused().series.clone();
        let i = cur.and_then(|c| keys.iter().position(|k| *k == c)).map(|i| i as i32).unwrap_or(-1);
        let n = keys.len() as i32;
        let next = keys[((i + step).rem_euclid(n)) as usize].clone();
        self.set_series(next);
    }

    pub fn set_series(&mut self, k: layout::SeriesKey) {
        let id = self.layout.focus;
        self.set_status(format!("tile {id}: {}", k.label()));
        self.layout.focused_mut().series = Some(k);
        self.caches.remove(&id);
        self.layout_changed();
    }

    pub fn on_mouse(&mut self, m: MouseEvent) {
        if self.modal.is_some() {
            // A popup takes every click; clicks outside it do nothing.
            if m.kind == MouseEventKind::Down(MouseButton::Left) {
                let h = self.modal_hits.iter().rev().find(|(r, _)| r.contains((m.column, m.row).into())).map(|(_, h)| *h);
                match h {
                    Some(Hit::CloseModal) => self.close_modal(),
                    Some(h) => modal::on_click(self, h),
                    None => {}
                }
            }
            return;
        }
        let found = self.hits.iter().rev().find(|(r, _)| r.contains((m.column, m.row).into())).copied();
        let hit = found.map(|(_, h)| h);
        if matches!(m.kind, MouseEventKind::ScrollUp | MouseEventKind::ScrollDown) && self.modal.is_none() && !self.menu.is_open() {
            let up = m.kind == MouseEventKind::ScrollUp;
            let id = match hit {
                Some(Hit::ChartPlot(id))
                | Some(Hit::Tile(id))
                | Some(Hit::ListHeader(id, _))
                | Some(Hit::ChartNav(id, _))
                | Some(Hit::ListRow(id, _)) => id,
                _ => return,
            };
            self.layout.set_focus(id);
            match self.layout.focused().view {
                layout::View::Chart => self.chart_zoom(id, up),
                layout::View::List => {
                    let c = &mut self.caches.entry(id).or_default().list;
                    c.sel = (c.sel + if up { -3 } else { 3 }).clamp(0, (c.total - 1).max(0));
                    self.propagate_from_list(id);
                }
            }
            return;
        }
        if m.kind != MouseEventKind::Down(MouseButton::Left) {
            return;
        }
        match hit {
            Some(Hit::MenuTop(i)) => {
                if self.menu.open == Some(i) {
                    self.menu.close();
                } else {
                    self.menu.open_top(i);
                }
            }
            Some(Hit::MenuItem(d, i)) => {
                let en = |a| self.enabled(a);
                let mut st = self.menu.clone();
                let out = st.click(d, i, &en);
                self.menu = st;
                if let menu::Outcome::Dispatch(a) = out {
                    self.dispatch(a);
                }
            }
            Some(Hit::Tile(id)) | Some(Hit::TabBar(id)) => {
                self.menu.close();
                self.sidebar.focused = false;
                self.sidebar.searching = false;
                self.layout.set_focus(id);
                self.layout_changed();
            }
            Some(Hit::Sidebar) => {
                self.sidebar.focused = true;
                self.history.focused = false;
            }
            Some(Hit::HistoryBar) => {
                self.history.focused = true;
                self.sidebar.focused = false;
                self.sidebar.searching = false;
            }
            Some(Hit::HistoryCollapse) => self.dispatch(Action::ToggleHistory),
            Some(Hit::HistoryRow(r)) => {
                self.history.focused = true;
                self.sidebar.focused = false;
                self.sidebar.searching = false;
                if self.history.sel == r {
                    self.history_goto_row(r);
                } else {
                    self.history.sel = r;
                }
            }
            Some(Hit::SidebarSearch) => {
                self.sidebar.focused = true;
                self.sidebar.searching = true;
            }
            Some(Hit::SidebarSortKey) => {
                let a = if self.prefs.tui.sidebar.sort == sidebar::SortBy::Year { Action::SortName } else { Action::SortYear };
                self.dispatch(a);
            }
            Some(Hit::SidebarSortOrder) => self.dispatch(Action::ToggleOrder),
            Some(Hit::SidebarFilter(k)) => self.dispatch(Action::Filter(k)),
            Some(Hit::Chip(i)) => {
                if let Some((c, _)) = self.prefs.tui.sidebar.filters.chips().get(i).cloned() {
                    self.prefs.tui.sidebar.filters.remove(&c);
                    self.filters_changed();
                }
            }
            Some(Hit::ClearFilters) => self.dispatch(Action::ClearFilters),
            Some(Hit::ListRow(id, row)) => {
                self.menu.close();
                self.sidebar.focused = false;
                self.layout.set_focus(id);
                let c = &mut self.caches.entry(id).or_default().list;
                c.sel = row.clamp(0, (c.total - 1).max(0));
                self.propagate_from_list(id);
                self.layout_changed();
            }
            Some(Hit::Url(i)) => self.open_url(i),
            Some(Hit::ShareView) => searches::open_save_then_share(self),
            Some(Hit::Press(code)) => self.on_key(ratatui::crossterm::event::KeyEvent::from(code)),
            Some(Hit::Chord(code, m)) => self.on_key(ratatui::crossterm::event::KeyEvent::new(code, m)),
            Some(
                Hit::CloseModal
                | Hit::ImportCancel
                | Hit::ImportConfirm(_)
                | Hit::ImportPlace(_)
                | Hit::ImportCandidate(_)
                | Hit::ImportEntry(_)
                | Hit::ImportAsk(_)
                | Hit::ImportInput(_)
                | Hit::Row(_)
                | Hit::HelpLink(_)
                | Hit::Picker(_)
                | Hit::FormRow(_),
            ) => {}
            Some(Hit::SidebarRow(r)) => {
                self.sidebar.focused = true;
                self.sidebar.searching = false;
                self.sidebar.sel = r;
                self.assign_selected();
            }
            Some(Hit::ChartNav(id, which)) => {
                self.layout.set_focus(id);
                self.chart_nav(id, which);
            }
            Some(Hit::ChartDate(id, which)) => {
                self.layout.set_focus(id);
                self.open_picker(id, if which == 0 { picker::Target::ChartStart } else { picker::Target::ChartEnd });
            }
            Some(Hit::ListRange(id, which)) => {
                self.layout.set_focus(id);
                self.open_picker(id, if which == 0 { picker::Target::ListFrom } else { picker::Target::ListTo });
            }
            Some(Hit::ChartPlot(id)) => {
                self.layout.set_focus(id);
                let r = found.unwrap().0;
                let c = &mut self.caches.entry(id).or_default().chart;
                let idx = (m.column - r.x) as usize;
                if let Some(b) = c.visible().get(idx) {
                    c.cursor = Some(b.ts);
                }
                self.propagate_from_chart(id);
            }
            Some(Hit::ListHeader(id, col)) => {
                self.layout.set_focus(id);
                if let Some((k, _)) = tiles::COLUMNS.get(col) {
                    let t = self.layout.focused_mut();
                    if t.list.sort == *k {
                        t.list.desc = !t.list.desc;
                    } else {
                        t.list.sort = *k;
                        t.list.desc = false;
                    }
                }
                self.layout_changed();
            }
            None => self.menu.close(),
        }
    }

    pub fn status_items(&self) -> Vec<String> {
        let mut v = vec![format!("● unlocked · {} ({})", self.prefs.theme.mode, self.variant.name()), self.paths.home.display().to_string()];
        if let Ok(Some(a)) = self.store.current_author() {
            v.push(format!("author: {}", a.name));
        }
        v
    }

    pub fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        let t = self.theme.clone();
        self.hits.clear();
        import::poll(self);
        if self.data.poll() && self.prefs.tui.sidebar.filters.price.active() {
            self.refresh_rows();
        }
        f.render_widget(Block::new().style(Style::new().bg(t.bg).fg(t.fg)), area);
        let hh = header::height(self.prefs.tui.header, area);
        let [head, bar, body, bottom] =
            Layout::vertical([Constraint::Length(hh), Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)]).areas(area);
        header::draw(f, head, &t, &self.status_items());
        let mut hits = std::mem::take(&mut self.hits);
        let xs = menu::draw_bar(&self.menu, f, bar, &t, &mut hits);
        let main = if self.prefs.tui.history_open && body.width >= 60 + history::WIDTH {
            let [h, rest] = Layout::horizontal([Constraint::Length(history::WIDTH), Constraint::Min(0)]).areas(body);
            history::draw(self, f, h, &t, &mut hits);
            rest
        } else {
            body
        };
        let (content, side) = if self.prefs.tui.sidebar_collapsed || main.width < 60 {
            (main, None)
        } else {
            let [c, s] = Layout::horizontal([Constraint::Min(0), Constraint::Length(sidebar::WIDTH)]).areas(main);
            (c, Some(s))
        };
        self.draw_content(f, content, &mut hits);
        if let Some(s) = side {
            let cur = self.layout.focused().series.clone();
            sidebar::draw(&mut self.sidebar, &self.prefs.tui.sidebar, &self.series, cur.as_ref(), f, s, &t, &mut hits);
        }
        if self.modal.is_some() {
            modal::draw(self, f, body);
        }
        let status = Paragraph::new(Line::from(vec![
            Span::styled(format!(" {} ", self.theme_file.display_name()), Style::new().fg(t.bg).bg(t.accent)),
            Span::styled(format!(" {} ", self.variant.name()), Style::new().fg(t.bg).bg(t.info)),
            Span::styled(format!(" {}", self.status), Style::new().fg(t.muted)),
        ]));
        // the history bar's toggle, on the left of the status line
        let hl = if self.prefs.tui.history_open { " « History " } else { " » History " };
        let hb = Rect { width: (hl.chars().count() as u16).min(bottom.width), ..bottom };
        f.render_widget(Paragraph::new(Line::styled(hl, Style::new().fg(t.bg).bg(t.muted))), hb);
        hits.push((hb, Hit::HistoryCollapse));
        f.render_widget(status, Rect { x: bottom.x + hb.width, width: bottom.width - hb.width, ..bottom });
        let en = |a| self.enabled(a);
        menu::draw_dropdowns(&self.menu, f, area, bar, &xs, &t, &en, &mut hits);
        // every key named in hint text is a button too (below the drawn controls)
        let colours = [t.muted, t.header, t.warning];
        if self.modal.is_some() && share::item(self).is_some() && self.modal_area.width > 16 {
            // the window's share button, on its top border, next to the [X]
            let r = self.modal_area;
            let b = Rect { x: r.right().saturating_sub(18), y: r.y, width: 11, height: 1 };
            let st = Style::new().fg(t.bg).bg(t.accent).add_modifier(Modifier::BOLD);
            f.render_widget(Paragraph::new(Line::styled(" ⇪ S Share ", st)), b);
            self.modal_hits.push((b, Hit::Chord(KeyCode::Char('S'), KeyModifiers::NONE)));
        }
        if self.modal.is_none() && !self.menu.is_open() && content.width > 30 {
            // the main view: share it from the status line (saved as a live search, then the share picker)
            let b = Rect { x: bottom.right().saturating_sub(16), y: bottom.y, width: 14, height: 1 };
            let st = Style::new().fg(t.bg).bg(t.accent).add_modifier(Modifier::BOLD);
            f.render_widget(Paragraph::new(Line::styled(" ⇪ Share view ", st)), b);
            hits.push((b, Hit::ShareView));
        }
        if self.modal.is_some() {
            let found = widgets::scan_hints(f.buffer_mut(), self.modal_area, &colours);
            self.modal_hits.splice(0..0, found);
        } else if !self.menu.is_open() {
            hits.splice(0..0, widgets::scan_hints(f.buffer_mut(), area, &colours));
        }
        // every URL on screen is clickable (on top of anything else)
        self.urls =
            if self.menu.is_open() { vec![] } else { widgets::scan_urls(f.buffer_mut(), if self.modal.is_some() { self.modal_area } else { area }) };
        for (i, (r, _)) in self.urls.iter().enumerate() {
            if self.modal.is_some() {
                self.modal_hits.push((*r, Hit::Url(i)));
            } else {
                hits.push((*r, Hit::Url(i)));
            }
        }
        self.hits = hits;
    }

    fn draw_content(&mut self, f: &mut Frame, area: Rect, hits: &mut Vec<(Rect, Hit)>) {
        self.content = area;
        let t = self.theme.clone();
        let scope = self.scope();
        for p in self.layout.placed(area) {
            match p {
                layout::Placed::Tile(id, r) => {
                    let Some(tile) = self.layout.tile(id).cloned() else { continue };
                    if let Some(ft) = &tile.frame {
                        frametile::draw(self, f, r, &tile, ft, id == self.layout.focus, &t, hits);
                        continue;
                    }
                    let bounds = self.bounds(tile.series.as_ref());
                    let d = tile.series.as_ref().and_then(|k| self.data.get(k, &self.store));
                    let pending = tile.series.as_ref().and_then(|k| self.data.pending(k));
                    let cache = self.caches.entry(id).or_default();
                    cache.pending = pending;
                    tiles::draw_tile(f, r, &tile, cache, d, scope, bounds, id == self.layout.focus, &t, hits);
                }
                layout::Placed::Bar { rect, mode, titles, active, first_ids } => {
                    let mut spans = vec![];
                    let mut x = rect.x;
                    for (i, title) in titles.iter().enumerate() {
                        let on = i as u8 == active;
                        let st = if on {
                            Style::new().fg(t.bg).bg(t.accent).add_modifier(Modifier::BOLD)
                        } else {
                            Style::new().fg(t.muted).bg(t.selection_bg)
                        };
                        let text = format!(" {title} ");
                        let w = text.chars().count() as u16;
                        if mode == layout::Mode::Stacked {
                            let row = Rect { x: rect.x, y: rect.y + i as u16, width: rect.width, height: 1 };
                            if row.y < rect.bottom() {
                                f.render_widget(Paragraph::new(format!("{text:<w$}", w = rect.width as usize)).style(st), row);
                                hits.push((row, Hit::TabBar(first_ids[i])));
                            }
                        } else {
                            spans.push(Span::styled(text, st));
                            hits.push((Rect { x, y: rect.y, width: w, height: 1 }, Hit::TabBar(first_ids[i])));
                            x += w;
                        }
                    }
                    if mode == layout::Mode::Tabbed {
                        f.render_widget(Paragraph::new(Line::from(spans)).style(Style::new().bg(t.selection_bg)), rect);
                    }
                }
            }
        }
    }
}

/// Keys while the sidebar has focus. Returns true when handled.
fn sidebar_key(app: &mut App, k: KeyEvent) -> bool {
    let sb = &mut app.sidebar;
    if sb.searching {
        match k.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Down => sb.searching = false,
            KeyCode::Backspace => {
                sb.query.pop();
                sb.sel = 0;
                app.refresh_rows();
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                sb.query.push(c);
                sb.sel = 0;
                app.refresh_rows();
            }
            _ => return false,
        }
        return true;
    }
    let n = sb.rows.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => sb.sel = (sb.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Up | KeyCode::Char('k') => sb.sel = sb.sel.saturating_sub(1),
        KeyCode::PageDown => sb.sel = (sb.sel + 10).min(n.saturating_sub(1)),
        KeyCode::PageUp => sb.sel = sb.sel.saturating_sub(10),
        KeyCode::Home => sb.sel = 0,
        KeyCode::End => sb.sel = n.saturating_sub(1),
        KeyCode::Enter => app.assign_selected(),
        KeyCode::Char('s') => {
            let a = if app.prefs.tui.sidebar.sort == sidebar::SortBy::Year { Action::SortName } else { Action::SortYear };
            app.dispatch(a);
        }
        KeyCode::Char('f') => app.dispatch(Action::Filter(action::FilterKind::Source)),
        KeyCode::Char('m') => app.dispatch(Action::Filter(action::FilterKind::Symbol)),
        KeyCode::Char('t') => app.dispatch(Action::Filter(action::FilterKind::Time)),
        KeyCode::Char('p') => app.dispatch(Action::Filter(action::FilterKind::Price)),
        KeyCode::Char('d') => app.dispatch(Action::Filter(action::FilterKind::Period)),
        KeyCode::Char('y') => app.dispatch(Action::Filter(action::FilterKind::Year)),
        KeyCode::Char('a') => app.dispatch(Action::AddToProject),
        KeyCode::Esc | KeyCode::Tab | KeyCode::Left => {
            sb.focused = false;
        }
        _ => return false,
    }
    true
}

fn view_of(v: action::ViewKind) -> layout::View {
    match v {
        action::ViewKind::List => layout::View::List,
        action::ViewKind::Chart => layout::View::Chart,
    }
}

/// Window commands after `Ctrl-w`.
pub fn window_action(k: KeyEvent) -> Option<Action> {
    use action::Dir4::*;
    Some(match k.code {
        KeyCode::Char('v') => Action::SplitRight,
        KeyCode::Char('s') => Action::SplitBelow,
        KeyCode::Char('q') | KeyCode::Char('c') => Action::CloseTile,
        KeyCode::Char('h') | KeyCode::Left => Action::Focus(Left),
        KeyCode::Char('j') | KeyCode::Down => Action::Focus(Down),
        KeyCode::Char('k') | KeyCode::Up => Action::Focus(Up),
        KeyCode::Char('l') | KeyCode::Right => Action::Focus(Right),
        KeyCode::Tab | KeyCode::Char('w') => Action::FocusNext,
        KeyCode::BackTab | KeyCode::Char('W') | KeyCode::Char('p') => Action::FocusPrev,
        KeyCode::Char('>') => Action::Resize(Right),
        KeyCode::Char('<') => Action::Resize(Left),
        KeyCode::Char('+') => Action::Resize(Down),
        KeyCode::Char('-') => Action::Resize(Up),
        KeyCode::Char('=') => Action::Equalize,
        KeyCode::Char('x') => Action::Swap,
        KeyCode::Char('z') => Action::ZoomTile,
        KeyCode::Char('t') => Action::ChangeView,
        KeyCode::Char('e') => Action::ToggleSplitDir,
        KeyCode::Char('y') => Action::LinkTile,
        _ => return None,
    })
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
    use ratatui::crossterm::{
        event::{DisableMouseCapture, EnableMouseCapture},
        execute,
    };
    network::autostart(&mut app);
    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let res = (|| -> crate::Result<()> {
        while !app.quit {
            app.tick();
            terminal.draw(|f| app.draw(f))?;
            if app.urls != app.painted_urls {
                let _ = widgets::paint_links(terminal.current_buffer_mut(), &app.urls);
                app.painted_urls = app.urls.clone();
            }
            if event::poll(std::time::Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(k) => app.on_key(k),
                    Event::Mouse(m) => app.on_mouse(m),
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
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    network::shutdown(&mut app);
    orgs::shutdown(&mut app);
    res
}

impl App {
    /// Open the n-th URL on screen in the system's browser (muninn:// links are shown, not opened).
    pub fn open_url(&mut self, i: usize) {
        let Some((_, url)) = self.urls.get(i).cloned() else { return };
        self.opened = Some(url.clone());
        if !url.starts_with("http://") && !url.starts_with("https://") {
            self.status = format!("link: {url}");
            return;
        }
        if cfg!(test) {
            return;
        }
        let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "macos") {
            ("open", vec![])
        } else if cfg!(windows) {
            ("cmd", vec!["/C", "start", ""])
        } else {
            ("xdg-open", vec![])
        };
        let r = std::process::Command::new(cmd)
            .args(args)
            .arg(&url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        self.status = match r {
            Ok(_) => format!("opened {url}"),
            Err(e) => format!("could not open {url}: {e}"),
        };
    }

    /// Periodic work between frames (background tasks).
    /// Every loop turn (about 10 a second): show what rules did.
    pub fn tick(&mut self) {
        rules::poll(self);
        code::poll(self);
        pipes::poll(self);
        frametile::poll(self);
        designer::poll(self);
        network::poll(self);
        orgs::poll(self);
        reviews::poll(self);
    }
}

#[cfg(test)]
mod code_tests;
#[cfg(test)]
mod frames_tests;
#[cfg(test)]
mod models_tests;
#[cfg(test)]
mod network_tests;
#[cfg(test)]
mod orgs_tests;
#[cfg(test)]
mod reports_tests;
#[cfg(test)]
mod reviews_tests;
#[cfg(test)]
mod rules_tests;
#[cfg(test)]
mod screenshots;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod workspace_tests;
