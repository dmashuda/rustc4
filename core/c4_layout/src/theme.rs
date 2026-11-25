//! Colors. A model names them (`primary`, `green`, or one it defined); a
//! diagram carries the actual values, so no renderer has a palette.

use std::collections::BTreeMap;

/// What one element color means on the page.
#[derive(Debug, Clone, PartialEq)]
pub struct Swatch {
    pub fill: String,
    pub stroke: String,
    /// Text on the fill: a title, and smaller print.
    pub text: String,
    pub text_soft: String,
}

fn swatch(fill: &str, stroke: &str, text: &str, text_soft: &str) -> Swatch {
    Swatch { fill: fill.into(), stroke: stroke.into(), text: text.into(), text_soft: text_soft.into() }
}

/// Lines and the text beside them, and group titles, are drawn on the page
/// itself, which may be light or dark: a middle gray reads on both.
pub const LINE_GRAY: &str = "#8a919e";
pub const LABEL_GRAY: &str = "#737a87";

pub fn element(name: &str, custom: &BTreeMap<String, String>) -> Swatch {
    match name {
        "primary" | "blue" => swatch("#3b82f6", "#2563eb", "#eff6ff", "#bfdbfe"),
        "secondary" | "sky" => swatch("#0284c7", "#0369a1", "#f0f9ff", "#b6ecf7"),
        "muted" | "slate" => swatch("#64748b", "#475569", "#f8fafc", "#cbd5e1"),
        "indigo" => swatch("#6366f1", "#4f46e5", "#eef2ff", "#c7d2fe"),
        "red" => swatch("#ac4d39", "#853a2d", "#fbd3cb", "#f5b2a3"),
        "green" => swatch("#428a4f", "#2d5d39", "#f8fafc", "#c2f0c2"),
        "amber" => swatch("#a35829", "#7e451d", "#ffe0c2", "#f9b27c"),
        "gray" => swatch("#737373", "#525252", "#fafafa", "#d4d4d4"),
        other => match custom.get(other).and_then(|hex| c4_model::parse_hex(hex)) {
            Some(rgb) => from_rgb(rgb),
            None => element("primary", custom),
        },
    }
}

/// The color of a line. Gray is the page-neutral one; any other is the
/// element color of that name.
pub fn line(name: &str, custom: &BTreeMap<String, String>) -> String {
    match name {
        "gray" | "muted" | "slate" => LINE_GRAY.to_string(),
        other => element(other, custom).fill,
    }
}

fn hex((r, g, b): (u8, u8, u8)) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let ch = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round().clamp(0.0, 255.0) as u8;
    (ch(a.0, b.0), ch(a.1, b.1), ch(a.2, b.2))
}

/// A swatch around a color someone picked: a darker border, and text in
/// whichever of light and dark stands out against it.
fn from_rgb(rgb: (u8, u8, u8)) -> Swatch {
    let linear = |c: u8| {
        let c = c as f64 / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let luminance = 0.2126 * linear(rgb.0) + 0.7152 * linear(rgb.1) + 0.0722 * linear(rgb.2);
    let (black, white) = ((0, 0, 0), (255, 255, 255));
    let (text, text_soft) = if luminance > 0.4 {
        (mix(rgb, black, 0.85), mix(rgb, black, 0.6))
    } else {
        (mix(rgb, white, 0.92), mix(rgb, white, 0.7))
    };
    Swatch { fill: hex(rgb), stroke: hex(mix(rgb, black, 0.25)), text: hex(text), text_soft: hex(text_soft) }
}

#[cfg(test)]
#[path = "../tests/theme.rs"]
mod tests;
