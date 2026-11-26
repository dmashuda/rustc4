//! A [`Diagram`] as SVG.
//!
//! The same text is an exported image and the web app's drawing: every
//! node and line is a `<g>` with its id in `data-id`, which a page can
//! listen on and an image viewer ignores. Nothing is decided here that the
//! diagram has not already decided; this only spells it in SVG.

use std::fmt::Write;

use c4_layout::{Anchor, Diagram, Edge, Label, Legend, LineStyle, Node, Shape};

#[derive(Debug, Clone, Default)]
pub struct SvgOptions {
    /// A color to paint behind the diagram; `None` leaves it transparent.
    pub background: Option<String>,
}

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// A number as SVG wants it: no trailing zeros.
fn n(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

fn dash(line: LineStyle) -> &'static str {
    match line {
        LineStyle::Solid => "",
        LineStyle::Dashed => r#" stroke-dasharray="6 4""#,
        LineStyle::Dotted => r#" stroke-dasharray="1.5 4" stroke-linecap="round""#,
        // No border: the same shape, with nothing drawn around it.
        LineStyle::Hidden => r#" stroke-dasharray="0 1""#,
    }
}

fn text(out: &mut String, label: &Label) {
    let anchor = match label.anchor {
        Anchor::Start => "",
        Anchor::Middle => r#" text-anchor="middle""#,
    };
    let weight = if label.bold { r#" font-weight="700""# } else { "" };
    let _ = write!(
        out,
        r#"<text class="{role:?}" x="{x}" y="{y}" font-size="{size}"{weight}{anchor} fill="{fill}">{text}</text>"#,
        role = label.role,
        x = n(label.x),
        y = n(label.y),
        size = n(label.size),
        fill = esc(&label.color),
        text = esc(&label.text),
    );
}

/// The outline of a node's shape, with its top left at `x`, `y`.
fn shape(out: &mut String, node: &Node, x: f64, y: f64) {
    let (w, h) = (node.width, node.height);
    let (fill, stroke) = (esc(&node.fill), esc(&node.stroke));
    let border = dash(node.border);
    let paint = format!(r#"fill="{fill}" stroke="{stroke}" stroke-width="1.5"{border}"#);
    match node.shape {
        Shape::Rectangle => {
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="6" {paint}/>"#,
                n(x),
                n(y),
                n(w),
                n(h)
            );
        }
        Shape::Person => {
            // Shoulders, then a head overlapping them.
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="18" {paint}/><circle cx="{}" cy="{}" r="19" {paint}/>"#,
                n(x),
                n(y + 24.0),
                n(w),
                n(h - 24.0),
                n(x + w / 2.0),
                n(y + 19.75),
            );
        }
        Shape::Browser => {
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="6" fill="{stroke}" stroke="{stroke}" stroke-width="1.5"{border}/>"#,
                n(x),
                n(y),
                n(w),
                n(h)
            );
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="3" fill="{fill}"/>"#,
                n(x + 4.0),
                n(y + 22.0),
                n(w - 8.0),
                n(h - 26.0)
            );
            for i in 0..3 {
                let _ = write!(
                    out,
                    r##"<circle cx="{}" cy="{}" r="3.5" fill="#fff" fill-opacity="0.55"/>"##,
                    n(x + 13.0 + 12.0 * i as f64),
                    n(y + 11.0)
                );
            }
        }
        Shape::Mobile => {
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="10" fill="{stroke}" stroke="{stroke}" stroke-width="1.5"{border}/>"#,
                n(x),
                n(y),
                n(w),
                n(h)
            );
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="{}" rx="4" fill="{fill}"/><circle cx="{}" cy="{}" r="5" fill="{fill}"/>"#,
                n(x + 10.0),
                n(y + 5.0),
                n(w - 32.0),
                n(h - 10.0),
                n(x + w - 11.0),
                n(y + h / 2.0),
            );
        }
        Shape::Cylinder | Shape::Storage => {
            let (rx, ry) = (w / 2.0, 10.0);
            let _ = write!(
                out,
                r#"<path d="M{x0} {top}A{rx} {ry} 0 0 1 {x1} {top}V{bottom}A{rx} {ry} 0 0 1 {x0} {bottom}Z" {paint}/><path d="M{x0} {top}A{rx} {ry} 0 0 0 {x1} {top}" fill="none" stroke="{stroke}" stroke-width="1.5"/>"#,
                x0 = n(x),
                x1 = n(x + w),
                top = n(y + ry),
                bottom = n(y + h - ry),
                rx = n(rx),
                ry = n(ry),
            );
        }
        Shape::Queue => {
            let (rx, ry) = (10.0, h / 2.0);
            let _ = write!(
                out,
                r#"<path d="M{left} {y0}H{right}A{rx} {ry} 0 0 1 {right} {y1}H{left}A{rx} {ry} 0 0 1 {left} {y0}Z" {paint}/><path d="M{right} {y0}A{rx} {ry} 0 0 0 {right} {y1}" fill="none" stroke="{stroke}" stroke-width="1.5"/>"#,
                left = n(x + rx),
                right = n(x + w - rx),
                y0 = n(y),
                y1 = n(y + h),
                rx = n(rx),
                ry = n(ry),
            );
        }
    }
}

fn node(out: &mut String, node: &Node) {
    let class = if node.group { "node group" } else { "node" };
    let _ = write!(out, r#"<g class="{class}" data-id="{}" data-kind="{}""#, esc(&node.id), esc(&node.kind));
    if let Some(view) = &node.navigate_to {
        let _ = write!(out, r#" data-view="{}""#, esc(view));
    }
    let _ = write!(out, "><title>{}</title>", esc(&node.title));
    if node.group {
        let _ = write!(
            out,
            r#"<rect x="{}" y="{}" width="{}" height="{}" rx="8" fill="{}" fill-opacity="{}" stroke="{}" stroke-opacity="0.55" stroke-width="1.25"{}/>"#,
            n(node.x),
            n(node.y),
            n(node.width),
            n(node.height),
            esc(&node.fill),
            n(node.fill_opacity),
            esc(&node.stroke),
            dash(node.border),
        );
    } else {
        // A stack: the ones behind first.
        for corner in &node.behind {
            shape(out, node, corner.x, corner.y);
        }
        shape(out, node, node.x, node.y);
    }
    if let Some(icon) = &node.icon {
        let _ = write!(
            out,
            r#"<image href="{}" x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="xMidYMid meet"/>"#,
            esc(&icon.href),
            n(icon.x),
            n(icon.y),
            n(icon.width),
            n(icon.height),
        );
    }
    for label in &node.labels {
        text(out, label);
    }
    if node.navigate_to.is_some() {
        // A magnifying glass in the corner: there is a closer view. Under
        // it, something to point at as large as the Apple drawing's
        // (`DiagramHit`): the glass alone is two thin strokes.
        let (cx, cy) = (node.x + node.width - 17.0, node.y + if node.group { 22.0 } else { 16.0 });
        let color = if node.group { esc(&node.stroke) } else { "#fff".to_string() };
        let _ = write!(
            out,
            r#"<g class="open" fill="none" stroke="{color}" stroke-opacity="0.85" stroke-width="1.6" stroke-linecap="round"><rect class="hit" x="{}" y="{}" width="24" height="24" fill="{color}" fill-opacity="0" stroke="none"/><circle cx="{}" cy="{}" r="4.5"/><path d="M{} {}l4 4"/></g>"#,
            n(cx - 12.0),
            n(cy - 12.0),
            n(cx),
            n(cy),
            n(cx + 3.4),
            n(cy + 3.4),
        );
    }
    out.push_str("</g>");
}

fn edge(out: &mut String, edge: &Edge) {
    let _ = write!(
        out,
        r#"<g class="edge" data-id="{}" data-source="{}" data-target="{}"><title>{}</title>"#,
        esc(&edge.id),
        esc(&edge.source),
        esc(&edge.target),
        esc(&edge.title),
    );
    line(out, edge, true);
    out.push_str("</g>");
}

/// What the looks in the diagram mean. Its samples are the same shapes and
/// lines as the diagram's, without ids: there is nothing to tap.
fn legend(out: &mut String, legend: &Legend) {
    let _ = write!(
        out,
        r#"<g class="legend"><rect x="{}" y="{}" width="{}" height="{}" rx="8" fill="none" stroke="{}" stroke-opacity="0.55" stroke-width="1.25"/>"#,
        n(legend.x),
        n(legend.y),
        n(legend.width),
        n(legend.height),
        esc(&legend.stroke),
    );
    for node in &legend.nodes {
        shape(out, node, node.x, node.y);
    }
    for edge in &legend.edges {
        line(out, edge, false);
    }
    for label in &legend.labels {
        text(out, label);
    }
    out.push_str("</g>");
}

/// An edge's curve and end marks. `hit`: with something wide to point at.
fn line(out: &mut String, edge: &Edge, hit: bool) {
    let color = esc(&edge.color);
    if let Some(start) = edge.path.first() {
        let mut d = format!("M{} {}", n(start.x), n(start.y));
        for curve in edge.path[1..].as_chunks::<3>().0 {
            let _ = write!(
                d,
                "C{} {} {} {} {} {}",
                n(curve[0].x),
                n(curve[0].y),
                n(curve[1].x),
                n(curve[1].y),
                n(curve[2].x),
                n(curve[2].y)
            );
        }
        // A wide invisible stroke first: something to point at.
        if hit {
            let _ = write!(
                out,
                r#"<path class="hit" d="{d}" fill="none" stroke="{color}" stroke-opacity="0" stroke-width="14"/>"#
            );
        }
        let _ = write!(
            out,
            r#"<path d="{d}" fill="none" stroke="{color}" stroke-width="1.6"{}/>"#,
            dash(edge.line)
        );
    }
    for marker in &edge.markers {
        let fill = if marker.filled { color.as_str() } else { "none" };
        if marker.radius > 0.0 {
            let _ = write!(
                out,
                r#"<circle cx="{}" cy="{}" r="{}" fill="{fill}" stroke="{color}" stroke-width="1.6"/>"#,
                n(marker.points[0].x),
                n(marker.points[0].y),
                n(marker.radius)
            );
        } else {
            let points: Vec<String> =
                marker.points.iter().map(|p| format!("{},{}", n(p.x), n(p.y))).collect();
            let tag = if marker.closed { "polygon" } else { "polyline" };
            let _ = write!(
                out,
                r#"<{tag} points="{}" fill="{fill}" stroke="{color}" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round"/>"#,
                points.join(" ")
            );
        }
    }
}

pub fn svg(diagram: &Diagram, options: &SvgOptions) -> String {
    let mut out = String::new();
    let (w, h) = (n(diagram.width), n(diagram.height));
    let _ = write!(
        out,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" font-family="{}" data-view="{}"><title>{}</title>"#,
        esc(&diagram.font_family),
        esc(&diagram.id),
        esc(&diagram.title),
    );
    if let Some(background) = &options.background {
        let _ = write!(out, r#"<rect width="{w}" height="{h}" fill="{}"/>"#, esc(background));
    }
    // A sequence's lines down, behind everything.
    for line in &diagram.lifelines {
        let _ = write!(
            out,
            r#"<path class="lifeline" d="M{} {}L{} {}" fill="none" stroke="{}" stroke-width="1.2"{}/>"#,
            n(line.from.x),
            n(line.from.y),
            n(line.to.x),
            n(line.to.y),
            esc(&line.color),
            dash(line.line)
        );
    }
    // Groups at the back, then lines, then boxes, then the words on lines.
    for item in diagram.nodes.iter().filter(|x| x.group) {
        node(&mut out, item);
    }
    for item in &diagram.edges {
        edge(&mut out, item);
    }
    for item in diagram.nodes.iter().filter(|x| !x.group) {
        node(&mut out, item);
    }
    for item in &diagram.edges {
        if !item.labels.is_empty() {
            let _ = write!(out, r#"<g class="edge-label" data-id="{}">"#, esc(&item.id));
            for label in &item.labels {
                text(&mut out, label);
            }
            out.push_str("</g>");
        }
    }
    if let Some(item) = &diagram.legend {
        legend(&mut out, item);
    }
    out.push_str("</svg>");
    out
}

#[cfg(test)]
#[path = "../tests/lib.rs"]
mod tests;
