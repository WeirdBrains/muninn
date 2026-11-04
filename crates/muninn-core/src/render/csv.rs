//! CSV for report and project data.

/// One CSV field (quoted when needed).
pub fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// One CSV line (with a trailing newline).
pub fn line<S: AsRef<str>>(cells: &[S]) -> String {
    cells.iter().map(|c| field(c.as_ref())).collect::<Vec<_>>().join(",") + "\n"
}

/// Parse CSV text into rows (quotes, doubled quotes and newlines in quotes).
pub fn parse(s: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let mut row = vec![];
    let mut cur = String::new();
    let mut q = false;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match (q, c) {
            (true, '"') if it.peek() == Some(&'"') => {
                cur.push('"');
                it.next();
            }
            (true, '"') => q = false,
            (true, _) => cur.push(c),
            (false, '"') if cur.is_empty() => q = true,
            (false, ',') => row.push(std::mem::take(&mut cur)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut cur));
                rows.push(std::mem::take(&mut row));
            }
            (false, _) => cur.push(c),
        }
    }
    if !cur.is_empty() || !row.is_empty() {
        row.push(cur);
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_round_trips() {
        let cells = ["a", "b,c", "say \"hi\"", "two\nlines", " pad"];
        let l = line(&cells);
        assert_eq!(l, "a,\"b,c\",\"say \"\"hi\"\"\",\"two\nlines\",\" pad\"\n");
        assert_eq!(parse(&l), vec![cells.iter().map(|s| s.to_string()).collect::<Vec<_>>()]);
    }
}
