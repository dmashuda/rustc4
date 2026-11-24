//! What the parser produces: the source, structured, with nothing resolved.

pub use crate::lexer::Span;

#[derive(Debug, Default)]
pub struct File {
    pub specs: Vec<SpecItem>,
    pub model: Vec<ModelItem>,
    pub views: Vec<ViewDef>,
    /// For each `views` block, the `style` rules written directly in it:
    /// they apply to every view in that block, and no other.
    pub view_styles: Vec<Vec<ViewRule>>,
    /// What `global { }` blocks define, for views to ask for by name.
    pub globals: Vec<GlobalDef>,
    /// `import { a, b } from 'project'`: each name, where it was written.
    pub imports: Vec<Ref>,
}

/// A named set of rules from a `global { }` block: `style name * { }`,
/// `styleGroup name { }`, `predicateGroup name { }`.
#[derive(Debug)]
pub struct GlobalDef {
    pub name: String,
    pub span: Span,
    /// Asked for with `global style`; otherwise with `global predicate`.
    pub style: bool,
    pub rules: Vec<ViewRule>,
}

/// `key value` pairs from a `style { ... }` block, in order.
#[derive(Debug, Default, Clone)]
pub struct StyleProps(pub Vec<(String, String, Span)>);

#[derive(Debug)]
pub enum SpecItem {
    Element {
        name: String,
        span: Span,
        style: StyleProps,
        technology: Option<String>,
        /// What this kind means, for a legend.
        notation: Option<String>,
    },
    Relationship {
        name: String,
        span: Span,
        style: StyleProps,
        technology: Option<String>,
        notation: Option<String>,
    },
    Tag,
    Color {
        name: String,
        value: String,
        span: Span,
    },
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
    pub summary: Option<String>,
    pub technology: Option<String>,
    pub notation: Option<String>,
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
    pub metadata: Vec<(String, String)>,
    pub notation: Option<String>,
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
    /// Which `views` block of the file it is in: see [`File::view_styles`].
    pub block: usize,
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
    Style {
        targets: Vec<ElementExpr>,
        style: StyleProps,
    },
    /// `autoLayout LeftRight 120 110`: which way, then how far apart rows
    /// are, then how far apart the boxes in a row, if it says.
    AutoLayout {
        direction: Direction,
        rank_gap: Option<f64>,
        node_gap: Option<f64>,
    },
    /// `global style name` / `global predicate name`: the rules a `global`
    /// block gave that name, here.
    Global {
        name: String,
        style: bool,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub enum Predicate {
    /// `with` holds overrides for just these elements in just this view:
    /// style keys, and `title`/`description`/`technology`.
    Element {
        expr: ElementExpr,
        /// `where ...`: only those of them this holds for.
        cond: Option<Condition>,
        with: StyleProps,
    },
    /// With `where`, only relationships it holds for count.
    Relation(RelExpr, Option<Condition>),
}

/// What comes after `where`: `kind is service and not (tag is #old)`.
#[derive(Debug, Clone)]
pub enum Condition {
    Kind {
        of: Whose,
        name: String,
        eq: bool,
    },
    Tag {
        of: Whose,
        name: String,
        eq: bool,
    },
    /// `metadata.key is 'value'`; with no value, that the key is there.
    Metadata {
        of: Whose,
        key: String,
        value: Option<String>,
        eq: bool,
    },
    Not(Box<Condition>),
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
}

/// Whose kind, tag or metadata a condition asks about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whose {
    /// The element's, or the relationship's: `kind is x`.
    Own,
    /// `source.kind is x`, of a relationship's source.
    Source,
    /// `target.kind is x`
    Target,
}

impl Condition {
    /// Whether any part asks about a relationship's `source` or `target`.
    pub fn names_an_end(&self) -> bool {
        match self {
            Condition::Kind { of, .. } | Condition::Tag { of, .. } | Condition::Metadata { of, .. } => {
                *of != Whose::Own
            }
            Condition::Not(c) => c.names_an_end(),
            Condition::And(a, b) | Condition::Or(a, b) => a.names_an_end() || b.names_an_end(),
        }
    }
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
