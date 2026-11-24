//! From a computed view to a [`Diagram`]: sizes each box from its text, lays
//! the boxes out (`graph.rs`), and turns the result into shapes, text lines
//! and curves at fixed coordinates.

mod diagram;
mod graph;
pub mod text;
pub mod theme;

use std::collections::BTreeMap;

use c4_model::{Arrow, ComputedView, Direction, Line, ViewNode};

pub use diagram::{Anchor, Diagram, Edge, Label, LineStyle, Marker, Node, Point, Shape, TextRole};
pub use text::FONT_FAMILY;

const TITLE_SIZE: f64 = 15.0;
const TITLE_LINE: f64 = 19.0;
const TECH_SIZE: f64 = 11.0;
const TECH_LINE: f64 = 15.0;
const DESC_SIZE: f64 = 12.5;
const DESC_LINE: f64 = 16.5;
const TEXT_MAX: f64 = 220.0;
const PAD_X: f64 = 18.0;
const PAD_Y: f64 = 16.0;
const MIN_W: f64 = 150.0;
const MIN_H: f64 = 76.0;

const GROUP_TITLE_SIZE: f64 = 13.0;
const EDGE_LABEL_SIZE: f64 = 12.0;
const EDGE_LABEL_LINE: f64 = 15.0;
const EDGE_TECH_SIZE: f64 = 10.5;
const EDGE_LABEL_MAX: f64 = 150.0;

/// A box's text, wrapped, and the size the box needs for it.
struct Sized {
    w: f64,
    h: f64,
    title: Vec<String>,
    technology: String,
    description: Vec<String>,
    text_h: f64,
    /// Parts of the shape text keeps out of: left, right, top, bottom.
    inset: [f64; 4],
}

fn shape_of(shape: c4_model::Shape) -> Shape {
    match shape {
        c4_model::Shape::Rectangle => Shape::Rectangle,
        c4_model::Shape::Person => Shape::Person,
        c4_model::Shape::Browser => Shape::Browser,
        c4_model::Shape::Mobile => Shape::Mobile,
        c4_model::Shape::Cylinder => Shape::Cylinder,
        c4_model::Shape::Storage => Shape::Storage,
        c4_model::Shape::Queue => Shape::Queue,
    }
}

fn line_of(line: Line) -> LineStyle {
    match line {
        Line::Solid => LineStyle::Solid,
        Line::Dashed => LineStyle::Dashed,
        Line::Dotted => LineStyle::Dotted,
    }
}

fn size(node: &ViewNode) -> Sized {
    let title = text::clamp_lines(text::wrap(&node.title, TEXT_MAX, TITLE_SIZE, true), 3);
    let technology = text::fit(node.technology.trim(), TEXT_MAX, TECH_SIZE, false);
    let description = text::clamp_lines(text::wrap(&node.description, TEXT_MAX, DESC_SIZE, false), 4);
    let mut text_h = title.len() as f64 * TITLE_LINE;
    if !technology.is_empty() {
        text_h += 2.0 + TECH_LINE;
    }
    if !description.is_empty() {
        text_h += 6.0 + description.len() as f64 * DESC_LINE;
    }
    let mut text_w: f64 = 0.0;
    for line in &title {
        text_w = text_w.max(text::width(line, TITLE_SIZE, true));
    }
    text_w = text_w.max(text::width(&technology, TECH_SIZE, false));
    for line in &description {
        text_w = text_w.max(text::width(line, DESC_SIZE, false));
    }
    let inset = match node.shape {
        c4_model::Shape::Rectangle => [0.0, 0.0, 0.0, 0.0],
        // A head above the shoulders.
        c4_model::Shape::Person => [0.0, 0.0, 34.0, 0.0],
        // The bar with its three dots.
        c4_model::Shape::Browser => [0.0, 0.0, 22.0, 0.0],
        // The bezel, with the home button's side wider.
        c4_model::Shape::Mobile => [12.0, 22.0, 0.0, 0.0],
        // The lid, and the curve of the base.
        c4_model::Shape::Cylinder | c4_model::Shape::Storage => [0.0, 0.0, 20.0, 6.0],
        // A cylinder on its side.
        c4_model::Shape::Queue => [6.0, 22.0, 0.0, 0.0],
    };
    let w = (text_w + 2.0 * PAD_X).max(MIN_W) + inset[0] + inset[1];
    let h = (text_h + 2.0 * PAD_Y).max(MIN_H) + inset[2] + inset[3];
    Sized { w: w.ceil(), h: h.ceil(), title, technology, description, text_h, inset }
}

/// Lays out a view. `colors` are the model's own (`Model::colors`).
pub fn layout(view: &ComputedView, colors: &BTreeMap<String, String>) -> Diagram {
    let horizontal = matches!(view.direction, Direction::LeftRight | Direction::RightLeft);
    let index: BTreeMap<&str, usize> =
        view.nodes.iter().enumerate().map(|(i, n)| (n.id.as_str(), i)).collect();

    let sized: Vec<Option<Sized>> =
        view.nodes.iter().map(|n| if n.group { None } else { Some(size(n)) }).collect();
    let in_nodes: Vec<graph::InNode> = view
        .nodes
        .iter()
        .zip(&sized)
        .map(|(node, sized)| {
            let parent = node.parent.as_deref().and_then(|p| index.get(p).copied());
            match sized {
                Some(s) => graph::InNode { parent, group: false, w: s.w, h: s.h },
                None => {
                    // Room for at least the start of the title.
                    let title = text::width(&node.title, GROUP_TITLE_SIZE, true).min(TEXT_MAX);
                    graph::InNode { parent, group: true, w: title + 2.0 * graph::GROUP_PAD, h: 0.0 }
                }
            }
        })
        .collect();

    struct EdgeText {
        lines: Vec<String>,
        technology: String,
        w: f64,
        h: f64,
    }
    let mut edge_text: Vec<EdgeText> = Vec::new();
    let mut in_edges: Vec<graph::InEdge> = Vec::new();
    for edge in &view.edges {
        let lines = text::clamp_lines(text::wrap(&edge.label, EDGE_LABEL_MAX, EDGE_LABEL_SIZE, false), 3);
        let technology = if edge.technology.trim().is_empty() {
            String::new()
        } else {
            text::fit(&format!("[{}]", edge.technology.trim()), EDGE_LABEL_MAX, EDGE_TECH_SIZE, false)
        };
        let mut w: f64 = text::width(&technology, EDGE_TECH_SIZE, false);
        for line in &lines {
            w = w.max(text::width(line, EDGE_LABEL_SIZE, false));
        }
        let h = (lines.len() + usize::from(!technology.is_empty())) as f64 * EDGE_LABEL_LINE;
        // An edge with an end the view lost cannot be drawn; `compute_view`
        // never makes one.
        let (Some(&source), Some(&target)) =
            (index.get(edge.source.as_str()), index.get(edge.target.as_str()))
        else {
            continue;
        };
        in_edges.push(graph::InEdge { source, target, label_w: w.ceil(), label_h: h });
        edge_text.push(EdgeText { lines, technology, w: w.ceil(), h });
    }

    let placed = graph::layout(&in_nodes, &in_edges, view.direction);

    // ------------------------------------------------------------ nodes --
    let depth_of = |mut i: usize| -> u32 {
        let mut depth = 0;
        while let Some(p) = in_nodes[i].parent {
            depth += 1;
            i = p;
        }
        depth
    };
    let mut nodes = Vec::with_capacity(view.nodes.len());
    for (i, node) in view.nodes.iter().enumerate() {
        let rect = placed.nodes[i];
        let swatch = theme::element(&node.color, colors);
        let mut labels = Vec::new();
        let (fill_opacity, stroke);
        match &sized[i] {
            None => {
                let room = (rect.w - 2.0 * 16.0).max(20.0);
                labels.push(Label {
                    x: rect.x + 16.0,
                    y: rect.y + 27.0,
                    text: text::fit(&node.title, room, GROUP_TITLE_SIZE, true),
                    size: GROUP_TITLE_SIZE,
                    bold: true,
                    color: swatch.fill.clone(),
                    anchor: Anchor::Start,
                    role: TextRole::GroupTitle,
                });
                // A tint, so what is inside stays the subject.
                fill_opacity = if node.opacity < 1.0 { node.opacity } else { 0.07 };
                stroke = swatch.fill.clone();
            }
            Some(s) => {
                let cx = rect.x + s.inset[0] + (rect.w - s.inset[0] - s.inset[1]) / 2.0;
                let mut top = rect.y + s.inset[2] + (rect.h - s.inset[2] - s.inset[3] - s.text_h) / 2.0;
                // (gap above, text, size, line height, bold, color, role)
                let mut lines: Vec<(f64, &str, f64, f64, bool, &str, TextRole)> = Vec::new();
                for t in &s.title {
                    lines.push((0.0, t, TITLE_SIZE, TITLE_LINE, true, &swatch.text, TextRole::Title));
                }
                if !s.technology.is_empty() {
                    lines.push((
                        2.0,
                        &s.technology,
                        TECH_SIZE,
                        TECH_LINE,
                        false,
                        &swatch.text_soft,
                        TextRole::Technology,
                    ));
                }
                for (k, t) in s.description.iter().enumerate() {
                    let gap = if k == 0 { 6.0 } else { 0.0 };
                    lines.push((
                        gap,
                        t,
                        DESC_SIZE,
                        DESC_LINE,
                        false,
                        &swatch.text_soft,
                        TextRole::Description,
                    ));
                }
                for (gap, text, size, line_h, bold, color, role) in lines {
                    top += gap;
                    labels.push(Label {
                        x: cx,
                        y: round(top + line_h / 2.0 + size * 0.35),
                        text: text.to_string(),
                        size,
                        bold,
                        color: color.to_string(),
                        anchor: Anchor::Middle,
                        role,
                    });
                    top += line_h;
                }
                fill_opacity = 1.0;
                stroke = swatch.stroke.clone();
            }
        }
        nodes.push(Node {
            id: node.id.clone(),
            parent: node.parent.clone(),
            kind: node.kind.clone(),
            title: node.title.clone(),
            shape: shape_of(node.shape),
            group: node.group,
            depth: depth_of(i),
            x: round(rect.x),
            y: round(rect.y),
            width: round(rect.w),
            height: round(rect.h),
            fill: swatch.fill,
            stroke,
            fill_opacity,
            border: line_of(node.border),
            labels,
            navigate_to: node.navigate_to.clone(),
        });
    }

    // ------------------------------------------------------------ edges --
    let mut edges = Vec::with_capacity(in_edges.len());
    let mut drawn = 0;
    for edge in &view.edges {
        if !(index.contains_key(edge.source.as_str()) && index.contains_key(edge.target.as_str())) {
            continue;
        }
        let route = &placed.edges[drawn];
        let words = &edge_text[drawn];
        drawn += 1;
        let color = theme::line(&edge.color, colors);
        let mut points = route.points.clone();
        let mut markers = Vec::new();
        if points.len() >= 2 {
            points.reverse();
            markers.extend(end_mark(&mut points, edge.tail));
            points.reverse();
            markers.extend(end_mark(&mut points, edge.head));
        }
        let mut labels = Vec::new();
        if let Some(rect) = route.label {
            // Beside a line going down, starting at the line; under a line
            // going across, centered on its box.
            let (x, anchor) =
                if horizontal { (rect.x + rect.w / 2.0, Anchor::Middle) } else { (rect.x, Anchor::Start) };
            let mut top = rect.y + (rect.h - words.h) / 2.0;
            let label_color = if matches!(edge.color.as_str(), "gray" | "muted" | "slate") {
                theme::LABEL_GRAY.to_string()
            } else {
                color.clone()
            };
            for line in &words.lines {
                labels.push(Label {
                    x: round(x),
                    y: round(top + EDGE_LABEL_LINE / 2.0 + EDGE_LABEL_SIZE * 0.35),
                    text: line.clone(),
                    size: EDGE_LABEL_SIZE,
                    bold: false,
                    color: label_color.clone(),
                    anchor,
                    role: TextRole::EdgeLabel,
                });
                top += EDGE_LABEL_LINE;
            }
            if !words.technology.is_empty() {
                labels.push(Label {
                    x: round(x),
                    y: round(top + EDGE_LABEL_LINE / 2.0 + EDGE_TECH_SIZE * 0.35),
                    text: words.technology.clone(),
                    size: EDGE_TECH_SIZE,
                    bold: false,
                    color: label_color.clone(),
                    anchor,
                    role: TextRole::EdgeLabel,
                });
            }
            debug_assert!(words.w <= rect.w.max(rect.h) + 1.0);
        }
        edges.push(Edge {
            id: edge.id.clone(),
            source: edge.source.clone(),
            target: edge.target.clone(),
            path: smooth(&points),
            color,
            line: line_of(edge.line),
            markers,
            labels,
            title: edge.label.clone(),
        });
    }

    Diagram {
        id: view.id.clone(),
        title: view.title.clone(),
        description: view.description.clone(),
        width: placed.width.ceil(),
        height: placed.height.ceil(),
        font_family: FONT_FAMILY.to_string(),
        nodes,
        edges,
    }
}

/// Two decimals: plenty for a pixel, and short in SVG and JSON.
fn round(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The mark at the end of `points`, which is shortened to make room for it.
fn end_mark(points: &mut [(f64, f64)], arrow: Arrow) -> Option<Marker> {
    let n = points.len();
    let (tip, before) = (points[n - 1], points[n - 2]);
    let (dx, dy) = (tip.0 - before.0, tip.1 - before.1);
    let len = (dx * dx + dy * dy).sqrt();
    if arrow == Arrow::None || len < 0.01 {
        return None;
    }
    let (ux, uy) = (dx / len, dy / len);
    // A point `along` back from the tip and `across` to the side.
    let at = |along: f64, across: f64| Point {
        x: round(tip.0 - ux * along - uy * across),
        y: round(tip.1 - uy * along + ux * across),
    };
    let (marker, cut): (Marker, f64) = match arrow {
        Arrow::None => unreachable!(),
        Arrow::Normal | Arrow::ONormal => (
            Marker {
                points: vec![at(0.0, 0.0), at(10.0, 4.5), at(10.0, -4.5)],
                radius: 0.0,
                filled: arrow == Arrow::Normal,
                closed: true,
            },
            9.0,
        ),
        Arrow::Open => (
            Marker {
                points: vec![at(9.0, 5.0), at(0.0, 0.0), at(9.0, -5.0)],
                radius: 0.0,
                filled: false,
                closed: false,
            },
            0.5,
        ),
        Arrow::Diamond | Arrow::ODiamond => (
            Marker {
                points: vec![at(0.0, 0.0), at(6.5, 4.5), at(13.0, 0.0), at(6.5, -4.5)],
                radius: 0.0,
                filled: arrow == Arrow::Diamond,
                closed: true,
            },
            12.5,
        ),
        Arrow::Dot | Arrow::ODot => (
            Marker { points: vec![at(4.5, 0.0)], radius: 4.5, filled: arrow == Arrow::Dot, closed: true },
            8.5,
        ),
    };
    // The line stops at the back of the mark, not at its tip.
    let cut = cut.min(len * 0.9);
    points[n - 1] = (tip.0 - ux * cut, tip.1 - uy * cut);
    Some(marker)
}

/// A smooth curve past the corners: a uniform B-spline, as cubic Béziers.
/// It starts and ends exactly on the first and last corner, leaving and
/// arriving along the first and last stretch.
fn smooth(corners: &[(f64, f64)]) -> Vec<Point> {
    let Some((&first, &last)) = corners.first().zip(corners.last()) else { return Vec::new() };
    let mut p = vec![first, first];
    p.extend_from_slice(corners);
    p.extend([last, last]);
    let point = |x: f64, y: f64| Point { x: round(x), y: round(y) };
    let mut out = vec![point(first.0, first.1)];
    for w in p.windows(4) {
        let (a, b, c) = (w[1], w[2], w[3]);
        out.push(point((2.0 * a.0 + b.0) / 3.0, (2.0 * a.1 + b.1) / 3.0));
        out.push(point((a.0 + 2.0 * b.0) / 3.0, (a.1 + 2.0 * b.1) / 3.0));
        out.push(point((a.0 + 4.0 * b.0 + c.0) / 6.0, (a.1 + 4.0 * b.1 + c.1) / 6.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use c4_model::{Source, load};

    const SHOP: &str = "
        specification {
            element actor { style { shape person; color green } }
            element system
            element component
            color brand #ffee00
        }
        model {
            customer = actor 'Customer' 'Someone who buys things from the shop, now and then'
            shop = system 'Shop' {
                ui = component 'Web app' 'What the customer sees' 'React' { style { shape browser; color brand } }
                api = component 'API'
                db = component 'Database' { style { shape storage } }
                ui -> api 'calls' 'HTTPS'
                api -> db 'reads and writes'
            }
            customer -> shop.ui 'browses'
            customer -> shop 'buys from' { head diamond; tail odot; color red }
        }
        views {
            view index { include * }
            view shop of shop { include * }
            view across of shop { include *; autoLayout LeftRight }
        }";

    fn diagram(view: &str) -> Diagram {
        let ws = load(&[Source { name: "shop.c4".into(), text: SHOP.into() }]);
        assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
        layout(&ws.model.compute_view(view).unwrap(), &ws.model.colors)
    }

    #[test]
    fn boxes_fit_their_text() {
        let d = diagram("index");
        assert_eq!(d.nodes.len(), 2);
        let customer = &d.nodes[0];
        assert_eq!(customer.shape, Shape::Person);
        assert_eq!(customer.fill, "#428a4f");
        let texts: Vec<_> = customer.labels.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["Customer", "Someone who buys things from the", "shop, now and then"]);
        for label in &customer.labels {
            let half = text::width(&label.text, label.size, label.bold) / 2.0;
            assert!(label.x - half >= customer.x && label.x + half <= customer.x + customer.width);
            assert!(label.y > customer.y + 34.0 && label.y < customer.y + customer.height);
        }
        assert!(customer.labels[0].bold && customer.labels[0].role == TextRole::Title);
        assert_eq!(d.nodes[1].navigate_to.as_deref(), Some("shop"));
        assert!(d.width > customer.width && d.height > customer.height * 2.0);
    }

    #[test]
    fn a_group_is_a_tinted_titled_border() {
        let d = diagram("shop");
        let shop = d.nodes.iter().find(|n| n.id == "shop").unwrap();
        assert!(shop.group && shop.fill_opacity < 0.2 && shop.depth == 0);
        assert_eq!(shop.labels[0].role, TextRole::GroupTitle);
        let ui = d.nodes.iter().find(|n| n.id == "shop.ui").unwrap();
        assert_eq!((ui.depth, ui.fill.as_str(), ui.shape), (1, "#ffee00", Shape::Browser));
        assert!(ui.x > shop.x && ui.y > shop.labels[0].y && ui.x + ui.width < shop.x + shop.width);
        let texts: Vec<_> = ui.labels.iter().map(|l| (l.text.as_str(), l.role)).collect();
        assert_eq!(
            texts,
            [
                ("Web app", TextRole::Title),
                ("React", TextRole::Technology),
                ("What the customer sees", TextRole::Description)
            ]
        );
    }

    #[test]
    fn lines_have_curves_marks_and_labels() {
        let d = diagram("shop");
        let calls = d.edges.iter().find(|e| e.id == "shop.ui->shop.api").unwrap();
        assert_eq!(calls.path.len() % 3, 1);
        let texts: Vec<_> = calls.labels.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["calls", "[HTTPS]"]);
        assert_eq!(calls.labels[0].anchor, Anchor::Start);
        assert_eq!(calls.color, theme::LINE_GRAY);
        // One arrowhead, its tip on the top of the box it points at.
        assert_eq!(calls.markers.len(), 1);
        let api = d.nodes.iter().find(|n| n.id == "shop.api").unwrap();
        let tip = calls.markers[0].points[0];
        assert_eq!(tip.y, api.y);
        assert!(tip.x > api.x && tip.x < api.x + api.width);
        assert_eq!(calls.path.last().unwrap().y, api.y - 9.0);

        let buys = d.edges.iter().find(|e| e.id == "customer->shop").unwrap();
        assert_eq!(buys.color, "#ac4d39");
        assert_eq!(buys.markers.len(), 2);
        assert!(buys.markers[0].radius > 0.0 && !buys.markers[0].filled, "the tail: an open dot");
        assert_eq!(buys.markers[1].points.len(), 4);

        let across = diagram("across");
        let calls = across.edges.iter().find(|e| e.id == "shop.ui->shop.api").unwrap();
        assert_eq!(calls.labels[0].anchor, Anchor::Middle);
        let (ui, api) = (&across.nodes[2], &across.nodes[3]);
        assert!(ui.x + ui.width < api.x);
    }

    #[test]
    fn the_same_source_gives_the_same_diagram() {
        assert_eq!(diagram("shop"), diagram("shop"));
        let json = |d: &Diagram| format!("{:?}", d);
        assert_eq!(json(&diagram("across")), json(&diagram("across")));
    }

    #[test]
    fn a_smooth_line_starts_and_ends_where_it_should() {
        let path = smooth(&[(0.0, 0.0), (0.0, 30.0), (50.0, 60.0), (50.0, 90.0)]);
        assert_eq!(path.len(), 1 + 3 * 5);
        assert_eq!((path[0].x, path[0].y), (0.0, 0.0));
        assert_eq!((path[15].x, path[15].y), (50.0, 90.0));
        // Leaves straight down.
        assert!(path[1].x == 0.0 && path[2].x == 0.0);
        assert!(smooth(&[]).is_empty());
    }
}
