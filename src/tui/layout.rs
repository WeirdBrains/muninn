//! Tiling layout (i3 style): a binary tree of containers with leaf tiles.
//! Pure model + geometry; no drawing. Persisted in `preferences.toml`.
use super::action::{ContainerMode, Dir4, Preset};
use crate::store::BarSort;
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

pub const MIN_W: u16 = 20;
pub const MIN_H: u16 = 6;
const STEP: f32 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum View {
    #[default]
    List,
    Chart,
}

/// Side by side (`Horizontal`, split right) or stacked (`Vertical`, split below).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Dir {
    #[default]
    Horizontal,
    Vertical,
}

impl Dir {
    pub fn flip(self) -> Self {
        match self {
            Dir::Horizontal => Dir::Vertical,
            Dir::Vertical => Dir::Horizontal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Split,
    Tabbed,
    Stacked,
}

impl From<ContainerMode> for Mode {
    fn from(m: ContainerMode) -> Self {
        match m {
            ContainerMode::Split => Mode::Split,
            ContainerMode::Tabbed => Mode::Tabbed,
            ContainerMode::Stacked => Mode::Stacked,
        }
    }
}

/// Which series a tile shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SeriesKey {
    pub source: String,
    /// `BASE/QUOTE`
    pub symbol: String,
    /// e.g. `5m`
    pub duration: String,
}

impl SeriesKey {
    pub fn label(&self) -> String {
        format!("{} · {} · {}", self.symbol, self.source, self.duration)
    }
    pub fn of(s: &crate::store::SeriesInfo) -> Self {
        SeriesKey { source: s.source.clone(), symbol: s.symbol.display(), duration: s.duration.to_string() }
    }
    pub fn filter(&self) -> crate::store::BarFilter {
        crate::store::BarFilter {
            source: Some(self.source.clone()),
            symbol: crate::model::Symbol::parse(&self.symbol).ok(),
            duration: crate::model::Duration::parse(&self.duration).ok(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ListOpts {
    #[serde(default)]
    pub sort: BarSort,
    #[serde(default)]
    pub desc: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tile {
    pub id: u32,
    #[serde(default)]
    pub view: View,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<SeriesKey>,
    /// Linked tile id: list selection centers the chart, chart range scrolls the list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<u32>,
    #[serde(default)]
    pub list: ListOpts,
    #[serde(default)]
    pub chart: super::chart::ChartOpts,
}

impl Tile {
    pub fn new(id: u32, view: View, series: Option<SeriesKey>) -> Self {
        Tile { id, view, series, link: None, list: ListOpts::default(), chart: Default::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Container {
    pub dir: Dir,
    #[serde(default)]
    pub mode: Mode,
    /// Share of the first child (0..1).
    pub ratio: f32,
    /// Visible child for tabbed/stacked containers (0 = first).
    #[serde(default)]
    pub active: u8,
    pub a: Box<Node>,
    pub b: Box<Node>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Node {
    Tile(Tile),
    Split(Container),
}

impl Node {
    fn split(dir: Dir, ratio: f32, a: Node, b: Node) -> Node {
        Node::Split(Container { dir, mode: Mode::Split, ratio, active: 0, a: Box::new(a), b: Box::new(b) })
    }
    pub fn tiles(&self) -> Vec<&Tile> {
        match self {
            Node::Tile(t) => vec![t],
            Node::Split(c) => {
                let mut v = c.a.tiles();
                v.extend(c.b.tiles());
                v
            }
        }
    }
    fn tiles_mut(&mut self) -> Vec<&mut Tile> {
        match self {
            Node::Tile(t) => vec![t],
            Node::Split(c) => {
                let mut v = c.a.tiles_mut();
                v.extend(c.b.tiles_mut());
                v
            }
        }
    }
    fn path_to(&self, id: u32) -> Option<Vec<bool>> {
        match self {
            Node::Tile(t) => (t.id == id).then(Vec::new),
            Node::Split(c) => {
                if let Some(mut p) = c.a.path_to(id) {
                    p.insert(0, false);
                    Some(p)
                } else if let Some(mut p) = c.b.path_to(id) {
                    p.insert(0, true);
                    Some(p)
                } else {
                    None
                }
            }
        }
    }
    fn at_mut(&mut self, path: &[bool]) -> &mut Node {
        let mut n = self;
        for &b in path {
            n = match n {
                Node::Split(c) => {
                    if b {
                        &mut c.b
                    } else {
                        &mut c.a
                    }
                }
                Node::Tile(_) => break,
            };
        }
        n
    }
    fn at(&self, path: &[bool]) -> &Node {
        let mut n = self;
        for &b in path {
            n = match n {
                Node::Split(c) => {
                    if b {
                        &c.b
                    } else {
                        &c.a
                    }
                }
                Node::Tile(_) => break,
            };
        }
        n
    }
    /// Units along `dir` for equalizing (nested same-direction splits count their leaves).
    fn units(&self, dir: Dir) -> f32 {
        match self {
            Node::Split(c) if c.dir == dir && c.mode == Mode::Split => c.a.units(dir) + c.b.units(dir),
            _ => 1.0,
        }
    }
    fn equalize(&mut self) {
        if let Node::Split(c) = self {
            let (ua, ub) = (c.a.units(c.dir), c.b.units(c.dir));
            c.ratio = ua / (ua + ub);
            c.a.equalize();
            c.b.equalize();
        }
    }
}

/// Something to draw: a tile or a tab/stack title bar.
#[derive(Debug, Clone, PartialEq)]
pub enum Placed {
    Tile(u32, Rect),
    /// Title bar of a tabbed/stacked container: titles of both children, active index.
    Bar {
        rect: Rect,
        mode: Mode,
        titles: [String; 2],
        active: u8,
        first_ids: [u32; 2],
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileLayout {
    pub root: Node,
    pub focus: u32,
    #[serde(default)]
    pub zoomed: bool,
    /// Direction for new tiles (`Ctrl-w e` toggles).
    #[serde(default)]
    pub new_dir: Dir,
    pub next_id: u32,
}

impl Default for TileLayout {
    fn default() -> Self {
        Self::single(None)
    }
}

fn split_rect(r: Rect, dir: Dir, ratio: f32) -> (Rect, Rect) {
    match dir {
        Dir::Horizontal => {
            let w = ((r.width as f32) * ratio).round() as u16;
            let w = w.min(r.width);
            (Rect { width: w, ..r }, Rect { x: r.x + w, width: r.width - w, ..r })
        }
        Dir::Vertical => {
            let h = ((r.height as f32) * ratio).round() as u16;
            let h = h.min(r.height);
            (Rect { height: h, ..r }, Rect { y: r.y + h, height: r.height - h, ..r })
        }
    }
}

fn title_of(n: &Node) -> (String, u32) {
    let t = n.tiles()[0];
    let v = match t.view {
        View::List => "list",
        View::Chart => "chart",
    };
    let s = t.series.as_ref().map(|s| s.symbol.clone()).unwrap_or_default();
    (format!("{v} {s}"), t.id)
}

fn place(n: &Node, r: Rect, out: &mut Vec<Placed>) {
    match n {
        Node::Tile(t) => out.push(Placed::Tile(t.id, r)),
        Node::Split(c) => match c.mode {
            Mode::Split => {
                let (ra, rb) = split_rect(r, c.dir, c.ratio);
                place(&c.a, ra, out);
                place(&c.b, rb, out);
            }
            Mode::Tabbed | Mode::Stacked => {
                let bar_h = if c.mode == Mode::Tabbed { 1 } else { 2 }.min(r.height);
                let (ta, ia) = title_of(&c.a);
                let (tb, ib) = title_of(&c.b);
                out.push(Placed::Bar { rect: Rect { height: bar_h, ..r }, mode: c.mode, titles: [ta, tb], active: c.active, first_ids: [ia, ib] });
                let body = Rect { y: r.y + bar_h, height: r.height - bar_h, ..r };
                place(if c.active == 0 { &c.a } else { &c.b }, body, out);
            }
        },
    }
}

impl TileLayout {
    /// A fresh session: one list tile.
    pub fn single(series: Option<SeriesKey>) -> Self {
        TileLayout { root: Node::Tile(Tile::new(1, View::List, series)), focus: 1, zoomed: false, new_dir: Dir::Horizontal, next_id: 2 }
    }

    pub fn tiles(&self) -> Vec<&Tile> {
        self.root.tiles()
    }
    pub fn tile(&self, id: u32) -> Option<&Tile> {
        self.root.tiles().into_iter().find(|t| t.id == id)
    }
    pub fn tile_mut(&mut self, id: u32) -> Option<&mut Tile> {
        self.root.tiles_mut().into_iter().find(|t| t.id == id)
    }
    pub fn focused(&self) -> &Tile {
        self.tile(self.focus).unwrap_or_else(|| self.root.tiles()[0])
    }
    pub fn focused_mut(&mut self) -> &mut Tile {
        let id = self.focus;
        if self.tile(id).is_none() {
            self.focus = self.root.tiles()[0].id;
        }
        let f = self.focus;
        self.tile_mut(f).unwrap()
    }

    /// Visible tiles and title bars inside `area`.
    pub fn placed(&self, area: Rect) -> Vec<Placed> {
        let mut out = vec![];
        if self.zoomed {
            out.push(Placed::Tile(self.focus, area));
        } else {
            place(&self.root, area, &mut out);
        }
        out
    }

    pub fn rects(&self, area: Rect) -> Vec<(u32, Rect)> {
        self.placed(area)
            .into_iter()
            .filter_map(|p| match p {
                Placed::Tile(id, r) => Some((id, r)),
                _ => None,
            })
            .collect()
    }

    fn rect_of(&self, id: u32, area: Rect) -> Option<Rect> {
        self.rects(area).into_iter().find(|(i, _)| *i == id).map(|(_, r)| r)
    }

    /// Every visible tile meets the minimum size.
    pub fn fits(&self, area: Rect) -> bool {
        self.rects(area).iter().all(|(_, r)| r.width >= MIN_W && r.height >= MIN_H)
    }

    fn parent_path(&self) -> Option<Vec<bool>> {
        let mut p = self.root.path_to(self.focus)?;
        p.pop()?;
        Some(p)
    }

    /// Mark the containers on the focus path as showing the focused branch.
    fn sync_active(&mut self) {
        let Some(path) = self.root.path_to(self.focus) else { return };
        for i in 0..path.len() {
            if let Node::Split(c) = self.root.at_mut(&path[..i]) {
                c.active = path[i] as u8;
            }
        }
        if let Some(pp) = self.parent_path()
            && let Node::Split(c) = self.root.at(&pp)
            && c.mode == Mode::Split
        {
            self.new_dir = c.dir;
        }
    }

    pub fn set_focus(&mut self, id: u32) {
        if self.tile(id).is_some() {
            self.focus = id;
            self.sync_active();
        }
    }

    /// Split the focused tile; the new tile copies its series. Refused below the minimum size.
    pub fn split(&mut self, dir: Dir, view: Option<View>, area: Rect) -> Result<u32, String> {
        let r = self.rect_of(self.focus, area).ok_or("focused tile is not visible")?;
        let ok = match dir {
            Dir::Horizontal => r.width / 2 >= MIN_W,
            Dir::Vertical => r.height / 2 >= MIN_H,
        };
        if !ok {
            return Err(format!("tile too small to split (minimum {MIN_W}×{MIN_H})"));
        }
        let path = self.root.path_to(self.focus).ok_or("no focus")?;
        let id = self.next_id;
        self.next_id += 1;
        let node = self.root.at_mut(&path);
        let old = std::mem::replace(node, Node::Tile(Tile::new(0, View::List, None)));
        let mut new = match &old {
            Node::Tile(t) => Tile { id, link: None, ..t.clone() },
            _ => unreachable!(),
        };
        if let Some(v) = view {
            new.view = v;
        }
        *node = Node::split(dir, 0.5, old, Node::Tile(new));
        self.zoomed = false;
        self.new_dir = dir;
        self.set_focus(id);
        Ok(id)
    }

    /// Close the focused tile; its sibling takes the space.
    pub fn close(&mut self) -> Result<(), String> {
        let path = self.root.path_to(self.focus).ok_or("no focus")?;
        if path.is_empty() {
            return Err("can't close the last tile".into());
        }
        let closed = self.focus;
        let (parent, last) = (&path[..path.len() - 1], path[path.len() - 1]);
        let pnode = self.root.at_mut(parent);
        let sibling = match std::mem::replace(pnode, Node::Tile(Tile::new(0, View::List, None))) {
            Node::Split(c) => {
                if last {
                    *c.a
                } else {
                    *c.b
                }
            }
            n => n,
        };
        let next = sibling.tiles()[0].id;
        *pnode = sibling;
        for t in self.root.tiles_mut() {
            if t.link == Some(closed) {
                t.link = None;
            }
        }
        self.zoomed = false;
        self.set_focus(next);
        Ok(())
    }

    /// Grow/shrink the focused tile: Right = wider, Left = narrower, Down = taller, Up = shorter.
    pub fn resize(&mut self, d: Dir4, area: Rect) -> Result<(), String> {
        let want = match d {
            Dir4::Left | Dir4::Right => Dir::Horizontal,
            Dir4::Up | Dir4::Down => Dir::Vertical,
        };
        let grow = matches!(d, Dir4::Right | Dir4::Down);
        let path = self.root.path_to(self.focus).ok_or("no focus")?;
        for i in (0..path.len()).rev() {
            let before = self.root.clone();
            if let Node::Split(c) = self.root.at_mut(&path[..i]) {
                if c.dir != want || c.mode != Mode::Split {
                    continue;
                }
                let in_a = !path[i];
                let delta = if grow == in_a { STEP } else { -STEP };
                c.ratio = (c.ratio + delta).clamp(0.05, 0.95);
                if !self.fits(area) {
                    self.root = before;
                    return Err(format!("minimum tile size is {MIN_W}×{MIN_H}"));
                }
                return Ok(());
            }
        }
        Err("no split in that direction".into())
    }

    pub fn equalize(&mut self) {
        self.root.equalize();
    }

    /// Focus the nearest visible tile in a direction.
    pub fn focus_dir(&mut self, d: Dir4, area: Rect) -> bool {
        let rects = self.rects(area);
        let Some(cur) = rects.iter().find(|(i, _)| *i == self.focus).map(|(_, r)| *r) else { return false };
        let (cx, cy) = (cur.x as i32 + cur.width as i32 / 2, cur.y as i32 + cur.height as i32 / 2);
        let best = rects
            .iter()
            .filter(|(i, r)| {
                *i != self.focus
                    && match d {
                        Dir4::Left => r.right() <= cur.x && overlaps(r.y, r.bottom(), cur.y, cur.bottom()),
                        Dir4::Right => r.x >= cur.right() && overlaps(r.y, r.bottom(), cur.y, cur.bottom()),
                        Dir4::Up => r.bottom() <= cur.y && overlaps(r.x, r.right(), cur.x, cur.right()),
                        Dir4::Down => r.y >= cur.bottom() && overlaps(r.x, r.right(), cur.x, cur.right()),
                    }
            })
            .min_by_key(|(_, r)| {
                let (x, y) = (r.x as i32 + r.width as i32 / 2, r.y as i32 + r.height as i32 / 2);
                (x - cx).abs() + (y - cy).abs()
            })
            .map(|(i, _)| *i);
        match best {
            Some(id) => {
                self.set_focus(id);
                true
            }
            None => false,
        }
    }

    pub fn focus_next(&mut self) {
        let ids: Vec<u32> = self.tiles().iter().map(|t| t.id).collect();
        let i = ids.iter().position(|i| *i == self.focus).unwrap_or(0);
        self.set_focus(ids[(i + 1) % ids.len()]);
    }

    /// Swap the focused tile with the next one (focus moves with it).
    pub fn swap(&mut self) -> Result<(), String> {
        let ids: Vec<u32> = self.tiles().iter().map(|t| t.id).collect();
        if ids.len() < 2 {
            return Err("nothing to swap with".into());
        }
        let i = ids.iter().position(|i| *i == self.focus).unwrap_or(0);
        let other = ids[(i + 1) % ids.len()];
        let pa = self.root.path_to(self.focus).unwrap();
        let pb = self.root.path_to(other).unwrap();
        let ta = self.root.at(&pa).clone();
        let tb = self.root.at(&pb).clone();
        *self.root.at_mut(&pa) = tb;
        *self.root.at_mut(&pb) = ta;
        self.set_focus(self.focus);
        Ok(())
    }

    pub fn toggle_zoom(&mut self) {
        self.zoomed = !self.zoomed;
    }

    pub fn change_view(&mut self) -> View {
        let t = self.focused_mut();
        t.view = match t.view {
            View::List => View::Chart,
            View::Chart => View::List,
        };
        t.view
    }

    pub fn toggle_split_dir(&mut self) -> Dir {
        self.new_dir = self.new_dir.flip();
        self.new_dir
    }

    /// Switch the focused tile's parent container between split, tabbed and stacked.
    pub fn set_mode(&mut self, m: Mode) -> Result<(), String> {
        let pp = self.parent_path().ok_or("a single tile has no container; split it first")?;
        if let Node::Split(c) = self.root.at_mut(&pp) {
            c.mode = m;
        }
        self.sync_active();
        Ok(())
    }

    /// Link the focused tile to the next tile (cycling), or unlink after the last one.
    pub fn cycle_link(&mut self) -> Option<u32> {
        let ids: Vec<u32> = self.tiles().iter().map(|t| t.id).filter(|i| *i != self.focus).collect();
        let t = self.focused_mut();
        t.link = match t.link {
            None => ids.first().copied(),
            Some(cur) => ids.iter().position(|i| *i == cur).and_then(|p| ids.get(p + 1).copied()),
        };
        t.link
    }

    /// Replace the tree with a preset; every tile gets `series`.
    pub fn preset(&mut self, p: Preset, series: Option<SeriesKey>) {
        let mut id = 0;
        let mut t = |v: View| {
            id += 1;
            Node::Tile(Tile::new(id, v, series.clone()))
        };
        use Dir::*;
        use View::*;
        self.root = match p {
            Preset::TwoUp => Node::split(Horizontal, 0.5, t(List), t(Chart)),
            Preset::ThreeColumn => Node::split(Horizontal, 1.0 / 3.0, t(List), Node::split(Horizontal, 0.5, t(Chart), t(Chart))),
            Preset::MainStack => Node::split(Horizontal, 0.6, t(Chart), Node::split(Vertical, 0.5, t(List), t(Chart))),
            Preset::Grid2x2 => {
                let top = Node::split(Horizontal, 0.5, t(List), t(Chart));
                let bottom = Node::split(Horizontal, 0.5, t(Chart), t(List));
                Node::split(Vertical, 0.5, top, bottom)
            }
        };
        self.next_id = id + 1;
        self.zoomed = false;
        self.set_focus(1);
    }
}

fn overlaps(a0: u16, a1: u16, b0: u16, b1: u16) -> bool {
    a0 < b1 && b0 < a1
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect { x: 0, y: 0, width: 120, height: 40 };

    fn key() -> Option<SeriesKey> {
        Some(SeriesKey { source: "binance".into(), symbol: "BTC/USDT".into(), duration: "5m".into() })
    }

    #[test]
    fn split_any_tile_at_any_depth_and_close() {
        let mut l = TileLayout::single(key());
        let b = l.split(Dir::Horizontal, Some(View::Chart), AREA).unwrap();
        assert_eq!(l.focus, b);
        assert_eq!(l.focused().view, View::Chart);
        assert_eq!(l.focused().series, key());
        let c = l.split(Dir::Vertical, None, AREA).unwrap();
        l.set_focus(1);
        let d = l.split(Dir::Vertical, None, AREA).unwrap();
        assert_eq!(l.tiles().len(), 4);
        let r = l.rects(AREA);
        assert_eq!(r.len(), 4);
        // no overlap and full coverage
        let cells: u32 = r.iter().map(|(_, r)| r.width as u32 * r.height as u32).sum();
        assert_eq!(cells, 120 * 40);
        l.set_focus(c);
        l.close().unwrap();
        assert_eq!(l.focus, b);
        l.set_focus(d);
        l.close().unwrap();
        l.close().unwrap();
        assert_eq!(l.tiles().len(), 1);
        assert!(l.close().is_err());
    }

    #[test]
    fn min_size_is_enforced() {
        let mut l = TileLayout::single(None);
        let small = Rect { x: 0, y: 0, width: 45, height: 13 };
        l.split(Dir::Horizontal, None, small).unwrap();
        assert!(l.split(Dir::Horizontal, None, small).is_err());
        l.split(Dir::Vertical, None, small).unwrap();
        assert!(l.split(Dir::Vertical, None, small).is_err());
        for _ in 0..20 {
            let _ = l.resize(Dir4::Left, small);
        }
        assert!(l.fits(small));
        assert!(l.resize(Dir4::Left, small).is_err());
    }

    #[test]
    fn resize_equalize_swap_zoom() {
        let mut l = TileLayout::single(None);
        let b = l.split(Dir::Horizontal, None, AREA).unwrap();
        l.resize(Dir4::Right, AREA).unwrap(); // focused (second) grows
        let r: Vec<_> = l.rects(AREA);
        assert!(r[1].1.width > r[0].1.width);
        l.equalize();
        let r: Vec<_> = l.rects(AREA);
        assert_eq!(r[0].1.width, r[1].1.width);
        assert!(l.resize(Dir4::Down, AREA).is_err());
        // swap moves the focused tile to the other slot
        l.swap().unwrap();
        assert_eq!(l.rects(AREA)[0].0, b);
        assert_eq!(l.focus, b);
        l.toggle_zoom();
        assert_eq!(l.rects(AREA), vec![(b, AREA)]);
        l.toggle_zoom();
        // 3 columns equalize to thirds
        l.split(Dir::Horizontal, None, AREA).unwrap();
        l.equalize();
        let w: Vec<u16> = l.rects(AREA).iter().map(|(_, r)| r.width).collect();
        assert_eq!(w, vec![40, 40, 40]);
    }

    #[test]
    fn focus_moves_geometrically() {
        let mut l = TileLayout::default();
        l.preset(Preset::Grid2x2, key());
        assert_eq!(l.focus, 1);
        assert!(l.focus_dir(Dir4::Right, AREA));
        assert_eq!(l.focus, 2);
        assert!(l.focus_dir(Dir4::Down, AREA));
        assert_eq!(l.focus, 4);
        assert!(!l.focus_dir(Dir4::Down, AREA));
        l.focus_next();
        assert_eq!(l.focus, 1);
    }

    #[test]
    fn tabbed_and_stacked_containers() {
        let mut l = TileLayout::single(None);
        assert!(l.set_mode(Mode::Tabbed).is_err());
        let b = l.split(Dir::Horizontal, None, AREA).unwrap();
        l.set_mode(Mode::Tabbed).unwrap();
        let p = l.placed(AREA);
        assert!(matches!(p[0], Placed::Bar { mode: Mode::Tabbed, active: 1, .. }));
        assert_eq!(l.rects(AREA), vec![(b, Rect { y: 1, height: 39, ..AREA })]);
        l.focus_next();
        assert_eq!(l.rects(AREA)[0].0, 1);
        l.set_mode(Mode::Stacked).unwrap();
        assert_eq!(l.rects(AREA)[0].1.y, 2);
    }

    #[test]
    fn presets_and_serialize_round_trip() {
        for p in [Preset::TwoUp, Preset::ThreeColumn, Preset::MainStack, Preset::Grid2x2] {
            let mut l = TileLayout::default();
            l.preset(p, key());
            assert!(l.fits(AREA), "{p:?}");
            let n = l.tiles().len();
            assert!(n >= 2);
            let s = toml::to_string(&l).unwrap();
            let back: TileLayout = toml::from_str(&s).unwrap();
            assert_eq!(back, l, "{s}");
        }
        let mut l = TileLayout::default();
        l.preset(Preset::Grid2x2, key());
        l.set_mode(Mode::Stacked).unwrap();
        l.focused_mut().list.sort = BarSort::ChangePct;
        l.cycle_link();
        let s = toml::to_string(&l).unwrap();
        assert_eq!(toml::from_str::<TileLayout>(&s).unwrap(), l);
    }

    #[test]
    fn links_cycle_and_clear_on_close() {
        let mut l = TileLayout::default();
        l.preset(Preset::ThreeColumn, key());
        assert_eq!(l.cycle_link(), Some(2));
        assert_eq!(l.cycle_link(), Some(3));
        assert_eq!(l.cycle_link(), None);
        l.cycle_link();
        l.set_focus(2);
        l.close().unwrap();
        assert_eq!(l.tile(1).unwrap().link, None);
    }
}
