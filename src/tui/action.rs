//! Named actions. Menus, keybindings and help all dispatch these.
use crate::prefs::ThemeMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewKind {
    List,
    Chart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    TwoUp,
    ThreeColumn,
    MainStack,
    Grid2x2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContainerMode {
    Split,
    Tabbed,
    Stacked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dir4 {
    Left,
    Down,
    Up,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterKind {
    Source,
    Symbol,
    Price,
    Period,
    Year,
    Time,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // File
    Import,
    Export,
    BackupCreate,
    BackupRestore,
    Preferences,
    Quit,
    // View
    NewTile(ViewKind),
    SplitRight,
    SplitBelow,
    CloseTile,
    ZoomTile,
    Equalize,
    Container(ContainerMode),
    Preset(Preset),
    LayoutsSaved,
    LayoutSave,
    ToggleSidebar,
    Focus(Dir4),
    FocusNext,
    FocusPrev,
    /// Focus the tile with this id (Alt+1..9).
    FocusTile(u32),
    Resize(Dir4),
    Swap,
    ChangeView,
    ToggleSplitDir,
    LinkTile,
    ChartLayers,
    // Data
    Sources,
    Symbols,
    Filter(FilterKind),
    ClearFilters,
    SortYear,
    SortName,
    ToggleOrder,
    Coverage,
    Refresh,
    FocusSidebar,
    // Keys
    KeysInit,
    Lock,
    RotateKey,
    Rekey,
    KeysExport,
    RecoverWords,
    RecoverShards,
    ReissueShards,
    // Theme
    ThemeSelect,
    ThemeMode(ThemeMode),
    ThemeEdit,
    ThemeNew,
    ThemeImport,
    ThemeExport,
    // Help
    Help,
    HelpSearch,
    Keybindings,
    About,
    // Menu
    OpenMenu,
}

impl Action {
    /// Human name (shown in help and the status line).
    pub fn name(self) -> String {
        use Action::*;
        match self {
            Import => "import".into(),
            Export => "export".into(),
            BackupCreate => "backup.create".into(),
            BackupRestore => "backup.restore".into(),
            Preferences => "preferences".into(),
            Quit => "quit".into(),
            NewTile(v) => format!("tile.new.{}", if v == ViewKind::List { "list" } else { "chart" }),
            SplitRight => "tile.split-right".into(),
            SplitBelow => "tile.split-below".into(),
            CloseTile => "tile.close".into(),
            ZoomTile => "tile.zoom".into(),
            Equalize => "layout.equalize".into(),
            Container(m) => format!("container.{m:?}").to_lowercase(),
            Preset(p) => format!("layout.preset.{p:?}").to_lowercase(),
            LayoutsSaved => "layout.saved".into(),
            LayoutSave => "layout.save".into(),
            ToggleSidebar => "sidebar.toggle".into(),
            Focus(d) => format!("tile.focus.{d:?}").to_lowercase(),
            FocusNext => "tile.focus-next".into(),
            FocusPrev => "tile.focus-prev".into(),
            FocusTile(n) => format!("tile.focus-{n}"),
            Resize(d) => format!("tile.resize.{d:?}").to_lowercase(),
            Swap => "tile.swap".into(),
            ChangeView => "tile.change-view".into(),
            ToggleSplitDir => "layout.toggle-split-dir".into(),
            LinkTile => "tile.link".into(),
            ChartLayers => "chart.layers".into(),
            Sources => "data.sources".into(),
            Symbols => "data.symbols".into(),
            Filter(k) => format!("filter.{k:?}").to_lowercase(),
            ClearFilters => "filter.clear".into(),
            SortYear => "sort.year".into(),
            SortName => "sort.name".into(),
            ToggleOrder => "sort.toggle-order".into(),
            Coverage => "data.coverage".into(),
            Refresh => "data.refresh".into(),
            FocusSidebar => "sidebar.focus".into(),
            KeysInit => "keys.init".into(),
            Lock => "keys.lock".into(),
            RotateKey => "keys.rotate".into(),
            Rekey => "keys.rekey".into(),
            KeysExport => "keys.export".into(),
            RecoverWords => "keys.recover.words".into(),
            RecoverShards => "keys.recover.shards".into(),
            ReissueShards => "keys.shards".into(),
            ThemeSelect => "theme.select".into(),
            ThemeMode(m) => format!("theme.mode.{m}"),
            ThemeEdit => "theme.edit".into(),
            ThemeNew => "theme.new".into(),
            ThemeImport => "theme.import".into(),
            ThemeExport => "theme.export".into(),
            Help => "help".into(),
            HelpSearch => "help.search".into(),
            Keybindings => "help.keybindings".into(),
            About => "about".into(),
            OpenMenu => "menu.open".into(),
        }
    }
}
