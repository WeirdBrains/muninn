//! App-level chart navigation and tile linking.
use super::layout::View;
use super::App;
use crate::ingest::time::parse_ts;

impl App {
    /// Data bounds (first, last bar) of a series.
    pub fn bounds(&self, k: Option<&super::layout::SeriesKey>) -> Option<(i64, i64)> {
        let k = k?;
        self.series.iter().find(|s| super::layout::SeriesKey::of(s) == *k).map(|s| (s.first, s.last))
    }

    fn chart_geom(&mut self, id: u32) -> super::tiles::ChartCache {
        self.caches.entry(id).or_default().chart.clone()
    }

    pub fn chart_set_end(&mut self, id: u32, end: Option<i64>) {
        let c = self.chart_geom(id);
        let end = end.map(|e| e.clamp(c.data_first, c.data_last));
        let live = end.is_none_or(|e| e >= c.data_last);
        if let Some(t) = self.layout.tile_mut(id) {
            t.chart.end = if live { None } else { end };
        }
        self.layout_changed();
        self.propagate_from_chart(id);
    }

    /// Pan by a fraction of the visible span (negative = earlier).
    pub fn chart_pan(&mut self, id: u32, frac: f64) {
        let c = self.chart_geom(id);
        let step = (frac.signum() as i64) * ((c.width as f64 * frac.abs()) as i64).max(1) * c.bucket_ms;
        self.chart_set_end(id, Some(c.end + step));
    }

    /// Zoom in (fewer bars per column) or out, keeping the centre.
    pub fn chart_zoom(&mut self, id: u32, zoom_in: bool) {
        let c = self.chart_geom(id);
        let Some(t) = self.layout.tile_mut(id) else { return };
        let old = t.chart.zoom.max(1);
        let new = if zoom_in { (old / 2).max(1) } else { (old * 2).min(1 << 20) };
        if new == old {
            return;
        }
        t.chart.zoom = new;
        let centre = (c.start + c.end) / 2;
        let half = (c.width as i64 / 2) * c.dur_ms * new as i64;
        let was_live = t.chart.end.is_none();
        self.set_status(format!("zoom: {new} bar(s) per column"));
        if was_live && !zoom_in {
            self.layout_changed();
            return self.propagate_from_chart(id);
        }
        self.chart_set_end(id, Some(centre + half));
    }

    pub fn chart_cursor(&mut self, id: u32, step: i64) {
        let c = &mut self.caches.entry(id).or_default().chart;
        let v = c.visible().to_vec();
        if v.is_empty() {
            return;
        }
        let i = c.cursor_index().unwrap_or(v.len() - 1) as i64 + step;
        if i < 0 {
            c.cursor = Some(v[0].ts - c.bucket_ms);
            self.chart_pan(id, -0.25);
        } else if i as usize >= v.len() {
            c.cursor = Some(v[v.len() - 1].ts + c.bucket_ms);
            self.chart_pan(id, 0.25);
        } else {
            c.cursor = Some(v[i as usize].ts);
            self.propagate_from_chart(id);
        }
    }

    /// Set the period start, keeping the end (adjusts zoom to fit).
    pub fn chart_set_start(&mut self, id: u32, start: i64) {
        let c = self.chart_geom(id);
        if start >= c.end {
            return self.set_status("start must be before end");
        }
        let cols = c.width.max(1) as i64;
        let zoom = (((c.end - start) as f64 / (c.dur_ms * cols) as f64).ceil() as u32).max(1);
        if let Some(t) = self.layout.tile_mut(id) {
            t.chart.zoom = zoom;
        }
        let end = c.end;
        self.chart_set_end(id, Some(end));
    }

    /// Period selector arrows.
    pub fn chart_nav(&mut self, id: u32, which: u8) {
        let c = self.chart_geom(id);
        let step = (c.end - c.start) / 4;
        match which {
            0 => self.chart_set_start(id, c.start - step),
            1 => self.chart_set_start(id, c.start + step),
            2 => self.chart_set_end(id, Some(c.end - step)),
            _ => self.chart_set_end(id, Some(c.end + step)),
        }
    }

    pub fn chart_prompt(&mut self, kind: super::modal::PromptKind, input: &str) {
        use super::modal::PromptKind::*;
        let id = self.layout.focus;
        if kind == Mas {
            match super::chart::Ma::parse_list(input) {
                Ok(m) => {
                    let t = self.layout.focused_mut();
                    t.chart.mas = m;
                    t.chart.layers.ma = true;
                    self.layout_changed();
                    self.set_status("moving averages updated");
                }
                Err(e) => self.set_status(e),
            }
            return;
        }
        let Some(ts) = parse_ts(input) else { return self.set_status(format!("can't read date `{input}`")) };
        let c = self.chart_geom(id);
        match kind {
            ChartStart => self.chart_set_start(id, ts),
            ChartEnd => self.chart_set_end(id, Some(ts)),
            JumpDate => {
                self.caches.entry(id).or_default().chart.cursor = Some(ts);
                self.chart_set_end(id, Some(ts + (c.width as i64 / 2) * c.bucket_ms));
            }
            _ => {}
        }
    }

    fn linked(&self, id: u32) -> Vec<u32> {
        let me = self.layout.tile(id).and_then(|t| t.link);
        self.layout.tiles().iter().filter(|t| t.id != id && (t.link == Some(id) || me == Some(t.id))).map(|t| t.id).collect()
    }

    /// List selection centres linked charts on the selected bar.
    pub fn propagate_from_list(&mut self, id: u32) {
        if self.layout.tile(id).map(|t| t.view) != Some(View::List) {
            return;
        }
        let Some(ts) = self.caches.get(&id).and_then(|c| c.list.selected()).map(|b| b.ts) else { return };
        for other in self.linked(id) {
            if self.layout.tile(other).map(|t| t.view) != Some(View::Chart) {
                continue;
            }
            let c = self.chart_geom(other);
            let end = ts + (c.width as i64 / 2) * c.bucket_ms.max(1);
            let live = end >= c.data_last;
            if let Some(t) = self.layout.tile_mut(other) {
                t.chart.end = if live { None } else { Some(end) };
            }
            self.caches.entry(other).or_default().chart.cursor = Some(ts);
        }
    }

    /// Chart window/crosshair scrolls linked time-sorted lists.
    pub fn propagate_from_chart(&mut self, id: u32) {
        let Some(t) = self.layout.tile(id).cloned() else { return };
        if t.view != View::Chart {
            return;
        }
        let c = self.chart_geom(id);
        let target = c.cursor.unwrap_or(c.start);
        for other in self.linked(id) {
            let Some(lt) = self.layout.tile(other).cloned() else { continue };
            if lt.view != View::List || lt.list.sort != crate::store::BarSort::Time {
                continue;
            }
            let Some(s) = &lt.series else { continue };
            let Some(d) = self.data.get(s, &self.store) else { continue };
            let r = d.range(self.scope());
            let sl = &d.bars[r];
            // Same as Store::time_index: bars before the target in the list's order.
            let i = if lt.list.desc { (sl.len() - sl.partition_point(|b| b.ts <= target)) as i64 } else { sl.partition_point(|b| b.ts < target) as i64 };
            {
                let lc = &mut self.caches.entry(other).or_default().list;
                lc.sel = i;
                lc.top = i;
            }
        }
    }
}
