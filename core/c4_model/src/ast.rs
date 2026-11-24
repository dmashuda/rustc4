//! What the parser produces: the source, structured, with nothing resolved.

pub use crate::lexer::Span;

#[derive(Debug, Default)]
pub struct File {
    pub specs: Vec<SpecItem>,
    pub model: Vec<ModelItem>,
    pub views: Vec<ViewDef>,
    /// `style` rules written directly in a `views` block: they apply to
    /// every view in it.
    pub view_styles: Vec<ViewRule>,
}

/// `key value` pairs from a `style { ... }` block, in order.
#[derive(Debug, Default, Clone)]
pub struct StyleProps(pub Vec<(String, String, Span)>);

#[derive(Debug)]
pub enum SpecItem {
    Element { name: String, span: Span, style: StyleProps, technology: Option<String> },
    Relationship { name: String, span: Span, style: StyleProps, technology: Option<String> },
    Tag,
    Color { name: String, value: String, span: Span },
}

#[derive(Debug)]
pub enum ModelItem {
    Element(ElementDef),
    Relation(RelationDef),
    Extend { target: Ref, items: Vec<ModelItem> },
}

/// A dotted name as written: `cloud.backend.api`.
#[derive(Debug, Clone)]
pub struct Ref {
    pub parts: Vec<String>,
    pub span: Span,
}

impl Ref {
    pub fn text(&self) -> String {
        self.parts.join(".")
    }
}

#[derive(Debug, Default)]
pub struct ElementDef {
    pub name: String,
    pub kind: String,
    pub span: Span,
    pub title: Option<String>,
    pub description: Option<String>,
    pub technology: Option<String>,
    pub tags: Vec<String>,
    /// (url, title)
    pub links: Vec<(String, Option<String>)>,
    pub style: StyleProps,
    pub metadata: Vec<(String, String)>,
    pub items: Vec<ModelItem>,
}

#[derive(Debug)]
pub struct RelationDef {
    /// `None`: the element it is written inside.
    pub source: Option<Ref>,
    pub target: Ref,
    pub kind: Option<String>,
    pub span: Span,
    pub title: Option<String>,
    pub description: Option<String>,
    pub technology: Option<String>,
    pub tags: Vec<String>,
    pub style: StyleProps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TopBottom,
    BottomTop,
    LeftRight,
    RightLeft,
}

#[derive(Debug)]
pub struct ViewDef {
    pub id: Option<String>,
    pub of: Option<Ref>,
    pub extends: Option<(String, Span)>,
    pub span: Span,
    pub title: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub rules: Vec<ViewRule>,
    /// `dynamic view`: the steps below are what it shows.
    pub dynamic: bool,
    /// `variant sequence`
    pub sequence: bool,
    pub steps: Vec<StepItem>,
}

#[derive(Debug)]
pub enum StepItem {
    One(StepDef),
    /// `parallel { ... }`: steps that happen at once.
    Parallel(Vec<StepDef>),
}

/// One step of a dynamic view: `a -> b 'title'`, or `a <- b` for an answer
/// going back.
#[derive(Debug)]
pub struct StepDef {
    /// Where it starts and ends, whichever way it was written.
    pub source: Ref,
    pub target: Ref,
    /// Written `target <- source`.
    pub backward: bool,
    pub kind: Option<String>,
    pub span: Span,
    pub title: Option<String>,
    pub technology: Option<String>,
    pub notes: Option<String>,
    pub style: StyleProps,
}

#[derive(Debug, Clone)]
pub enum ViewRule {
    Include(Vec<Predicate>),
    Exclude(Vec<Predicate>),
    Style { targets: Vec<ElementExpr>, style: StyleProps },
    AutoLayout(Direction),
}

#[derive(Debug, Clone)]
pub enum Predicate {
    /// `with` holds overrides for just these elements in just this view:
    /// style keys, and `title`/`description`/`technology`.
    Element {
        expr: ElementExpr,
        with: StyleProps,
    },
    Relation(RelExpr),
}

#[derive(Debug, Clone)]
pub enum ElementExpr {
    /// `*`
    Wildcard,
    /// `a.b`
    Ref(Ref),
    /// `a.b.*`
    Children(Ref),
    /// `a.b.**`
    Descendants(Ref),
    /// `a.b._`: the element and its children.
    Expanded(Ref),
    /// `element.kind = x` / `element.kind != x`
    Kind { name: String, eq: bool },
    /// `element.tag = #x` / `element.tag != #x`
    Tag { name: String, eq: bool },
}

#[derive(Debug, Clone)]
pub enum RelExpr {
    /// `-> x`
    Incoming(ElementExpr),
    /// `x ->`
    Outgoing(ElementExpr),
    /// `-> x ->`
    InOut(ElementExpr),
    /// `a -> b`
    Direct(ElementExpr, ElementExpr),
}
