//! Measuring and wrapping text without a font.
//!
//! The core runs where there is no font to ask (wasm, a CLI), and every
//! front end has to agree on where a line breaks, so widths come from a
//! table: Helvetica's advance widths, which Arial and the usual system
//! sans-serifs stay close to. Renderers draw with a font from that family
//! and never wrap text themselves.

/// The font renderers should ask for, so text is the width measured here.
pub const FONT_FAMILY: &str = "Helvetica, Arial, 'Liberation Sans', sans-serif";

/// Advance widths in thousandths of the font size, for ' ' to '~'.
const WIDTHS: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, // space to /
    556, 556, 556, 556, 556, 556, 556, 556, 556, 556, // digits
    278, 278, 584, 584, 584, 556, 1015, // : to @
    667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722,
    667, 944, 667, 667, 611, // A to Z
    278, 278, 278, 469, 556, 333, // [ to `
    556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556,
    500, 722, 500, 500, 500, // a to z
    334, 260, 334, 584, // { to ~
];

fn char_width(c: char) -> f64 {
    let code = c as u32;
    if (32..127).contains(&code) {
        return WIDTHS[(code - 32) as usize] as f64;
    }
    match code {
        // Combining marks take no room of their own.
        0x300..=0x36f => 0.0,
        // CJK, kana, hangul, full-width forms, emoji: a full square.
        0x1100..=0x115f
        | 0x2e80..=0xa4cf
        | 0xac00..=0xd7a3
        | 0xf900..=0xfaff
        | 0xff00..=0xff60
        | 0x1f300..=0x1faff => 1000.0,
        _ => 600.0,
    }
}

/// How wide `text` is at `size` pixels. A little generous, so a font that
/// runs wider than the table still fits its box.
pub fn width(text: &str, size: f64, bold: bool) -> f64 {
    let units: f64 = text.chars().map(char_width).sum();
    units / 1000.0 * size * if bold { 1.1 } else { 1.03 }
}

/// Breaks text into lines no wider than `max`. Line breaks in the text are
/// kept; a word too long for a line is split.
pub fn wrap(text: &str, max: f64, size: f64, bold: bool) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if width(&candidate, size, bold) <= max {
                line = candidate;
                continue;
            }
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            // The word alone: whole if it fits, else in pieces.
            let mut piece = String::new();
            for c in word.chars() {
                piece.push(c);
                if width(&piece, size, bold) > max && piece.chars().count() > 1 {
                    let last = piece.pop().expect("more than one character");
                    lines.push(std::mem::replace(&mut piece, last.to_string()));
                }
            }
            line = piece;
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    lines
}

/// At most `max_lines` lines, the last ending in an ellipsis if text was cut.
pub fn clamp_lines(mut lines: Vec<String>, max_lines: usize) -> Vec<String> {
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            last.push('…');
        }
    }
    lines
}

/// `text`, cut to fit `max` with an ellipsis if it has to be.
pub fn fit(text: &str, max: f64, size: f64, bold: bool) -> String {
    if width(text, size, bold) <= max {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        out.push(c);
        if width(&format!("{out}…"), size, bold) > max {
            out.pop();
            break;
        }
    }
    format!("{}…", out.trim_end())
}

#[cfg(test)]
#[path = "../tests/text.rs"]
mod tests;
