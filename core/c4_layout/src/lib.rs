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
        Line::None => LineStyle::Hidden,
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
    /// A legend over the diagram: a sample of each look whose kind has a
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
        let ranks: Vec<graph::InRank> = view
            .ranks
            .iter()
            .map(|r| graph::InRank {
                rank: r.rank,
                nodes: r.nodes.iter().filter_map(|id| index.get(id.as_str()).copied()).collect(),
            })
            .collect();
        (
            graph::layout_spaced(&in_nodes, &in_edges, view.direction, view.rank_gap, view.node_gap, &ranks),
            Vec::new(),
        )
    };
    let mut lifelines: Vec<Lifeline> = view
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
    let legend = if options.legend { legend(view, colors, width) } else { None };
    if let Some(legend) = &legend {
        // The legend is first, over the drawing: everything drawn so far
        // moves down to make room.
        let down = legend.y + legend.height;
        let label = |l: &mut Label| l.y = round(l.y + down);
        let point = |p: &mut Point| p.y = round(p.y + down);
        for node in &mut nodes {
            node.y = round(node.y + down);
            node.behind.iter_mut().for_each(point);
            node.labels.iter_mut().for_each(label);
            if let Some(icon) = &mut node.icon {
                icon.y = round(icon.y + down);
            }
        }
        for edge in &mut edges {
            edge.path.iter_mut().for_each(point);
            edge.labels.iter_mut().for_each(label);
            for marker in &mut edge.markers {
                marker.points.iter_mut().for_each(point);
            }
        }
        for lifeline in &mut lifelines {
            point(&mut lifeline.from);
            point(&mut lifeline.to);
        }
        width = width.max(legend.x + legend.width + LEGEND_MARGIN);
        height += down;
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
/// The most entries in a row, however wide the drawing.
const LEGEND_COLUMNS: usize = 5;

/// The legend for a view, over a drawing this wide: one entry for each
/// different look that has a `notation`, boxes first, in the order they
/// first appear. As wide as the drawing, in rows of about the same length.
fn legend(view: &ComputedView, colors: &BTreeMap<String, String>, width: f64) -> Option<Legend> {
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
    let fit = (((room + ENTRY_GAP_X) / (cell_w + ENTRY_GAP_X)).floor() as usize).clamp(1, entries.len());
    // Rows of about the same length rather than a full one and a remainder,
    // and never one long thin row.
    let rows = entries.len().div_ceil(fit.min(LEGEND_COLUMNS));
    let columns = entries.len().div_ceil(rows);
    // The frame is as wide as the drawing over it, the columns spread
    // evenly across it.
    let inner = (columns as f64 * cell_w + (columns - 1) as f64 * ENTRY_GAP_X).max(room);
    let pitch = (inner + ENTRY_GAP_X) / columns as f64;
    let (x, y) = (LEGEND_MARGIN, LEGEND_MARGIN);
    let legend_w = 2.0 * LEGEND_PAD + inner;
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
        let left = x + LEGEND_PAD + (i % columns) as f64 * pitch;
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
        // A filled head with a notch in its back.
        Arrow::Vee => (
            Marker {
                points: vec![at(0.0, 0.0), at(11.0, 5.0), at(7.0, 0.0), at(11.0, -5.0)],
                radius: 0.0,
                filled: true,
                closed: true,
            },
            6.5,
        ),
        // A crow's foot: from a point back along the line, out to either
        // side of where it ends. The line itself is the middle toe.
        Arrow::Crow => (
            Marker {
                points: vec![at(0.0, 6.0), at(11.0, 0.0), at(0.0, -6.0)],
                radius: 0.0,
                filled: false,
                closed: false,
            },
            0.0,
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
#[path = "../tests/lib.rs"]
mod tests;
