//! Code-space charts as PNG for reports: the adapter from result views to `weirdnet-render`
//! (which draws plain charts and tables and knows nothing about code spaces).
use crate::code::view::{Axis, Line};
use weirdnet_render::{Chart, Limits, Series};

fn to_chart(x: &Axis, lines: &[Line]) -> Chart {
    Chart { x: x.values.clone(), series: lines.iter().map(|l| Series { name: l.name.clone(), values: l.values.clone() }).collect() }
}

/// Draw the lines over x (`w`×`h` pixels). `None` when nothing is finite (or it cannot be
/// drawn at that size).
pub fn chart(x: &Axis, lines: &[Line], w: usize, h: usize) -> Option<Vec<u8>> {
    weirdnet_render::chart_png(&to_chart(x, lines), w, h, &Limits::default()).ok()
}

/// The legend: line names with their colors' names.
pub fn legend(lines: &[Line]) -> String {
    weirdnet_render::legend(&Chart { x: vec![], series: lines.iter().map(|l| Series { name: l.name.clone(), values: vec![] }).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charts_draw_as_png() {
        let x = Axis { name: "row".into(), time: false, values: vec![Some(0.0), Some(1.0), Some(2.0)] };
        let l = vec![Line { name: "a".into(), values: vec![Some(1.0), None, Some(3.0)] }, Line { name: "b".into(), values: vec![Some(2.0); 3] }];
        let png = chart(&x, &l, 200, 100).unwrap();
        assert_eq!(&png[1..4], b"PNG");
        assert!(chart(&x, &[Line { name: "n".into(), values: vec![None; 3] }], 10, 10).is_none());
        assert_eq!(legend(&l), "a (blue), b (red)");
    }
}
