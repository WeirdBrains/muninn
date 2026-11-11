//! Tables: a header row on grey, a grid, and cell text in an 8x8 bitmap font (cells that read
//! as numbers are right-aligned).
use crate::canvas::Canvas;
use crate::{Error, Limits, Result, Table, check_size};
use font8x8::{BASIC_FONTS, BLOCK_FONTS, BOX_FONTS, GREEK_FONTS, HIRAGANA_FONTS, LATIN_FONTS, UnicodeFonts};

const GLYPH: usize = 8;
const PAD: usize = 4;
const ROW: usize = GLYPH + 2 * PAD;
const INK: [u8; 3] = [20, 20, 20];
const GRID: [u8; 3] = [200, 200, 200];
const HEAD: [u8; 3] = [235, 235, 235];

/// The glyph drawn for a character: its own, or `?` (controls and characters the font lacks).
pub(crate) fn glyph(c: char) -> [u8; 8] {
    let c = if c == '\t' { ' ' } else { c };
    if c.is_control() {
        return BASIC_FONTS.get('?').unwrap_or_default();
    }
    BASIC_FONTS
        .get(c)
        .or_else(|| LATIN_FONTS.get(c))
        .or_else(|| GREEK_FONTS.get(c))
        .or_else(|| BOX_FONTS.get(c))
        .or_else(|| BLOCK_FONTS.get(c))
        .or_else(|| HIRAGANA_FONTS.get(c))
        .unwrap_or_else(|| BASIC_FONTS.get('?').unwrap_or_default())
}

/// The characters drawn for a cell: line breaks as spaces, at most `max` (a cut cell ends in `~`).
pub(crate) fn cell_text(s: &str, max: usize) -> Vec<char> {
    let mut c: Vec<char> = s.chars().map(|c| if c == '\n' || c == '\r' { ' ' } else { c }).collect();
    if c.len() > max {
        c.truncate(max.saturating_sub(1));
        if max > 0 {
            c.push('~');
        }
    }
    c
}

fn numeric(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t.parse::<f64>().is_ok()
}

fn check(t: &Table, l: &Limits) -> Result<()> {
    if t.columns.is_empty() {
        return Err(Error::Invalid("a table needs at least one column".into()));
    }
    if t.columns.len() > l.max_columns {
        return Err(Error::TooLarge(format!("{} columns (at most {})", t.columns.len(), l.max_columns)));
    }
    if t.rows.len() > l.max_rows {
        return Err(Error::TooLarge(format!("{} rows (at most {})", t.rows.len(), l.max_rows)));
    }
    if let Some(i) = t.rows.iter().position(|r| r.len() != t.columns.len()) {
        return Err(Error::Invalid(format!("row {} has {} cells for {} columns", i + 1, t.rows[i].len(), t.columns.len())));
    }
    Ok(())
}

/// Draw a table as PNG; its size follows from the text (checked against the limits first).
pub fn table_png(t: &Table, limits: &Limits) -> Result<Vec<u8>> {
    check(t, limits)?;
    let head: Vec<Vec<char>> = t.columns.iter().map(|c| cell_text(c, limits.max_cell_chars)).collect();
    let body: Vec<Vec<Vec<char>>> = t.rows.iter().map(|r| r.iter().map(|c| cell_text(c, limits.max_cell_chars)).collect()).collect();
    let widths: Vec<usize> =
        (0..head.len()).map(|j| body.iter().map(|r| r[j].len()).chain([head[j].len()]).max().unwrap_or(0) * GLYPH + 2 * PAD).collect();
    let w = widths.iter().sum::<usize>() + 1;
    let h = (body.len() + 1) * ROW + 1;
    check_size(w, h, limits)?;
    let mut c = Canvas::new(w, h, [255; 3]);
    c.fill(0, 0, w, ROW, HEAD);
    let mut y = 0;
    for (r, cells) in std::iter::once(&head).chain(body.iter()).enumerate() {
        let mut x = 0;
        for (j, text) in cells.iter().enumerate() {
            let right = r > 0 && numeric(&t.rows[r - 1][j]);
            let start = if right { x + widths[j] - PAD - text.len() * GLYPH } else { x + PAD };
            for (k, ch) in text.iter().enumerate() {
                c.glyph(start + k * GLYPH, y + PAD, glyph(*ch), INK);
            }
            x += widths[j];
        }
        y += ROW;
    }
    for r in 0..=body.len() + 1 {
        c.hline(0, w - 1, r * ROW, GRID);
    }
    let mut x = 0;
    for wd in std::iter::once(0).chain(widths.iter().copied()) {
        x += wd;
        c.vline(x, 0, h - 1, GRID);
    }
    c.png()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(b: &[u8]) -> (usize, usize, Vec<u8>) {
        let mut r = ::png::Decoder::new(std::io::Cursor::new(b)).read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        let i = r.next_frame(&mut buf).unwrap();
        buf.truncate(i.buffer_size());
        (i.width as usize, i.height as usize, buf)
    }

    fn t(cols: &[&str], rows: &[&[&str]]) -> Table {
        Table {
            columns: cols.iter().map(|s| s.to_string()).collect(),
            rows: rows.iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect(),
        }
    }

    #[test]
    fn a_table_is_a_grid_of_text() {
        let tb = t(&["name", "temp"], &[&["north", "21.5"], &["south", "-3"]]);
        let (w, h, px) = decode(&table_png(&tb, &Limits::default()).unwrap());
        // name/north: 5 chars, temp/21.5: 4 chars; 3 rows of 16 px, 1 px closing grid line
        assert_eq!((w, h), (5 * 8 + 8 + 4 * 8 + 8 + 1, 3 * 16 + 1));
        let at = |x: usize, y: usize| px[(y * w + x) * 3..][..3].to_vec();
        assert_eq!(at(0, 0), GRID.to_vec());
        assert_eq!(at(2, 2), HEAD.to_vec(), "header background");
        assert_eq!(at(2, 18), vec![255; 3], "body background");
        let ink =
            |x0: usize, y0: usize| (y0..y0 + 8).flat_map(|y| (x0..x0 + 8).map(move |x| (x, y))).filter(|&(x, y)| at(x, y) == INK.to_vec()).count();
        // 'n' of "name" is drawn at the header's padding; "-3" is right-aligned in its column
        assert!(ink(4, 4) > 0);
        let col2 = 48;
        assert_eq!(ink(col2 + 4, 36), 0, "the number sits at the right, not the left");
        assert!(ink(col2 + 40 - 4 - 8, 36) > 0, "its last digit ends at the right padding");
    }

    #[test]
    fn same_table_same_bytes_and_text_matters() {
        let l = Limits::default();
        let a = table_png(&t(&["a"], &[&["1"]]), &l).unwrap();
        assert_eq!(a, table_png(&t(&["a"], &[&["1"]]), &l).unwrap());
        assert_ne!(a, table_png(&t(&["a"], &[&["2"]]), &l).unwrap());
        assert_eq!(&a[1..4], b"PNG");
    }

    #[test]
    fn malformed_tables_and_limits_are_errors() {
        let l = Limits::default();
        assert!(matches!(table_png(&Table::default(), &l), Err(Error::Invalid(_))));
        assert_eq!(table_png(&t(&["a", "b"], &[&["1"]]), &l), Err(Error::Invalid("row 1 has 1 cells for 2 columns".into())));
        assert!(matches!(table_png(&t(&["a", "b"], &[]), &Limits { max_columns: 1, ..l }), Err(Error::TooLarge(_))));
        assert!(matches!(table_png(&t(&["a"], &[&["1"], &["2"]]), &Limits { max_rows: 1, ..l }), Err(Error::TooLarge(_))));
        assert!(matches!(table_png(&t(&["a"], &[&["1"]]), &Limits { max_pixels: 10, ..l }), Err(Error::TooLarge(_))));
        assert!(matches!(table_png(&t(&["a"], &[&["1"]]), &Limits { max_height: 20, ..l }), Err(Error::TooLarge(_))));
        // a huge cell is cut to the cell limit, so the image stays bounded
        let long = "x".repeat(100_000);
        let (w, _, _) = decode(&table_png(&t(&["a"], &[&[long.as_str()]]), &l).unwrap());
        assert_eq!(w, 80 * 8 + 8 + 1);
    }

    #[test]
    fn cells_are_cut_and_unknown_characters_shown() {
        assert_eq!(cell_text("abcdef", 4).iter().collect::<String>(), "abc~");
        assert_eq!(cell_text("a\nb", 9).iter().collect::<String>(), "a b");
        assert_eq!(cell_text("abc", 0), Vec::<char>::new());
        assert_eq!(glyph('\u{7}'), glyph('?'));
        assert_eq!(glyph('\u{1F600}'), glyph('?'));
        assert_ne!(glyph('é'), glyph('?'));
        assert_ne!(glyph('A'), glyph('?'));
    }
}
