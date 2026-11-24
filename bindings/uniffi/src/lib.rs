//! UniFFI surface over `c4_ui` for Swift (Apple), and Kotlin when there is
//! an Android app.
//!
//! Proc-macro mode: no .udl file and no build.rs. Bindings are generated
//! from the compiled library's embedded metadata (see //bindings/uniffi:BUILD).
//!
//! The records a renderer draws are `c4_ui`'s and `c4_layout`'s own types,
//! declared below as UniFFI *remote* types: each declaration repeats the
//! type's shape, and UniFFI implements its FFI traits for the real type
//! from that. A field added to or removed from the core without a matching
//! edit below is a build error. There is no mirror type and no conversion.

use std::sync::Mutex;

use c4_layout::{
    Anchor, Diagram, Edge, Icon, Label, Legend, Lifeline, LineStyle, Marker, Node, Point, Shape, TextRole,
};
use c4_model::Source;
use c4_ui::{Controls, Details, Editor, Export, ExportKind, Item, Link, Problem, Screen, Section};

uniffi::setup_scaffolding!();

#[uniffi::remote(Record)]
pub struct Source {
    pub name: String,
    pub text: String,
}

// ---------------------------------------------------------------- diagram --

#[uniffi::remote(Record)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[uniffi::remote(Enum)]
pub enum Shape {
    Rectangle,
    Person,
    Browser,
    Mobile,
    Cylinder,
    Storage,
    Queue,
}

#[uniffi::remote(Enum)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

#[uniffi::remote(Enum)]
pub enum Anchor {
    Start,
    Middle,
}

#[uniffi::remote(Enum)]
pub enum TextRole {
    Title,
    Technology,
    Description,
    GroupTitle,
    EdgeLabel,
    Legend,
}

#[uniffi::remote(Record)]
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

#[uniffi::remote(Record)]
pub struct Icon {
    pub href: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[uniffi::remote(Record)]
pub struct Node {
    pub id: String,
    pub parent: Option<String>,
    pub kind: String,
    pub title: String,
    pub shape: Shape,
    pub group: bool,
    pub depth: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub fill: String,
    pub stroke: String,
    pub fill_opacity: f64,
    pub border: LineStyle,
    pub behind: Vec<Point>,
    pub icon: Option<Icon>,
    pub labels: Vec<Label>,
    pub navigate_to: Option<String>,
}

#[uniffi::remote(Record)]
pub struct Marker {
    pub points: Vec<Point>,
    pub radius: f64,
    pub filled: bool,
    pub closed: bool,
}

#[uniffi::remote(Record)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub path: Vec<Point>,
    pub color: String,
    pub line: LineStyle,
    pub markers: Vec<Marker>,
    pub labels: Vec<Label>,
    pub title: String,
}

#[uniffi::remote(Record)]
pub struct Lifeline {
    pub node: String,
    pub from: Point,
    pub to: Point,
    pub color: String,
    pub line: LineStyle,
}

#[uniffi::remote(Record)]
pub struct Legend {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub stroke: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub labels: Vec<Label>,
}

#[uniffi::remote(Record)]
pub struct Diagram {
    pub id: String,
    pub title: String,
    pub description: String,
    pub width: f64,
    pub height: f64,
    pub font_family: String,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub lifelines: Vec<Lifeline>,
    pub legend: Option<Legend>,
}

// ----------------------------------------------------------------- screen --

#[uniffi::remote(Record)]
pub struct Item {
    pub id: String,
    pub text: String,
    pub detail: String,
    pub hint: String,
    pub selected: bool,
    pub enabled: bool,
}

#[uniffi::remote(Record)]
pub struct Section {
    pub title: String,
    pub rows: Vec<Item>,
}

#[uniffi::remote(Record)]
pub struct Link {
    pub url: String,
    pub text: String,
}

#[uniffi::remote(Record)]
pub struct Details {
    pub title: String,
    pub subtitle: String,
    pub technology: String,
    pub description: String,
    pub tags: Vec<String>,
    pub links: Vec<Link>,
    pub actions: Vec<Item>,
    pub sections: Vec<Section>,
    pub close: Item,
}

#[uniffi::remote(Record)]
pub struct Problem {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub error: bool,
    pub text: String,
}

#[uniffi::remote(Record)]
pub struct Editor {
    pub title: String,
    pub files: Vec<Item>,
    pub add: Item,
    pub remove: Option<Item>,
    pub file: u32,
    pub text: String,
}

#[uniffi::remote(Enum)]
pub enum ExportKind {
    Svg,
    Png,
}

#[uniffi::remote(Record)]
pub struct Export {
    pub kind: ExportKind,
    pub text: String,
    pub hint: String,
    pub file_name: String,
}

#[uniffi::remote(Record)]
pub struct Controls {
    pub zoom_in: String,
    pub zoom_out: String,
    pub fit: String,
    pub open: String,
}

#[uniffi::remote(Record)]
pub struct Screen {
    pub title: String,
    pub description: String,
    pub view: String,
    pub views_title: String,
    pub views: Vec<Item>,
    pub back: Option<Item>,
    pub legend: Option<Item>,
    pub diagram: Option<Diagram>,
    pub empty: String,
    pub selected: String,
    pub related: Vec<String>,
    pub details: Option<Details>,
    pub exports: Vec<Export>,
    pub problems_title: String,
    pub problems: Vec<Problem>,
    pub editor: Option<Editor>,
    pub status: String,
    pub controls: Controls,
}

/// The model a new window opens on, until another is opened.
const EXAMPLE: &str = include_str!("../../../examples/shop.c4");

/// The viewer. UniFFI objects are shared (`Arc`) across the FFI boundary
/// and may be called from any thread, hence the `Mutex`.
#[derive(uniffi::Object)]
pub struct C4Ui {
    inner: Mutex<c4_ui::C4Ui>,
}

#[uniffi::export]
impl C4Ui {
    /// The example model, with its source there to edit.
    #[uniffi::constructor]
    pub fn example() -> Self {
        let sources = vec![Source { name: "shop.c4".into(), text: EXAMPLE.into() }];
        C4Ui { inner: Mutex::new(c4_ui::C4Ui::new(sources, true)) }
    }

    /// `editable`: whether the screen offers the source for editing.
    #[uniffi::constructor]
    pub fn new(sources: Vec<Source>, editable: bool) -> Self {
        C4Ui { inner: Mutex::new(c4_ui::C4Ui::new(sources, editable)) }
    }

    /// What to draw, from scratch.
    pub fn screen(&self) -> Screen {
        self.inner.lock().unwrap().screen()
    }

    /// The user activated the thing with this id.
    pub fn tap(&self, id: String) -> Screen {
        self.inner.lock().unwrap().tap(&id)
    }

    /// The text of source file `file` was edited.
    pub fn set_source(&self, file: u32, text: String) -> Screen {
        self.inner.lock().unwrap().set_source(file as usize, &text)
    }

    /// Another model was opened, or this one's files changed on disk.
    pub fn set_sources(&self, sources: Vec<Source>, editable: bool) -> Screen {
        self.inner.lock().unwrap().set_sources(sources, editable)
    }

    /// The showing diagram as SVG, for saving: empty when there is none.
    /// `background`: a color to paint behind it, or `None` for transparent.
    pub fn svg(&self, background: Option<String>) -> String {
        match self.inner.lock().unwrap().diagram() {
            Some(diagram) => c4_render::svg(diagram, &c4_render::SvgOptions { background }),
            None => String::new(),
        }
    }
}
