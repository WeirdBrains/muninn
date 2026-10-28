//! Header: MUNINN in block letters, version and status.
use crate::theme::Theme;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

const M: [&str; 6] = ["███╗   ███╗", "████╗ ████║", "██╔████╔██║", "██║╚██╔╝██║", "██║ ╚═╝ ██║", "╚═╝     ╚═╝"];
const U: [&str; 6] = ["██╗   ██╗", "██║   ██║", "██║   ██║", "██║   ██║", "╚██████╔╝", " ╚═════╝ "];
const N: [&str; 6] = ["███╗   ██╗", "████╗  ██║", "██╔██╗ ██║", "██║╚██╗██║", "██║ ╚████║", "╚═╝  ╚═══╝"];
const I: [&str; 6] = ["██╗", "██║", "██║", "██║", "██║", "╚═╝"];

/// The six banner rows for MUNINN.
pub fn banner() -> Vec<String> {
    (0..6).map(|r| [M[r], U[r], N[r], I[r], N[r], N[r]].join("")).collect()
}

pub const LARGE_HEIGHT: u16 = 6;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Decide the header height: large needs >= 30 rows and >= 80 columns unless forced.
pub fn height(mode: crate::prefs::HeaderMode, area: Rect) -> u16 {
    use crate::prefs::HeaderMode::*;
    match mode {
        Compact => 1,
        Large => LARGE_HEIGHT,
        Auto => {
            if area.height >= 30 && area.width >= 80 {
                LARGE_HEIGHT
            } else {
                1
            }
        }
    }
}

pub fn draw(f: &mut Frame, area: Rect, t: &Theme, status: &[String]) {
    let hs = Style::new().fg(t.header).bg(t.bg).add_modifier(Modifier::BOLD);
    let ms = Style::new().fg(t.muted).bg(t.bg);
    let ver = format!("v{}", version());
    if area.height < LARGE_HEIGHT {
        let mut spans = vec![Span::styled(" Muninn ", hs), Span::styled(ver, Style::new().fg(t.accent).bg(t.bg))];
        spans.push(Span::styled(format!("   {}", status.join(" · ")), ms));
        f.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }
    let rows = banner();
    let bw = rows[0].chars().count() as u16 + 1;
    let lines: Vec<Line> = rows.iter().map(|r| Line::styled(format!(" {r}"), hs)).collect();
    f.render_widget(Paragraph::new(lines), Rect { width: bw.min(area.width), ..area });
    if area.width > bw + 4 {
        let right = Rect { x: area.x + bw + 2, width: area.width - bw - 3, ..area };
        let mut info = vec![Line::from(Span::styled(ver, Style::new().fg(t.accent).bg(t.bg).add_modifier(Modifier::BOLD)))];
        info.push(Line::from(""));
        for s in status {
            info.push(Line::styled(s.clone(), ms));
        }
        f.render_widget(Paragraph::new(info).alignment(ratatui::layout::Alignment::Right), right);
    }
}
