//! A diagram, laid out: what every front end draws.
//!
//! Everything is decided here: where each box is, what color it is, where
//! each line of text goes and where it breaks, the curve of each line and
//! the outline of its arrowhead. A renderer draws shapes, paths and text
//! at the coordinates given, and decides nothing.
//!
//! Coordinates are pixels from the top left of the diagram.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Shape {
    Rectangle,
    Person,
    Browser,
    Mobile,
    Cylinder,
    Storage,
    Queue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Anchor {
    Start,
    Middle,
}

/// What a piece of text is, for a renderer that styles by meaning (a web
/// page whose labels follow its light or dark theme).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TextRole {
    Title,
    Technology,
    Description,
    GroupTitle,
    EdgeLabel,
}

/// One line of text. `y` is its baseline; `x` its start or its middle,
/// by `anchor`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Label {
    pub x: f64,
    pub y: f64,
    pub text: String,
    pub size: f64,
    pub bold: bool,
    pub color: String,
    pub anchor: Anchor,
    pub role: TextRole,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    /// The element's fully qualified name.
    pub id: String,
    pub parent: Option<String>,
    pub kind: String,
    /// The whole title, for accessibility: `labels` has it as drawn.
    pub title: String,
    pub shape: Shape,
    /// Drawn as a titled border around other nodes, not as a shape.
    pub group: bool,
    /// How many groups it is inside.
    pub depth: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: String,
    pub stroke: String,
    pub fill_opacity: f64,
    pub border: LineStyle,
    pub labels: Vec<Label>,
    /// A view of this element, to open from here.
    pub navigate_to: Option<String>,
}

/// An arrowhead or other end mark.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Marker {
    /// An outline to close and draw; for a circle, its center alone.
    pub points: Vec<Point>,
    /// Zero unless this is a circle.
    pub radius: f64,
    /// Filled with the line's color, or left open.
    pub filled: bool,
    /// Whether the outline's last point joins its first.
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    /// A start point, then three points for each cubic Bézier segment.
    pub path: Vec<Point>,
    pub color: String,
    pub line: LineStyle,
    pub markers: Vec<Marker>,
    pub labels: Vec<Label>,
    /// The whole label, for accessibility.
    pub title: String,
}

/// In a sequence: the line down from a participant, which its steps
/// start and end on. Drawn behind everything else, and not something to
/// tap.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lifeline {
    /// The participant: a node's id.
    pub node: String,
    pub from: Point,
    pub to: Point,
    pub color: String,
    pub line: LineStyle,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagram {
    pub id: String,
    pub title: String,
    pub description: String,
    pub width: f64,
    pub height: f64,
    /// The font the text was measured for.
    pub font_family: String,
    /// Groups before what is inside them: draw in this order.
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Empty unless this is a sequence.
    pub lifelines: Vec<Lifeline>,
}
