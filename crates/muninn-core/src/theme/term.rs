//! Terminal rendering of a theme (ratatui colors by color depth). The only part of the
//! theme module that depends on ratatui.
use super::{VariantDef, dist, nearest_256};
use ratatui::style::Color;

/// Terminal color depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Ansi256,
    Ansi16,
}

impl ColorDepth {
    pub fn detect() -> Self {
        let ct = std::env::var("COLORTERM").unwrap_or_default().to_ascii_lowercase();
        if ct.contains("truecolor") || ct.contains("24bit") {
            return ColorDepth::TrueColor;
        }
        let term = std::env::var("TERM").unwrap_or_default();
        if term == "linux" || term.contains("16color") || term == "xterm" || term == "vt100" {
            ColorDepth::Ansi16
        } else if term.contains("256") {
            ColorDepth::Ansi256
        } else {
            ColorDepth::TrueColor
        }
    }
}

const ANSI16: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (128, 0, 0),
    (0, 128, 0),
    (128, 128, 0),
    (0, 0, 128),
    (128, 0, 128),
    (0, 128, 128),
    (192, 192, 192),
    (128, 128, 128),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (0, 0, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

pub fn to_color(c: (u8, u8, u8), depth: ColorDepth) -> Color {
    match depth {
        ColorDepth::TrueColor => Color::Rgb(c.0, c.1, c.2),
        ColorDepth::Ansi256 => Color::Indexed(nearest_256(c)),
        ColorDepth::Ansi16 => {
            let i = ANSI16.iter().enumerate().min_by_key(|(_, a)| dist(c, **a)).map(|(i, _)| i).unwrap_or(7);
            Color::Indexed(i as u8)
        }
    }
}

/// A resolved variant ready for rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub muted: Color,
    pub border: Color,
    pub border_focus: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub accent: Color,
    pub header: Color,
    pub up: Color,
    pub down: Color,
    pub warning: Color,
    pub error: Color,
    pub success: Color,
    pub info: Color,
    pub charts: [Color; 6],
}

impl Theme {
    pub fn from_variant(v: &VariantDef) -> Self {
        Self::from_variant_depth(v, ColorDepth::detect())
    }

    pub fn from_variant_depth(v: &VariantDef, depth: ColorDepth) -> Self {
        let c = |r: &str| to_color(v.resolve(r).unwrap_or((128, 128, 128)), depth);
        Theme {
            bg: c("bg"),
            fg: c("fg"),
            muted: c("muted"),
            border: c("border"),
            border_focus: c("border_focus"),
            selection_bg: c("selection_bg"),
            selection_fg: c("selection_fg"),
            accent: c("accent"),
            header: c("header"),
            up: c("up"),
            down: c("down"),
            warning: c("warning"),
            error: c("error"),
            success: c("success"),
            info: c("info"),
            charts: [c("chart_1"), c("chart_2"), c("chart_3"), c("chart_4"), c("chart_5"), c("chart_6")],
        }
    }
}
