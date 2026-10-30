# Menus

The menu bar sits under the header: **File, View, Data, Projects, Net, Orgs, Keys, Theme, Help**.
The nine categories fit in 80 columns; the `F10 menu · Alt+letter` hint follows them when
there is room. Dropdowns open directly below their category and never cover the bar (on a
short terminal a long dropdown is clipped at the bottom instead).

## Opening and moving

| input | effect |
|-------|--------|
| F10 | open the first menu (File) |
| Alt+letter | open the menu with that underlined letter (Alt+F File, Alt+V View, Alt+D Data, Alt+P Projects, Alt+N Net, Alt+O Orgs, Alt+K Keys, Alt+T Theme, Alt+H Help) |
| mouse click | click a menu title to open or close it, click an item to run it |
| ← → / h l | previous / next menu; ← also closes a submenu |
| ↑ ↓ / k j | move inside a dropdown |
| Enter, Space, → | run the item, or open its submenu (`▸`) |
| underlined letter | run that item directly (letters win over h/j/k/l) |
| Esc | close the submenu, then the menu |

Each item shows its keyboard shortcut on the right. Greyed-out items are not available
right now (for example **Keys ▸ Initialize…** once keys exist, or **Recover** while the
database is unlocked).

## Actions

Every menu item runs a named action. The same actions are bound to keys
([keybindings](keybindings.md)), so a menu item and its shortcut always do the same thing.

| menu | items |
|------|-------|
| File | Import… · Export… · Backup ▸ (Create, Restore) · Authors… · Preferences… · Quit |
| View | New tile ▸ · Split ▸ · Close tile · Zoom tile · Equalize · Change view · Link tile · Chart layers… · Container ▸ · Layouts ▸ · Toggle sidebar · History ▸ (Show/hide history bar, Step back, Step forward) |
| Data | Sources… · Symbols… · Filters ▸ (Source, Symbol, Time range…, Price, Period, Year, Clear all) · Sort ▸ · Coverage & gaps · Model designer… · Refresh |
| Projects | Projects… · New project… · Live searches… · Save search as live search… · Journals… · New journal… · Reports… · New report… · Reviews ▸ (Reviews…, Ask for a review…, Thread…, Reviewing (joined)…) ([reviews](reviews.md)) · Rules… · Code spaces… · Add selected series to project… |
| Net | Node ▸ (Status… `K`, Start node, Stop node, Settings…) · Connections… · Invite or connect… · Peers… · Relays… · Shared data… · Network search… · Dark shards ▸ (Held & entrusted…, Entrust…) ([network](network.md), [key exchanges](key-exchanges.md), [dark shards](dark-shards.md)) |
| Orgs | Organizations… `G` · New organization… · Members… · Roles… · Resources… · Joined & browse… · Site & certificate… · Start site · Stop site ([organizations](organizations.md), [org hosting](org-hosting.md)) |
| Keys | Initialize… · Lock · Rotate service key · Rekey database · Export… · Recover ▸ · Re-issue shards |
| Theme | Select… · Mode ▸ (Light, Dark, System) · Edit current… · New from current… · Import… · Export… |
| Help | Help topics · Search help… · Keybindings · About Muninn |

Notes:

- **Backup ▸ Restore**, **Keys ▸ Rekey database** and **Keys ▸ Export…** show the
  command to run from the shell: they replace or rewrite the open database, or handle
  the 24 words, so they don't run inside the TUI. See [keys](keys.md) and
  [backup and recovery](backup-recovery.md).
- **Keys ▸ Lock** closes the TUI; the key material is dropped from memory.
- **Backup ▸ Create**, **Rotate service key** and **Re-issue shards** open the keys
  dialog on that action.
- **Theme** items open the [theme](themes.md) selector or editor; **Import…/Export…**
  ask for a file path.
- **Projects** opens [projects](projects.md), [live searches](live-searches.md),
  [journals](journals.md), [reports](reports.md), [rules](rules.md) and
  [code spaces](code-spaces.md).
- **File ▸ Authors…** manages [authors](authors.md) and the current author.
- **View ▸ History** shows the [history](history.md) bar and steps through it.
- **File ▸ Preferences…** toggles the header size, the sidebar, the history bar and zero-volume
  handling ([preferences](preferences.md)).
