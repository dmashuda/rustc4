//! From a computed view to a [`Diagram`]: sizes each box from its text, lays
//! the boxes out (`graph.rs`), and turns the result into shapes, text lines
//! and curves at fixed coordinates.

mod diagram;
mod graph;
mod sequence;
pub mod text;
pub mod theme;

use std::collections::BTreeMap;

use c4_model::{Arrow, ComputedView, Direction, IconPosition, Line, Size, ViewNode};

pub use diagram::{
    Anchor, Diagram, Edge, Icon, Label, Legend, Lifeline, LineStyle, Marker, Node, Point, Shape, TextRole,
};
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

/// How far behind and to the side each further one of a stack shows.
const STACK: f64 = 8.0;

/// What `size` multiplies a box's least width and height by, `padding` the
/// room around its words, and `textSize` its type.
fn size_scale(size: Size) -> f64 {
    match size {
        Size::XSmall => 0.6,
        Size::Small => 0.8,
        Size::Medium => 1.0,
        Size::Large => 1.3,
        Size::XLarge => 1.6,
    }
}

fn padding_scale(size: Size) -> f64 {
    match size {
        Size::XSmall => 0.5,
        Size::Small => 0.75,
        Size::Medium => 1.0,
        Size::Large => 1.5,
        Size::XLarge => 2.0,
    }
}

fn text_scale(size: Size) -> f64 {
    match size {
        Size::XSmall => 0.75,
        Size::Small => 0.875,
        Size::Medium => 1.0,
        Size::Large => 1.2,
        Size::XLarge => 1.45,
    }
}

/// Between an icon and the words it is with.
const ICON_GAP: f64 = 12.0;
/// Before a group's title: the icon's side, and the gap after it.
const GROUP_ICON: f64 = 16.0;
const GROUP_ICON_GAP: f64 = 6.0;

/// The side of the square an icon is drawn in.
fn icon_side(size: Size) -> f64 {
    match size {
        Size::XSmall => 20.0,
        Size::Small => 28.0,
        Size::Medium => 36.0,
        Size::Large => 48.0,
        Size::XLarge => 64.0,
    }
}

/// A box's text, wrapped, and the size the box needs for it.
struct Sized {
    /// Of the shape itself: a stack takes `STACK` more each way.
    w: f64,
    h: f64,
    /// What its type is multiplied by.
    scale: f64,
    title: Vec<String>,
    technology: String,
    description: Vec<String>,
    text_w: f64,
    text_h: f64,
    /// The side of its icon's square, or zero.
    icon: f64,
    /// Its words and icon together.
    content_w: f64,
    content_h: f64,
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
    let (big, pad, scale) = (size_scale(node.size), padding_scale(node.padding), text_scale(node.text_size));
    // A bigger box, or bigger type, has longer lines before they break.
    let text_max = TEXT_MAX * big * scale;
    let title = text::clamp_lines(text::wrap(&node.title, text_max, TITLE_SIZE * scale, true), 3);
    // The smallest says only what it is called.
    let terse = node.size == Size::XSmall;
    let technology = match terse {
        true => String::new(),
        false => text::fit(node.technology.trim(), text_max, TECH_SIZE * scale, false),
    };
    let description = match terse {
        true => Vec::new(),
        false => text::clamp_lines(text::wrap(&node.description, text_max, DESC_SIZE * scale, false), 4),
    };
    let mut text_h = title.len() as f64 * TITLE_LINE * scale;
    if !technology.is_empty() {
        text_h += 2.0 + TECH_LINE * scale;
    }
    if !description.is_empty() {
        text_h += 6.0 + description.len() as f64 * DESC_LINE * scale;
    }
    let mut text_w: f64 = 0.0;
    for line in &title {
        text_w = text_w.max(text::width(line, TITLE_SIZE * scale, true));
    }
    text_w = text_w.max(text::width(&technology, TECH_SIZE * scale, false));
    for line in &description {
        text_w = text_w.max(text::width(line, DESC_SIZE * scale, false));
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
    let icon = if node.icon.is_some() { icon_side(node.icon_size) } else { 0.0 };
    let (content_w, content_h) = match node.icon_position {
        _ if icon == 0.0 => (text_w, text_h),
        IconPosition::Left | IconPosition::Right => (icon + ICON_GAP + text_w, text_h.max(icon)),
        IconPosition::Top | IconPosition::Bottom => (text_w.max(icon), icon + ICON_GAP + text_h),
    };
    let w = (content_w + 2.0 * PAD_X * pad).max(MIN_W * big) + inset[0] + inset[1];
    let h = (content_h + 2.0 * PAD_Y * pad).max(MIN_H * big) + inset[2] + inset[3];
    Sized {
        w: w.ceil(),
        h: h.ceil(),
        scale,
        title,
        technology,
        description,
        text_w,
        text_h,
        icon,
        content_w,
        content_h,
        inset,
    }
}

/// What to draw besides the view itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// A legend under the diagram: a sample of each look whose kind has a
    /// `notation`, with what it says. None, if nothing shown has one.
    pub legend: bool,
}

/// With a legend: a diagram explains itself unless told not to.
impl Default for Options {
    fn default() -> Self {
        Options { legend: true }
    }
}

/// Whether a legend of this view would have anything in it.
pub fn has_legend(view: &ComputedView) -> bool {
    view.nodes.iter().any(|n| !n.notation.is_empty()) || view.edges.iter().any(|e| !e.notation.is_empty())
}

/// Lays out a view. `colors` are the model's own (`Model::colors`).
pub fn layout(view: &ComputedView, colors: &BTreeMap<String, String>) -> Diagram {
    layout_with(view, colors, Options::default())
}

pub fn layout_with(view: &ComputedView, colors: &BTreeMap<String, String>, options: Options) -> Diagram {
    // A sequence has only who takes part, side by side in the order they
    // come into it: no groups, and nobody who only stands by.
    let participants;
    let view = if view.sequence {
        let mut nodes: Vec<ViewNode> = Vec::new();
        for end in view.edges.iter().flat_map(|e| [&e.source, &e.target]) {
            if !nodes.iter().any(|n| n.id == *end)
                && let Some(node) = view.nodes.iter().find(|n| n.id == *end)
            {
                nodes.push(ViewNode { parent: None, group: false, ..node.clone() });
            }
        }
        participants = ComputedView { nodes, ..view.clone() };
        &participants
    } else {
        view
    };
    // Words go over a step's line, centered, as under a line going across.
    let horizontal = view.sequence || matches!(view.direction, Direction::LeftRight | Direction::RightLeft);
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
                Some(s) => {
                    // Room for the ones behind, so nothing is laid over them.
                    let behind = if node.multiple { STACK } else { 0.0 };
                    graph::InNode { parent, group: false, w: s.w + behind, h: s.h + behind }
                }
                None => {
                    // Room for at least the start of the title.
                    let title = text::width(&node.title, GROUP_TITLE_SIZE, true).min(TEXT_MAX);
                    let icon = if node.icon.is_some() { GROUP_ICON + GROUP_ICON_GAP } else { 0.0 };
                    graph::InNode { parent, group: true, w: icon + title + 2.0 * graph::GROUP_PAD, h: 0.0 }
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

    let (placed, lines_down) = if view.sequence {
        sequence::layout(&in_nodes, &in_edges)
    } else {
        (graph::layout_spaced(&in_nodes, &in_edges, view.direction, view.rank_gap, view.node_gap), Vec::new())
    };
    let lifelines = view
        .nodes
        .iter()
        .zip(&lines_down)
        .map(|(node, line)| Lifeline {
            node: node.id.clone(),
            from: Point { x: round(line.x), y: round(line.top) },
            to: Point { x: round(line.x), y: round(line.bottom) },
            color: theme::LINE_GRAY.to_string(),
            line: LineStyle::Dashed,
        })
        .collect();

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
        let mut rect = placed.nodes[i];
        let mut behind = Vec::new();
        if node.multiple && sized[i].is_some() {
            // The one in front is up and to the left; the other shows past
            // its right and bottom edges.
            rect.w -= STACK;
            rect.h -= STACK;
            behind.push(Point { x: round(rect.x + STACK), y: round(rect.y + STACK) });
        }
        let swatch = theme::element(&node.color, colors);
        let mut labels = Vec::new();
        let mut icon = None;
        let picture = |x: f64, y: f64, side: f64| {
            node.icon.as_ref().map(|href| Icon {
                href: href.clone(),
                x: round(x),
                y: round(y),
                width: side,
                height: side,
            })
        };
        let (fill_opacity, stroke);
        match &sized[i] {
            None => {
                // A group's icon is before its title.
                let before = if node.icon.is_some() { GROUP_ICON + GROUP_ICON_GAP } else { 0.0 };
                icon = picture(rect.x + 16.0, rect.y + 27.0 - 12.5, GROUP_ICON);
                let room = (rect.w - 2.0 * 16.0 - before).max(20.0);
                labels.push(Label {
                    x: rect.x + 16.0 + before,
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
                // The middle of what the shape leaves for words, and the
                // words and icon together in the middle of that.
                let mid_x = rect.x + s.inset[0] + (rect.w - s.inset[0] - s.inset[1]) / 2.0;
                let mid_y = rect.y + s.inset[2] + (rect.h - s.inset[2] - s.inset[3]) / 2.0;
                let (left, above) = (mid_x - s.content_w / 2.0, mid_y - s.content_h / 2.0);
                let (mut cx, mut top) = (mid_x, mid_y - s.text_h / 2.0);
                if s.icon > 0.0 {
                    let beside = s.icon + ICON_GAP;
                    match node.icon_position {
                        IconPosition::Left => {
                            icon = picture(left, mid_y - s.icon / 2.0, s.icon);
                            cx = left + beside + s.text_w / 2.0;
                        }
                        IconPosition::Right => {
                            icon = picture(left + s.text_w + ICON_GAP, mid_y - s.icon / 2.0, s.icon);
                            cx = left + s.text_w / 2.0;
                        }
                        IconPosition::Top => {
                            icon = picture(mid_x - s.icon / 2.0, above, s.icon);
                            top = above + beside;
                        }
                        IconPosition::Bottom => {
                            icon = picture(mid_x - s.icon / 2.0, above + s.text_h + ICON_GAP, s.icon);
                            top = above;
                        }
                    }
                }
                // (gap above, text, size, line height, bold, color, role)
                let mut lines: Vec<(f64, &str, f64, f64, bool, &str, TextRole)> = Vec::new();
                let m = s.scale;
                for t in &s.title {
                    lines.push((0.0, t, TITLE_SIZE * m, TITLE_LINE * m, true, &swatch.text, TextRole::Title));
                }
                if !s.technology.is_empty() {
                    lines.push((
                        2.0,
                        &s.technology,
                        TECH_SIZE * m,
                        TECH_LINE * m,
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
                        DESC_SIZE * m,
                        DESC_LINE * m,
                        false,
                        &swatch.text_soft,
                        TextRole::Description,
                    ));
                }
                for (gap, text, size, line_h, bold, color, role) in lines {
                    top += gap;
                    labels.push(Label {
                        x: round(cx),
                        y: round(top + line_h / 2.0 + size * 0.35),
                        text: text.to_string(),
                        size: round(size),
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
            behind,
            icon,
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

    let (mut width, mut height) = (placed.width.ceil(), placed.height.ceil());
    let legend = if options.legend { legend(view, colors, width, height) } else { None };
    if let Some(legend) = &legend {
        width = width.max(legend.x + legend.width + LEGEND_MARGIN);
        height = legend.y + legend.height + LEGEND_MARGIN;
    }
    Diagram {
        id: view.id.clone(),
        title: view.title.clone(),
        description: view.description.clone(),
        width,
        height,
        font_family: FONT_FAMILY.to_string(),
        nodes,
        edges,
        lifelines,
        legend,
    }
}

/// From the edge of the drawing to the legend's frame.
const LEGEND_MARGIN: f64 = 24.0;
/// Inside the frame.
const LEGEND_PAD: f64 = 14.0;
const LEGEND_TITLE: f64 = 26.0;
/// A sample: big enough for every shape to be itself.
const SAMPLE_W: f64 = 76.0;
const SAMPLE_H: f64 = 56.0;
const SAMPLE_GAP: f64 = 10.0;
const LEGEND_TEXT_MAX: f64 = 170.0;
const LEGEND_TEXT_SIZE: f64 = 12.5;
const LEGEND_TEXT_LINE: f64 = 16.5;
/// Between one entry and the next, across and down.
const ENTRY_GAP_X: f64 = 24.0;
const ENTRY_GAP_Y: f64 = 12.0;

/// The legend for a view, under a drawing of this size: one entry for each
/// different look that has a `notation`, boxes first, in the order they
/// first appear. As many to a row as fit under the drawing.
fn legend(view: &ComputedView, colors: &BTreeMap<String, String>, width: f64, height: f64) -> Option<Legend> {
    enum Sample<'a> {
        Node(&'a ViewNode),
        Edge(&'a c4_model::ViewEdge),
    }
    let mut entries: Vec<(Sample, &str)> = Vec::new();
    for node in view.nodes.iter().filter(|n| !n.notation.is_empty()) {
        let same = |other: &ViewNode| {
            (&other.notation, other.shape, &other.color, other.border)
                == (&node.notation, node.shape, &node.color, node.border)
        };
        if !entries.iter().any(|(s, _)| matches!(s, Sample::Node(other) if same(other))) {
            entries.push((Sample::Node(node), &node.notation));
        }
    }
    for edge in view.edges.iter().filter(|e| !e.notation.is_empty()) {
        let same = |other: &c4_model::ViewEdge| {
            (&other.notation, &other.color, other.line, other.head, other.tail)
                == (&edge.notation, &edge.color, edge.line, edge.head, edge.tail)
        };
        if !entries.iter().any(|(s, _)| matches!(s, Sample::Edge(other) if same(other))) {
            entries.push((Sample::Edge(edge), &edge.notation));
        }
    }
    if entries.is_empty() {
        return None;
    }

    let words: Vec<Vec<String>> = entries
        .iter()
        .map(|(_, notation)| {
            text::clamp_lines(text::wrap(notation, LEGEND_TEXT_MAX, LEGEND_TEXT_SIZE, false), 3)
        })
        .collect();
    let text_w = words
        .iter()
        .flatten()
        .map(|line| text::width(line, LEGEND_TEXT_SIZE, false))
        .fold(0.0, f64::max)
        .ceil();
    let cell_w = SAMPLE_W + SAMPLE_GAP + text_w;
    let room = width - 2.0 * LEGEND_MARGIN - 2.0 * LEGEND_PAD;
    let columns = (((room + ENTRY_GAP_X) / (cell_w + ENTRY_GAP_X)).floor() as usize).clamp(1, entries.len());
    let rows = entries.len().div_ceil(columns);
    let (x, y) = (LEGEND_MARGIN, height);
    let legend_w = 2.0 * LEGEND_PAD + columns as f64 * cell_w + (columns - 1) as f64 * ENTRY_GAP_X;
    let legend_h =
        LEGEND_PAD + LEGEND_TITLE + rows as f64 * SAMPLE_H + (rows - 1) as f64 * ENTRY_GAP_Y + LEGEND_PAD;

    let label = |x: f64, y: f64, text: &str, size: f64, bold: bool| Label {
        x: round(x),
        y: round(y),
        text: text.to_string(),
        size,
        bold,
        color: theme::LABEL_GRAY.to_string(),
        anchor: Anchor::Start,
        role: TextRole::Legend,
    };
    let mut labels = vec![label(x + LEGEND_PAD, y + LEGEND_PAD + 11.0, "Legend", GROUP_TITLE_SIZE, true)];
    let (mut nodes, mut edges) = (Vec::new(), Vec::new());
    for (i, ((sample, notation), lines)) in entries.iter().zip(&words).enumerate() {
        let left = x + LEGEND_PAD + (i % columns) as f64 * (cell_w + ENTRY_GAP_X);
        let top = y + LEGEND_PAD + LEGEND_TITLE + (i / columns) as f64 * (SAMPLE_H + ENTRY_GAP_Y);
        match sample {
            Sample::Node(node) => {
                let swatch = theme::element(&node.color, colors);
                nodes.push(Node {
                    id: String::new(),
                    parent: None,
                    kind: node.kind.clone(),
                    title: notation.to_string(),
                    shape: shape_of(node.shape),
                    group: false,
                    depth: 0,
                    x: round(left),
                    y: round(top),
                    width: SAMPLE_W,
                    height: SAMPLE_H,
                    fill: swatch.fill,
                    stroke: swatch.stroke,
                    fill_opacity: 1.0,
                    border: line_of(node.border),
                    behind: Vec::new(),
                    icon: None,
                    labels: Vec::new(),
                    navigate_to: None,
                });
            }
            Sample::Edge(edge) => {
                let middle = top + SAMPLE_H / 2.0;
                let mut points = vec![(left + SAMPLE_W, middle), (left, middle)];
                let mut markers = Vec::new();
                markers.extend(end_mark(&mut points, edge.tail));
                points.reverse();
                markers.extend(end_mark(&mut points, edge.head));
                edges.push(Edge {
                    id: String::new(),
                    source: String::new(),
                    target: String::new(),
                    path: smooth(&points),
                    color: theme::line(&edge.color, colors),
                    line: line_of(edge.line),
                    markers,
                    labels: Vec::new(),
                    title: notation.to_string(),
                });
            }
        }
        let mut baseline = top
            + (SAMPLE_H - lines.len() as f64 * LEGEND_TEXT_LINE) / 2.0
            + LEGEND_TEXT_LINE / 2.0
            + LEGEND_TEXT_SIZE * 0.35;
        for line in lines {
            labels.push(label(left + SAMPLE_W + SAMPLE_GAP, baseline, line, LEGEND_TEXT_SIZE, false));
            baseline += LEGEND_TEXT_LINE;
        }
    }
    Some(Legend {
        x,
        y,
        width: legend_w,
        height: legend_h,
        stroke: theme::LINE_GRAY.to_string(),
        nodes,
        edges,
        labels,
    })
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

    #[test]
    fn a_sequence_puts_each_step_under_the_last() {
        let source = SHOP.replace(
            "view index { include * }",
            "view index { include * }
            dynamic view buying {
                variant sequence
                include shop
                customer -> shop.ui 'opens the shop'
                shop.ui -> shop.api
                shop.api -> shop.db 'looks up what is for sale, which takes a while'
                customer <- shop.ui 'shows it'
            }",
        );
        let ws = load(&[Source { name: "shop.c4".into(), text: source }]);
        assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
        let d = layout(&ws.model.compute_view("buying").unwrap(), &ws.model.colors);
        // Who takes part, in the order they come into it; no group.
        let ids: Vec<&str> = d.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, ["customer", "shop.ui", "shop.api", "shop.db"]);
        assert!(d.nodes.iter().all(|n| !n.group && n.parent.is_none()));
        // Side by side, standing on one line, a line down from the middle of each.
        assert_eq!(d.lifelines.len(), 4);
        for (pair, line) in d.nodes.windows(2).zip(&d.lifelines) {
            assert!(pair[0].x + pair[0].width < pair[1].x);
            assert_eq!(pair[0].y + pair[0].height, pair[1].y + pair[1].height);
            assert_eq!(line.from.x, pair[0].x + pair[0].width / 2.0);
            assert_eq!(line.from.y, pair[0].y + pair[0].height);
            assert!(line.to.y < d.height);
        }
        // Each step level, from one line down to another, under the one before.
        let mut last = d.lifelines[0].from.y;
        for edge in &d.edges {
            let (start, end) = (edge.path[0], *edge.path.last().unwrap());
            assert_eq!(start.y, end.y);
            assert!(start.y > last && start.y < d.lifelines[0].to.y);
            last = start.y;
            let x_of = |id: &str| d.lifelines.iter().find(|l| l.node == id).unwrap().from.x;
            assert_eq!(start.x, x_of(&edge.source));
            // Its words fit between the two, above the line.
            let (lo, hi) =
                (x_of(&edge.source).min(x_of(&edge.target)), x_of(&edge.source).max(x_of(&edge.target)));
            for label in &edge.labels {
                let half = text::width(&label.text, label.size, label.bold) / 2.0;
                assert!(label.x - half > lo && label.x + half < hi && label.y < start.y, "{label:?}");
            }
        }
        assert_eq!(d.edges[3].title, "4. shows it");
        assert!(d.edges[3].path[0].x > d.edges[3].path.last().unwrap().x, "an answer goes back");
        // Any other view has no lines down.
        assert!(diagram("index").lifelines.is_empty());
    }

    #[test]
    fn a_step_it_takes_alone_is_a_loop_beside_it() {
        for variant in ["variant sequence", "autoLayout TopBottom", "autoLayout LeftRight"] {
            let source = SHOP.replace(
                "view index { include * }",
                &format!(
                    "view index {{ include * }}
                    dynamic view buying {{
                        {variant}
                        customer -> shop.ui 'opens the shop'
                        shop.ui -> shop.api
                        shop.api -> shop.api 'checks the stock'
                        shop.api -> shop.db
                        shop.db -> shop.db 'locks a row'
                    }}"
                ),
            );
            let ws = load(&[Source { name: "shop.c4".into(), text: source }]);
            assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
            let d = layout(&ws.model.compute_view("buying").unwrap(), &ws.model.colors);
            for (step, on) in [("shop.api->shop.api#3", "shop.api"), ("shop.db->shop.db#5", "shop.db")] {
                let edge = d.edges.iter().find(|e| e.id == step).expect(step);
                let start = edge.path[0];
                // Out of one side of it and back to the same side.
                let at = |p: Point| match d.lifelines.iter().find(|l| l.node == on) {
                    Some(line) => p.x == line.from.x,
                    None => {
                        let n = d.nodes.iter().find(|n| n.id == on).unwrap();
                        let side = if variant.ends_with("TopBottom") {
                            p.x == n.x + n.width
                        } else {
                            p.y == n.y + n.height
                        };
                        side && n.x <= p.x && p.x <= n.x + n.width && n.y <= p.y && p.y <= n.y + n.height
                    }
                };
                assert!(at(start) && at(edge.markers[0].points[0]), "{variant}: {step} {edge:?}");
                // Its words beside it (below, when lines go across), clear of every
                // box and inside the drawing.
                let label = &edge.labels[0];
                let half = text::width(&label.text, label.size, label.bold) / 2.0;
                let (left, right) = match label.anchor {
                    Anchor::Start => (label.x, label.x + 2.0 * half),
                    _ => (label.x - half, label.x + half),
                };
                let beside = match variant.ends_with("LeftRight") {
                    true => label.y - label.size > start.y,
                    false => left > start.x,
                };
                assert!(beside && right < d.width && label.y < d.height, "{variant}: {label:?}");
                for n in &d.nodes {
                    let overlaps = left < n.x + n.width
                        && right > n.x
                        && label.y > n.y
                        && label.y - label.size < n.y + n.height;
                    assert!(!overlaps, "{variant}: {label:?} over {}", n.id);
                }
            }
        }
    }

    fn drawn(source: &str) -> Diagram {
        let ws = load(&[Source { name: "t.c4".into(), text: source.into() }]);
        assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
        layout(&ws.model.compute_view("index").unwrap(), &ws.model.colors)
    }

    #[test]
    fn size_padding_and_text_size_change_the_box() {
        let d = drawn(
            "specification { element box }
            model {
                plain = box 'Orders' 'Takes an order from basket to doorstep' 'Rust'
                big = box 'Orders' 'Takes an order from basket to doorstep' 'Rust' { style { size xl } }
                tiny = box 'Orders' 'Takes an order from basket to doorstep' 'Rust' { style { size xs } }
                roomy = box 'Orders' 'Takes an order from basket to doorstep' 'Rust' { style { padding xl } }
                loud = box 'Orders' 'Takes an order from basket to doorstep' 'Rust' { style { textSize xl } }
            }
            views { view index { include * } }",
        );
        let node = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
        let (plain, big, tiny, roomy, loud) =
            (node("plain"), node("big"), node("tiny"), node("roomy"), node("loud"));
        assert!(big.width > plain.width && big.height > plain.height);
        assert!(tiny.width < plain.width && tiny.height < plain.height);
        // The smallest says only its title.
        assert_eq!(tiny.labels.len(), 1);
        assert_eq!(tiny.labels[0].role, TextRole::Title);
        // The same words, the same lines, more room around them.
        assert_eq!(roomy.labels.len(), plain.labels.len());
        assert!(roomy.width > plain.width && roomy.height > plain.height);
        assert_eq!(plain.labels[0].size, TITLE_SIZE);
        assert_eq!(loud.labels[0].size, 21.75);
        assert!(loud.height > plain.height);
        // Text stays inside its box, whatever its size.
        for n in &d.nodes {
            for label in &n.labels {
                let half = text::width(&label.text, label.size, label.bold) / 2.0;
                assert!(
                    label.x - half >= n.x && label.x + half <= n.x + n.width,
                    "{} in {}",
                    label.text,
                    n.id
                );
                assert!(label.y > n.y && label.y < n.y + n.height);
            }
        }
    }

    #[test]
    fn a_multiple_is_a_stack_with_room_behind_it() {
        let d = drawn(
            "specification { element box }
            model {
                one = box 'One'
                many = box 'Many' { style { multiple true } }
                next = box 'Next'
                one -> many
            }
            views { view index { include *; autoLayout LeftRight } }",
        );
        let node = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
        let (one, many, next) = (node("one"), node("many"), node("next"));
        assert!(one.behind.is_empty());
        // The same size in front; the one behind shows past two edges.
        assert_eq!((many.width, many.height), (one.width, one.height));
        assert_eq!(many.behind, [Point { x: many.x + STACK, y: many.y + STACK }]);
        // Nothing is laid over the one behind.
        for other in [one, next] {
            let apart = other.x >= many.x + many.width + STACK
                || other.x + other.width <= many.x
                || other.y >= many.y + many.height + STACK
                || other.y + other.height <= many.y;
            assert!(apart, "{} over the stack", other.id);
        }
    }

    #[test]
    fn an_icon_has_its_place_beside_the_words() {
        let d = drawn(
            "specification { element box; element group }
            model {
                none = box 'Orders' 'Takes them'
                left = box 'Orders' 'Takes them' { icon https://example.com/i.svg }
                right = box 'Orders' 'Takes them' { icon https://example.com/i.svg; style { iconPosition right } }
                top = box 'Orders' 'Takes them' { icon https://example.com/i.svg; style { iconPosition top } }
                bottom = box 'Orders' 'Takes them' { icon https://example.com/i.svg; style { iconPosition bottom; iconSize xl } }
                outer = group 'Outer' { icon https://example.com/i.svg; inner = box 'Inner' }
            }
            views { view index { include *, outer.* } }",
        );
        let node = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
        assert!(node("none").icon.is_none());
        for id in ["left", "right", "top", "bottom"] {
            let n = node(id);
            let icon = n.icon.as_ref().unwrap();
            assert_eq!(icon.href, "https://example.com/i.svg");
            assert_eq!(icon.width, if id == "bottom" { 64.0 } else { 36.0 });
            // Inside the box, and clear of every line of its words.
            assert!(icon.x >= n.x && icon.x + icon.width <= n.x + n.width, "{id}");
            assert!(icon.y >= n.y && icon.y + icon.height <= n.y + n.height, "{id}");
            for label in &n.labels {
                let half = text::width(&label.text, label.size, label.bold) / 2.0;
                let (l, r) = (label.x - half, label.x + half);
                let (t, b) = (label.y - label.size, label.y);
                let clear =
                    r <= icon.x || l >= icon.x + icon.width || b <= icon.y || t >= icon.y + icon.height;
                assert!(clear, "{} under the icon of {id}", label.text);
                assert!(l >= n.x && r <= n.x + n.width, "{} out of {id}", label.text);
            }
        }
        let words = |id: &str| node(id).labels[0].clone();
        let icon = |id: &str| node(id).icon.clone().unwrap();
        assert!(icon("left").x < words("left").x && icon("right").x > words("right").x);
        assert!(icon("top").y < words("top").y && icon("bottom").y > words("bottom").y);
        // A group's is before its title.
        let (outer, mark) = (node("outer"), icon("outer"));
        assert_eq!((mark.x, mark.width), (outer.x + 16.0, 16.0));
        assert_eq!(outer.labels[0].x, mark.x + 22.0);
    }

    #[test]
    fn auto_layout_can_say_how_far_apart() {
        let source = |layout: &str| {
            format!(
                "specification {{ element box }}
                model {{ a = box; b = box; c = box; a -> b; a -> c }}
                views {{ view index {{ include *; {layout} }} }}"
            )
        };
        let d = drawn(&source("autoLayout TopBottom 200 10"));
        let node = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
        let (a, b, c) = (node("a"), node("b"), node("c"));
        assert_eq!(b.y - (a.y + a.height), 200.0);
        assert_eq!(c.x - (b.x + b.width), 10.0);
        // Turned: rows are still what the first number is between.
        let d = drawn(&source("autoLayout LeftRight 200 10"));
        let node = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
        let (a, b, c) = (node("a"), node("b"), node("c"));
        assert_eq!(b.x - (a.x + a.width), 200.0);
        assert_eq!(c.y - (b.y + b.height), 10.0);
    }

    const NOTED: &str = "specification {
                element person { notation 'Someone' style { shape person; color green } }
                element service { notation 'A service we run, day and night, whatever happens' }
                element store { notation 'Data' style { shape storage } }
                element plain
                relationship async { notation 'Does not wait' line dotted; head open }
                relationship sync
            }
            model {
                user = person 'User'
                api = service 'API'
                jobs = service 'Jobs'
                odd = service 'Odd' { notation 'An odd one' }
                db = store 'DB'
                other = plain 'Other'
                user -> api
                api -[async]-> jobs
                api -[async]-> odd
                api -[sync]-> db
                jobs -> other
            }
            views {
                view index { include * }
                view quiet { include other, db with { notation '' } }
                view marked { include api, jobs with { notation 'Marked'; color red } }
            }";

    fn noted(view: &str, legend: bool) -> Diagram {
        let ws = load(&[Source { name: "t.c4".into(), text: NOTED.into() }]);
        assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
        let view = ws.model.compute_view(view).unwrap();
        assert_eq!(has_legend(&view), view.id != "quiet");
        layout_with(&view, &ws.model.colors, Options { legend })
    }

    #[test]
    fn a_legend_has_one_entry_for_each_look_with_a_notation() {
        let without = noted("index", false);
        assert!(without.legend.is_none());
        let with = noted("index", true);
        // There unless it is turned off.
        let ws = load(&[Source { name: "t.c4".into(), text: NOTED.into() }]);
        assert_eq!(layout(&ws.model.compute_view("index").unwrap(), &ws.model.colors), with);
        let legend = with.legend.as_ref().unwrap();
        // The drawing itself is where it was, the legend under it.
        assert_eq!((&with.nodes, &with.edges), (&without.nodes, &without.edges));
        assert_eq!(legend.y, without.height);
        assert!(with.height > without.height && with.width >= without.width);
        assert!(legend.x >= 0.0 && legend.x + legend.width <= with.width);
        assert!(legend.y + legend.height <= with.height);

        // Two services look the same and say the same: one entry. `plain`
        // and `sync` say nothing: none.
        let said: Vec<&str> = legend.nodes.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(
            said,
            ["Someone", "A service we run, day and night, whatever happens", "An odd one", "Data"]
        );
        assert_eq!(legend.nodes[0].shape, Shape::Person);
        assert_eq!(legend.nodes[3].shape, Shape::Storage);
        let lines: Vec<&str> = legend.edges.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(lines, ["Does not wait"]);
        assert_eq!((legend.edges[0].line, legend.edges[0].markers.len()), (LineStyle::Dotted, 1));

        // A sample is nothing to tap, and says nothing itself.
        assert!(legend.nodes.iter().all(|n| n.id.is_empty() && n.labels.is_empty()));
        assert_eq!(legend.labels[0].text, "Legend");
        // A long notation breaks; every line stays inside the frame.
        let texts: Vec<&str> = legend.labels.iter().map(|l| l.text.as_str()).collect();
        assert!(texts.contains(&"A service we run, day and"), "{texts:?}");
        for label in &legend.labels {
            assert_eq!(label.role, TextRole::Legend);
            let right = label.x + text::width(&label.text, label.size, label.bold);
            assert!(label.x >= legend.x && right <= legend.x + legend.width, "{}", label.text);
            assert!(label.y > legend.y && label.y < legend.y + legend.height, "{}", label.text);
        }
        // No two samples share a place.
        let places: Vec<(f64, f64)> = legend
            .nodes
            .iter()
            .map(|n| (n.x, n.y))
            .chain(legend.edges.iter().map(|e| (e.path.last().unwrap().x.floor(), e.path[0].y)))
            .collect();
        for (i, a) in places.iter().enumerate() {
            assert!(places[i + 1..].iter().all(|b| (a.0 - b.0).abs() > 50.0 || (a.1 - b.1).abs() > 20.0));
        }
    }

    #[test]
    fn a_legend_follows_what_the_view_shows() {
        // Nothing with a notation: no legend, though one was asked for.
        let quiet = noted("quiet", true);
        assert!(quiet.legend.is_none());
        assert_eq!(quiet.height, noted("quiet", false).height);
        // A look the view gives is a look of its own, with its own words.
        let marked = noted("marked", true);
        let legend = marked.legend.unwrap();
        let looks: Vec<(&str, &str)> =
            legend.nodes.iter().map(|n| (n.title.as_str(), n.fill.as_str())).collect();
        assert_eq!(
            looks,
            [("A service we run, day and night, whatever happens", "#3b82f6"), ("Marked", "#ac4d39")]
        );
        // Narrow drawing, so one under the other; and the drawing is made
        // wide enough for the legend.
        assert_eq!(legend.nodes[0].x, legend.nodes[1].x);
        assert!(legend.nodes[1].y > legend.nodes[0].y);
        assert!(marked.width >= legend.x + legend.width);
    }
}
