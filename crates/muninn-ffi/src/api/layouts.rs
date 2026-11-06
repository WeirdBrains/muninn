//! Named workspace layouts (`View ▸ Layouts ▸ Save current…` / TUI `layout.save`).
//! Stored in `preferences.toml` as `[tui.layouts]`, shared with the TUI.
use super::info::paths;
use muninn_core::model::Duration;
use muninn_core::prefs::Preferences;
use muninn_core::tui::chart::{ChartOpts, Layers};
use muninn_core::tui::frametile::FrameTile;
use muninn_core::tui::layout::{Container, Dir, Mode, Node, SeriesKey, Tile, TileLayout, View};

fn load(home: Option<String>) -> anyhow::Result<(muninn_core::paths::Paths, Preferences)> {
    let p = paths(home)?;
    Ok((p.clone(), Preferences::load_or_create(&p)?))
}

fn save(p: &muninn_core::paths::Paths, prefs: &Preferences) -> anyhow::Result<()> {
    prefs.save(p)?;
    Ok(())
}

/// Names of saved layouts, sorted like the TUI picker.
pub fn saved_layouts(home: Option<String>) -> anyhow::Result<Vec<String>> {
    let p = paths(home)?;
    let prefs = std::fs::read_to_string(p.preferences()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_else(Preferences::default);
    Ok(prefs.tui.layouts.keys().cloned().collect())
}

/// Save the current Flutter workspace as a named layout (also readable by the TUI).
pub fn save_named_layout(home: Option<String>, name: String, json: String) -> anyhow::Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow::anyhow!("a layout name"));
    }
    let layout = app_to_tile(&json)?;
    let (p, mut prefs) = load(home)?;
    prefs.tui.layouts.insert(name.to_string(), layout);
    save(&p, &prefs)?;
    Ok(format!("saved layout {name}"))
}

/// The named layout as Flutter workspace JSON.
pub fn named_layout(home: Option<String>, name: String) -> anyhow::Result<String> {
    let p = paths(home)?;
    let prefs = Preferences::load_or_create(&p)?;
    let layout = prefs.tui.layouts.get(name.trim()).ok_or_else(|| anyhow::anyhow!("no saved layout `{name}`"))?;
    tile_to_app(layout)
}

/// Remove a named layout.
pub fn remove_named_layout(home: Option<String>, name: String) -> anyhow::Result<String> {
    let (p, mut prefs) = load(home)?;
    if prefs.tui.layouts.remove(name.trim()).is_none() {
        return Err(anyhow::anyhow!("no saved layout `{name}`"));
    }
    save(&p, &prefs)?;
    Ok(format!("removed layout {name}"))
}

/// Whether the history bar is open (`[tui.history_open]`, TUI `Y`).
pub fn history_open(home: Option<String>) -> anyhow::Result<bool> {
    let p = paths(home)?;
    let prefs = std::fs::read_to_string(p.preferences()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_else(Preferences::default);
    Ok(prefs.tui.history_open)
}

/// Show or hide the history bar, shared with the TUI.
pub fn set_history_open(home: Option<String>, open: bool) -> anyhow::Result<()> {
    let (p, mut prefs) = load(home)?;
    prefs.tui.history_open = open;
    save(&p, &prefs)
}

/// The live workspace tree (`[tui.layout]`), shared with the TUI. Missing when never saved.
pub fn live_layout(home: Option<String>) -> anyhow::Result<Option<String>> {
    let prefs = Preferences::load_or_create(&paths(home)?)?;
    match prefs.tui.layout.as_ref() {
        Some(l) => Ok(Some(tile_to_app(l)?)),
        None => Ok(None),
    }
}

/// Write the live workspace tree to `[tui.layout]`, like the TUI after every layout change.
pub fn save_live_layout(home: Option<String>, json: String) -> anyhow::Result<String> {
    let layout = app_to_tile(&json)?;
    let (p, mut prefs) = load(home)?;
    prefs.tui.layout = Some(layout);
    save(&p, &prefs)?;
    Ok("saved layout".into())
}

fn app_to_tile(json: &str) -> anyhow::Result<TileLayout> {
    let app: serde_json::Value = serde_json::from_str(json)?;
    Ok(TileLayout {
        root: node_from_json(app.get("root").unwrap_or(&serde_json::Value::Null))?,
        focus: app.get("focus").and_then(|x| x.as_u64()).unwrap_or(1) as u32,
        zoomed: app.get("zoomed").and_then(|x| x.as_bool()).unwrap_or(false),
        new_dir: parse_dir(app.get("newDir").and_then(|x| x.as_str()).unwrap_or("horizontal")),
        next_id: app.get("nextId").and_then(|x| x.as_u64()).unwrap_or(2) as u32,
    })
}

fn tile_to_app(l: &TileLayout) -> anyhow::Result<String> {
    Ok(serde_json::to_string(&serde_json::json!({
        "focus": l.focus,
        "nextId": l.next_id,
        "zoomed": l.zoomed,
        "newDir": dir_name(l.new_dir),
        "root": node_to_json(&l.root),
    }))?)
}

fn node_from_json(v: &serde_json::Value) -> anyhow::Result<Node> {
    let ty = v.get("type").and_then(|x| x.as_str()).unwrap_or("tile");
    if ty == "split" {
        let dir = parse_dir(v.get("dir").and_then(|x| x.as_str()).unwrap_or("horizontal"));
        let mode = parse_mode(v.get("mode").and_then(|x| x.as_str()).unwrap_or("split"));
        let ratio = v.get("ratio").and_then(|x| x.as_f64()).unwrap_or(0.5) as f32;
        let active = v.get("active").and_then(|x| x.as_u64()).unwrap_or(0) as u8;
        let a = v.get("a").ok_or_else(|| anyhow::anyhow!("split missing a"))?;
        let b = v.get("b").ok_or_else(|| anyhow::anyhow!("split missing b"))?;
        return Ok(Node::Split(Container { dir, mode, ratio, active, a: Box::new(node_from_json(a)?), b: Box::new(node_from_json(b)?) }));
    }
    let id = v.get("id").and_then(|x| x.as_u64()).unwrap_or(1) as u32;
    let as_chart = v.get("asChart").and_then(|x| x.as_bool()).unwrap_or(true);
    let key = v.get("datasetKey").and_then(|x| x.as_str()).filter(|s| !s.is_empty());
    let link = v.get("link").and_then(|x| x.as_u64()).map(|n| n as u32);
    let mut chart = ChartOpts { zoom: scale_to_zoom(v.get("viewportScale").and_then(|x| x.as_f64()).unwrap_or(1.0)), ..Default::default() };
    if let Some(layers) = v.get("layers") {
        chart.layers = layers_from_json(layers);
    }
    Ok(Node::Tile(Tile {
        id,
        view: if as_chart { View::Chart } else { View::List },
        series: key.and_then(key_to_series),
        link,
        list: Default::default(),
        chart,
        frame: frame_from_json(v.get("frame")),
    }))
}

fn node_to_json(n: &Node) -> serde_json::Value {
    match n {
        Node::Tile(t) => {
            let mut m = serde_json::Map::new();
            m.insert("type".into(), serde_json::Value::String("tile".into()));
            m.insert("id".into(), t.id.into());
            m.insert("asChart".into(), (t.view == View::Chart).into());
            if let Some(s) = &t.series {
                m.insert("datasetKey".into(), series_to_key(s).into());
            }
            if let Some(link) = t.link {
                m.insert("link".into(), link.into());
            }
            if let Some(f) = t.frame.as_deref() {
                m.insert("frame".into(), frame_to_json(f));
            }
            m.insert("layers".into(), layers_to_json(&t.chart.layers));
            m.insert("viewportScale".into(), (t.chart.zoom as f64).into());
            m.insert("viewportStart".into(), 0.into());
            serde_json::Value::Object(m)
        }
        Node::Split(c) => serde_json::json!({
            "type": "split",
            "dir": dir_name(c.dir),
            "mode": mode_name(c.mode),
            "ratio": c.ratio,
            "active": c.active,
            "a": node_to_json(&c.a),
            "b": node_to_json(&c.b),
        }),
    }
}

fn frame_from_json(v: Option<&serde_json::Value>) -> Option<Box<FrameTile>> {
    let v = v?;
    let key = v.get("key").and_then(|x| x.as_str()).unwrap_or("").trim();
    if key.is_empty() {
        return None;
    }
    let columns =
        v.get("columns").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    Some(Box::new(FrameTile { key: key.to_string(), table: v.get("table").and_then(|x| x.as_bool()).unwrap_or(false), columns }))
}

fn frame_to_json(f: &FrameTile) -> serde_json::Value {
    serde_json::json!({
        "key": f.key,
        "table": f.table,
        "columns": f.columns,
    })
}

fn key_to_series(key: &str) -> Option<SeriesKey> {
    let (source, rest) = key.split_once(':')?;
    let (symbol, dur) = rest.rsplit_once(':')?;
    let secs: i64 = dur.parse().ok()?;
    Some(SeriesKey { source: source.into(), symbol: symbol.into(), duration: Duration(secs).to_string() })
}

fn series_to_key(s: &SeriesKey) -> String {
    let secs = Duration::parse(&s.duration).map(|d| d.secs()).unwrap_or(0);
    format!("{}:{}:{secs}", s.source, s.symbol)
}

fn scale_to_zoom(scale: f64) -> u32 {
    let z = scale.round() as u32;
    if z < 1 { 1 } else { z }
}

fn parse_dir(s: &str) -> Dir {
    if s.eq_ignore_ascii_case("vertical") { Dir::Vertical } else { Dir::Horizontal }
}

fn dir_name(d: Dir) -> &'static str {
    match d {
        Dir::Horizontal => "horizontal",
        Dir::Vertical => "vertical",
    }
}

fn parse_mode(s: &str) -> Mode {
    match s {
        "tabbed" => Mode::Tabbed,
        "stacked" => Mode::Stacked,
        _ => Mode::Split,
    }
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::Split => "split",
        Mode::Tabbed => "tabbed",
        Mode::Stacked => "stacked",
    }
}

fn layers_from_json(v: &serde_json::Value) -> Layers {
    let mut l = Layers::default();
    let get = |k: &str| v.get(k).and_then(|x| x.as_bool());
    if let Some(x) = get("candles") {
        l.candles = x;
    }
    if let Some(x) = get("ohlc") {
        l.ohlc = x;
    }
    if let Some(x) = get("line") {
        l.line = x;
    }
    if let Some(x) = get("area") {
        l.area = x;
    }
    if let Some(x) = get("envelope") {
        l.envelope = x;
    }
    if let Some(x) = get("volume") {
        l.volume = x;
    }
    if let Some(x) = get("ma") {
        l.ma = x;
    }
    if let Some(x) = get("wicks") {
        l.wicks = x;
    }
    if let Some(x) = get("gaps") {
        l.gaps = x;
    }
    if let Some(x) = get("zeroVolume") {
        l.zero_volume = x;
    }
    if let Some(x) = get("grid") {
        l.grid = x;
    }
    if let Some(x) = get("crosshair") {
        l.crosshair = x;
    }
    if let Some(x) = get("priceAxis") {
        l.price_axis = x;
    }
    if let Some(x) = get("timeAxis") {
        l.time_axis = x;
    }
    l
}

fn layers_to_json(l: &Layers) -> serde_json::Value {
    serde_json::json!({
        "candles": l.candles,
        "ohlc": l.ohlc,
        "line": l.line,
        "area": l.area,
        "envelope": l.envelope,
        "volume": l.volume,
        "ma": l.ma,
        "wicks": l.wicks,
        "gaps": l.gaps,
        "zeroVolume": l.zero_volume,
        "grid": l.grid,
        "crosshair": l.crosshair,
        "priceAxis": l.price_axis,
        "timeAxis": l.time_axis,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_layouts_round_trip_through_prefs_without_touching_home() {
        let dir = tempfile::tempdir().unwrap();
        let home = Some(dir.path().display().to_string());
        assert!(saved_layouts(home.clone()).unwrap().is_empty());
        assert!(!dir.path().join("preferences.toml").exists() || saved_layouts(home.clone()).unwrap().is_empty());
        let json = r#"{"focus":1,"nextId":3,"zoomed":false,"newDir":"horizontal","root":{"type":"split","dir":"horizontal","ratio":0.5,"mode":"split","active":0,"a":{"type":"tile","id":1,"asChart":true,"datasetKey":"binance:BTC/USDT:300","layers":{"envelope":true,"volume":false}},"b":{"type":"tile","id":2,"asChart":false,"datasetKey":"binance:ETH/USDT:3600","link":1,"frame":{"key":"monitor:feed"}}}}"#;
        assert_eq!(save_named_layout(home.clone(), "quad".into(), json.into()).unwrap(), "saved layout quad");
        assert_eq!(saved_layouts(home.clone()).unwrap(), vec!["quad"]);
        let loaded = named_layout(home.clone(), "quad".into()).unwrap();
        assert!(loaded.contains("BTC/USDT") && loaded.contains("ETH/USDT") && loaded.contains("monitor:feed"), "{loaded}");
        assert!(loaded.contains("\"asChart\":true") && loaded.contains("\"link\":1"), "{loaded}");
        // shared with the TUI
        let prefs = Preferences::load_or_create(&paths(home.clone()).unwrap()).unwrap();
        let t = prefs.tui.layouts.get("quad").unwrap();
        assert_eq!(t.tiles().len(), 2);
        assert_eq!(t.tile(2).unwrap().link, Some(1));
        assert_eq!(t.tile(2).unwrap().frame.as_ref().map(|f| f.key.as_str()), Some("monitor:feed"));
        assert!(t.tile(1).unwrap().chart.layers.envelope);
        assert!(!t.tile(1).unwrap().chart.layers.volume);
        assert_eq!(t.tile(1).unwrap().series.as_ref().unwrap().duration, "5m");
        assert_eq!(remove_named_layout(home.clone(), "quad".into()).unwrap(), "removed layout quad");
        assert!(saved_layouts(home).unwrap().is_empty());
    }

    #[test]
    fn history_open_persists_in_tui_prefs() {
        let dir = tempfile::tempdir().unwrap();
        let home = Some(dir.path().display().to_string());
        assert!(!history_open(home.clone()).unwrap());
        set_history_open(home.clone(), true).unwrap();
        assert!(history_open(home).unwrap());
    }

    #[test]
    fn live_layout_round_trips_through_tui_layout_pref() {
        let dir = tempfile::tempdir().unwrap();
        let home = Some(dir.path().display().to_string());
        assert!(live_layout(home.clone()).unwrap().is_none());
        let json = r#"{"focus":2,"nextId":3,"zoomed":false,"newDir":"horizontal","root":{"type":"split","dir":"horizontal","ratio":0.5,"mode":"split","active":0,"a":{"type":"tile","id":1,"asChart":true,"datasetKey":"binance:BTC/USDT:300"},"b":{"type":"tile","id":2,"asChart":false,"frame":{"key":"code:mean"}}}}"#;
        assert_eq!(save_live_layout(home.clone(), json.into()).unwrap(), "saved layout");
        let loaded = live_layout(home.clone()).unwrap().expect("saved");
        assert!(loaded.contains("code:mean") && loaded.contains("BTC/USDT"), "{loaded}");
        let prefs = Preferences::load_or_create(&paths(home).unwrap()).unwrap();
        let t = prefs.tui.layout.expect("tui.layout");
        assert_eq!(t.focus, 2);
        assert_eq!(t.tile(2).unwrap().frame.as_ref().map(|f| f.key.as_str()), Some("code:mean"));
    }
}
