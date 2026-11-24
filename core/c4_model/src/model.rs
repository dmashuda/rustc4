//! The model: every file's elements and relationships, resolved and merged.

use std::collections::BTreeMap;

use crate::ast::{self, ElementDef, ModelItem, Ref, SpecItem, StyleProps, ViewDef, ViewRule};
use crate::lexer::Span;
use crate::parser;

/// One source file: a name to report problems against, and its text.
#[derive(Debug, Clone)]
pub struct Source {
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub severity: Severity,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = if self.severity == Severity::Error { "error" } else { "warning" };
        write!(f, "{}:{}:{}: {level}: {}", self.file, self.line, self.col, self.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Rectangle,
    Person,
    Browser,
    Mobile,
    Cylinder,
    Storage,
    Queue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrow {
    None,
    /// A filled triangle.
    Normal,
    /// A hollow triangle.
    ONormal,
    /// Two strokes.
    Open,
    Diamond,
    ODiamond,
    Dot,
    ODot,
}

/// How an element is drawn. A color is a theme name (`primary`, `green`...)
/// or one from `specification { color name #hex }`.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementStyle {
    pub shape: Shape,
    pub color: String,
    pub border: Line,
    /// Of a group's fill, 0 to 1.
    pub opacity: f64,
}

impl Default for ElementStyle {
    fn default() -> Self {
        ElementStyle { shape: Shape::Rectangle, color: "primary".into(), border: Line::Solid, opacity: 1.0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationStyle {
    pub color: String,
    pub line: Line,
    pub head: Arrow,
    pub tail: Arrow,
}

impl Default for RelationStyle {
    fn default() -> Self {
        RelationStyle { color: "gray".into(), line: Line::Dashed, head: Arrow::Normal, tail: Arrow::None }
    }
}

#[derive(Debug, Clone)]
pub struct Element {
    /// `cloud.backend.api`
    pub fqn: String,
    pub name: String,
    pub kind: String,
    pub title: String,
    pub description: String,
    /// What its box says, if not the description.
    pub summary: String,
    pub technology: String,
    pub tags: Vec<String>,
    /// (url, title)
    pub links: Vec<(String, String)>,
    pub metadata: Vec<(String, String)>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub style: ElementStyle,
    pub origin: Origin,
}

impl Element {
    /// What its box says: the summary, or the description if it has none.
    pub fn summary_or_description(&self) -> &str {
        if self.summary.is_empty() { &self.description } else { &self.summary }
    }
}

/// Where something was written: in a model gathered from many
/// repositories, the way from an element back to who owns it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Origin {
    /// The source's name, as it was given to [`load`].
    pub file: String,
    pub line: u32,
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

#[derive(Debug, Clone)]
pub struct Relation {
    pub source: usize,
    pub target: usize,
    pub kind: Option<String>,
    pub title: String,
    pub description: String,
    pub technology: String,
    pub tags: Vec<String>,
    pub style: RelationStyle,
    pub origin: Origin,
}

/// A view as written, with its element references resolved lazily when it
/// is computed (see `view.rs`).
#[derive(Debug)]
pub struct View {
    pub id: String,
    pub title: String,
    pub description: String,
    /// The element this is a view *of*, if any.
    pub of: Option<usize>,
    pub(crate) rules: Vec<ViewRule>,
    /// The view whose rules come before this one's, and where that was written.
    pub(crate) extends: Option<(String, Span)>,
    /// A `dynamic view`: it shows its steps, in order, rather than the
    /// model's relationships.
    pub dynamic: bool,
    /// Drawn as a sequence: a line down from each participant, the steps
    /// one under another.
    pub sequence: bool,
    pub(crate) steps: Vec<Step>,
}

/// One step of a dynamic view, resolved.
#[derive(Debug)]
pub(crate) struct Step {
    /// `3`, or `3.1` for one of several at once.
    pub number: String,
    pub source: usize,
    pub target: usize,
    pub title: String,
    pub technology: String,
    pub notes: String,
    pub style: RelationStyle,
}

#[derive(Debug, Default)]
pub struct Model {
    /// In the order written, parents before their children.
    pub elements: Vec<Element>,
    pub relations: Vec<Relation>,
    pub views: Vec<View>,
    /// A view of each element that has children and no written view of it.
    /// Reached by going into the element, and by id; not listed.
    pub generated_views: Vec<View>,
    /// `specification { color name #hex }`
    pub colors: BTreeMap<String, String>,
    pub(crate) by_fqn: BTreeMap<String, usize>,
}

/// A parsed set of files. Always usable: whatever could not be understood is
/// in `diagnostics` and left out of the model.
#[derive(Debug, Default)]
pub struct Workspace {
    pub model: Model,
    pub diagnostics: Vec<Diagnostic>,
}

impl Workspace {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity == Severity::Error)
    }
}

pub(crate) const THEME_COLORS: [&str; 11] =
    ["primary", "secondary", "muted", "slate", "blue", "indigo", "sky", "red", "gray", "green", "amber"];

impl Model {
    pub fn element(&self, fqn: &str) -> Option<&Element> {
        self.by_fqn.get(fqn).map(|&i| &self.elements[i])
    }

    pub fn index_of(&self, fqn: &str) -> Option<usize> {
        self.by_fqn.get(fqn).copied()
    }

    /// The element a name means when nothing says where it was written: a
    /// full name (`shop.api`), or a short one that only one element has.
    pub fn find(&self, name: &str) -> Option<usize> {
        let parts: Vec<String> = name.split('.').map(str::to_string).collect();
        if parts.iter().any(|p| p.is_empty()) {
            return None;
        }
        self.resolve(&parts, None)
    }

    pub fn view_def(&self, id: &str) -> Option<&View> {
        self.views.iter().chain(&self.generated_views).find(|v| v.id == id)
    }

    /// Whether `a` is `b` or contains it, however deep.
    pub fn is_ancestor_or_self(&self, a: usize, b: usize) -> bool {
        let mut at = Some(b);
        while let Some(i) = at {
            if i == a {
                return true;
            }
            at = self.elements[i].parent;
        }
        false
    }

    pub fn is_ancestor(&self, a: usize, b: usize) -> bool {
        a != b && self.is_ancestor_or_self(a, b)
    }

    /// Finds what a name means where it is written. The first part is looked
    /// for among the children of `scope`, then of each element around it,
    /// then at the top; if it is nowhere on that path but exactly one element
    /// in the whole model has that name, that one is meant. Later parts step
    /// down from there.
    pub(crate) fn resolve(&self, parts: &[String], scope: Option<usize>) -> Option<usize> {
        let child_named = |parent: Option<usize>, name: &str| -> Option<usize> {
            let fqn = match parent {
                Some(p) => format!("{}.{name}", self.elements[p].fqn),
                None => name.to_string(),
            };
            self.by_fqn.get(&fqn).copied()
        };
        let first = &parts[0];
        let mut found = None;
        let mut at = scope;
        loop {
            if let Some(i) = child_named(at, first) {
                found = Some(i);
                break;
            }
            // The scope itself, by its own name: `backend -> db` inside `backend`.
            if let Some(i) = at
                && self.elements[i].name == *first
            {
                found = Some(i);
                break;
            }
            match at {
                Some(i) => at = self.elements[i].parent,
                None => break,
            }
        }
        if found.is_none() {
            let mut named = self.elements.iter().enumerate().filter(|(_, e)| e.name == *first);
            if let (Some((i, _)), None) = (named.next(), named.next()) {
                found = Some(i);
            }
        }
        let mut at = found?;
        for part in &parts[1..] {
            at = child_named(Some(at), part)?;
        }
        Some(at)
    }
}

// ------------------------------------------------------------ building --

#[derive(PartialEq)]
struct KindSpec {
    style: ElementStyle,
    technology: Option<String>,
}

#[derive(PartialEq)]
struct RelKindSpec {
    style: RelationStyle,
    technology: Option<String>,
}

struct Builder<'a> {
    sources: &'a [Source],
    model: Model,
    diagnostics: Vec<Diagnostic>,
    kinds: BTreeMap<String, KindSpec>,
    rel_kinds: BTreeMap<String, RelKindSpec>,
}

/// Reads every source into one model.
pub fn load(sources: &[Source]) -> Workspace {
    let mut b = Builder {
        sources,
        model: Model::default(),
        diagnostics: Vec::new(),
        kinds: BTreeMap::new(),
        rel_kinds: BTreeMap::new(),
    };
    let mut files = Vec::new();
    for (i, source) in sources.iter().enumerate() {
        let (file, errors) = parser::parse(i, &source.text);
        for e in errors {
            let severity =
                if e.message.contains("not supported yet") { Severity::Warning } else { Severity::Error };
            b.report(e.span, severity, e.message);
        }
        files.push(file);
    }

    // Colors first: kinds refer to them.
    for file in &files {
        for spec in &file.specs {
            if let SpecItem::Color { name, value, span } = spec {
                if parse_hex(value).is_some() {
                    b.model.colors.insert(name.clone(), value.clone());
                } else {
                    b.error(*span, format!("'{value}' is not a color: write it as #rrggbb."));
                }
            }
        }
    }
    for file in &files {
        for spec in &file.specs {
            match spec {
                SpecItem::Element { name, span, style, technology } => {
                    let mut resolved = ElementStyle::default();
                    b.element_style(style, &mut resolved);
                    // The same kind, specified the same way, in more than
                    // one file is fine: repositories gathered into one
                    // model each carry the specification they were
                    // checked against. Two that disagree is a problem.
                    let spec = KindSpec { style: resolved, technology: technology.clone() };
                    match b.kinds.get(name) {
                        Some(known) if *known == spec => {}
                        Some(_) => b.error(
                            *span,
                            format!("The element kind '{name}' is specified twice, and differently."),
                        ),
                        None => {
                            b.kinds.insert(name.clone(), spec);
                        }
                    }
                }
                SpecItem::Relationship { name, span, style, technology } => {
                    let mut resolved = RelationStyle::default();
                    b.relation_style(style, &mut resolved);
                    let spec = RelKindSpec { style: resolved, technology: technology.clone() };
                    match b.rel_kinds.get(name) {
                        Some(known) if *known == spec => {}
                        Some(_) => b.error(
                            *span,
                            format!("The relationship kind '{name}' is specified twice, and differently."),
                        ),
                        None => {
                            b.rel_kinds.insert(name.clone(), spec);
                        }
                    }
                }
                SpecItem::Tag | SpecItem::Color { .. } => {}
            }
        }
    }

    // Elements before relationships, so a relationship can name an element
    // written later, or in another file.
    for file in &files {
        b.elements(&file.model, None);
    }
    for file in &files {
        b.relations(&file.model, None);
    }
    for file in &files {
        for view in &file.views {
            b.view(view, &file.view_styles);
        }
    }
    b.extends();
    let everything = || {
        vec![ViewRule::Include(vec![ast::Predicate::Element {
            expr: ast::ElementExpr::Wildcard,
            with: StyleProps::default(),
        }])]
    };
    if b.model.views.is_empty() && !b.model.elements.is_empty() {
        b.model.views.push(View {
            id: "index".into(),
            title: "Landscape".into(),
            description: String::new(),
            of: None,
            rules: everything(),
            extends: None,
            dynamic: false,
            sequence: false,
            steps: Vec::new(),
        });
    }
    // Anything with something inside it can be looked into, whether or not
    // someone wrote a view of it: a model scraped together from a hundred
    // repositories is explored by going in, one level at a time.
    for i in 0..b.model.elements.len() {
        let el = &b.model.elements[i];
        if el.children.is_empty() || b.model.views.iter().any(|v| v.of == Some(i)) {
            continue;
        }
        let mut id = el.fqn.replace('.', "_");
        while b.model.view_def(&id).is_some() {
            id.push('_');
        }
        let view = View {
            id,
            title: el.title.clone(),
            description: String::new(),
            of: Some(i),
            rules: everything(),
            extends: None,
            dynamic: false,
            sequence: false,
            steps: Vec::new(),
        };
        b.model.generated_views.push(view);
    }
    b.diagnostics.sort_by(|x, y| (&x.file, x.line, x.col).cmp(&(&y.file, y.line, y.col)));
    Workspace { model: b.model, diagnostics: b.diagnostics }
}

/// Text as it was meant: a string written over several indented lines
/// loses the indentation and the empty lines around it.
fn tidy(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let first = lines.iter().position(|l| !l.is_empty()).unwrap_or(lines.len());
    let last = lines.iter().rposition(|l| !l.is_empty()).map_or(first, |i| i + 1);
    lines[first..last].join("\n")
}

pub(crate) fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.strip_prefix('#')?;
    let full = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => hex.to_string(),
        _ => return None,
    };
    let n = u32::from_str_radix(&full, 16).ok()?;
    Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

impl Builder<'_> {
    fn report(&mut self, span: Span, severity: Severity, message: String) {
        let file = self.sources.get(span.file).map(|s| s.name.clone()).unwrap_or_default();
        self.diagnostics.push(Diagnostic { file, line: span.line, col: span.col, severity, message });
    }

    fn origin(&self, span: Span) -> Origin {
        Origin {
            file: self.sources.get(span.file).map(|s| s.name.clone()).unwrap_or_default(),
            line: span.line,
        }
    }

    fn error(&mut self, span: Span, message: String) {
        self.report(span, Severity::Error, message);
    }

    fn color(&mut self, value: &str, span: Span) -> Option<String> {
        if THEME_COLORS.contains(&value) || self.model.colors.contains_key(value) {
            return Some(value.to_string());
        }
        self.error(
            span,
            format!(
                "Unknown color '{value}'. Use one of {}, or define it: specification {{ color {value} #rrggbb }}.",
                THEME_COLORS.join(", ")
            ),
        );
        None
    }

    fn line(&mut self, value: &str, span: Span) -> Option<Line> {
        match value {
            "solid" => Some(Line::Solid),
            "dashed" => Some(Line::Dashed),
            "dotted" => Some(Line::Dotted),
            // A border, not a line: drawn solid.
            "none" => Some(Line::Solid),
            _ => {
                self.error(span, format!("Unknown line '{value}': use solid, dashed or dotted."));
                None
            }
        }
    }

    pub(crate) fn element_style(&mut self, props: &StyleProps, style: &mut ElementStyle) {
        for (key, value, span) in &props.0 {
            match key.as_str() {
                "shape" => match value.as_str() {
                    "rectangle" | "component" | "document" | "bucket" => style.shape = Shape::Rectangle,
                    "person" => style.shape = Shape::Person,
                    "browser" => style.shape = Shape::Browser,
                    "mobile" => style.shape = Shape::Mobile,
                    "cylinder" => style.shape = Shape::Cylinder,
                    "storage" => style.shape = Shape::Storage,
                    "queue" => style.shape = Shape::Queue,
                    _ => self.error(
                        *span,
                        format!(
                            "Unknown shape '{value}': use rectangle, person, browser, mobile, cylinder, storage or queue."
                        ),
                    ),
                },
                "color" => {
                    if let Some(color) = self.color(value, *span) {
                        style.color = color;
                    }
                }
                "border" => {
                    if let Some(line) = self.line(value, *span) {
                        style.border = line;
                    }
                }
                "opacity" => match value.trim_end_matches('%').parse::<f64>() {
                    Ok(n) if (0.0..=100.0).contains(&n) => style.opacity = n / 100.0,
                    _ => self.error(*span, format!("'{value}' is not an opacity: use 0% to 100%.")),
                },
                // Accepted so LikeC4 sources load; they change nothing here yet.
                "icon" | "iconColor" | "iconSize" | "size" | "padding" | "textSize" | "multiple" => {}
                _ => self.error(*span, format!("Unknown style property '{key}'.")),
            }
        }
    }

    fn relation_style(&mut self, props: &StyleProps, style: &mut RelationStyle) {
        for (key, value, span) in &props.0 {
            match key.as_str() {
                "color" => {
                    if let Some(color) = self.color(value, *span) {
                        style.color = color;
                    }
                }
                "line" => {
                    if let Some(line) = self.line(value, *span) {
                        style.line = line;
                    }
                }
                "head" | "tail" => {
                    let arrow = match value.as_str() {
                        "none" => Arrow::None,
                        "normal" => Arrow::Normal,
                        "onormal" => Arrow::ONormal,
                        "open" | "vee" | "crow" => Arrow::Open,
                        "diamond" => Arrow::Diamond,
                        "odiamond" => Arrow::ODiamond,
                        "dot" => Arrow::Dot,
                        "odot" => Arrow::ODot,
                        _ => {
                            self.error(
                                *span,
                                format!(
                                    "Unknown arrow '{value}': use none, normal, onormal, open, diamond, odiamond, dot or odot."
                                ),
                            );
                            continue;
                        }
                    };
                    if key == "head" {
                        style.head = arrow;
                    } else {
                        style.tail = arrow;
                    }
                }
                "navigateTo" | "notation" => {}
                _ => self.error(*span, format!("Unknown relationship property '{key}'.")),
            }
        }
    }

    fn elements(&mut self, items: &[ModelItem], parent: Option<usize>) {
        for item in items {
            match item {
                ModelItem::Element(def) => self.element(def, parent),
                ModelItem::Extend { .. } | ModelItem::Relation(_) => {}
            }
        }
        // `extend` after the elements beside it, so it can come first in
        // the file.
        for item in items {
            if let ModelItem::Extend { target, items } = item {
                match self.model.resolve(&target.parts, parent) {
                    Some(i) => self.elements(items, Some(i)),
                    None => {
                        self.error(target.span, format!("There is no element '{}' to extend.", target.text()))
                    }
                }
            }
        }
    }

    fn element(&mut self, def: &ElementDef, parent: Option<usize>) {
        let fqn = match parent {
            Some(p) => format!("{}.{}", self.model.elements[p].fqn, def.name),
            None => def.name.clone(),
        };
        if self.model.by_fqn.contains_key(&fqn) {
            self.error(def.span, format!("There is already an element '{fqn}'."));
            return;
        }
        let (mut style, kind_technology) = match self.kinds.get(&def.kind) {
            Some(spec) => (spec.style.clone(), spec.technology.clone()),
            None => {
                let known: Vec<_> = self.kinds.keys().cloned().collect();
                let hint = if known.is_empty() {
                    format!("Add it: specification {{ element {} }}.", def.kind)
                } else {
                    format!("The kinds specified are: {}.", known.join(", "))
                };
                self.error(def.span, format!("Unknown element kind '{}'. {hint}", def.kind));
                (ElementStyle::default(), None)
            }
        };
        self.element_style(&def.style, &mut style);
        let index = self.model.elements.len();
        self.model.elements.push(Element {
            fqn: fqn.clone(),
            name: def.name.clone(),
            kind: def.kind.clone(),
            title: def
                .title
                .as_deref()
                .map(tidy)
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| def.name.clone()),
            description: tidy(def.description.as_deref().unwrap_or_default()),
            summary: tidy(def.summary.as_deref().unwrap_or_default()),
            technology: tidy(def.technology.as_deref().or(kind_technology.as_deref()).unwrap_or_default()),
            tags: def.tags.clone(),
            links: def
                .links
                .iter()
                .map(|(url, title)| (url.clone(), title.clone().unwrap_or_default()))
                .collect(),
            metadata: def.metadata.clone(),
            parent,
            children: Vec::new(),
            style,
            origin: self.origin(def.span),
        });
        self.model.by_fqn.insert(fqn, index);
        if let Some(p) = parent {
            self.model.elements[p].children.push(index);
        }
        self.elements(&def.items, Some(index));
    }

    fn find(&mut self, reference: &Ref, scope: Option<usize>) -> Option<usize> {
        let found = self.model.resolve(&reference.parts, scope);
        if found.is_none() {
            let name = reference.text();
            let last = reference.parts.last().expect("a reference has a part");
            let similar: Vec<_> = self
                .model
                .elements
                .iter()
                .filter(|e| e.name == *last)
                .map(|e| e.fqn.clone())
                .take(3)
                .collect();
            let hint = if similar.is_empty() {
                String::new()
            } else {
                format!(" Did you mean {}?", similar.join(" or "))
            };
            self.error(reference.span, format!("There is no element '{name}'.{hint}"));
        }
        found
    }

    fn relations(&mut self, items: &[ModelItem], scope: Option<usize>) {
        for item in items {
            match item {
                ModelItem::Element(def) => {
                    let fqn = match scope {
                        Some(p) => format!("{}.{}", self.model.elements[p].fqn, def.name),
                        None => def.name.clone(),
                    };
                    // A duplicate was reported and dropped, and its body with it.
                    if let Some(&i) = self.model.by_fqn.get(&fqn) {
                        self.relations(&def.items, Some(i));
                    }
                }
                ModelItem::Extend { target, items } => {
                    if let Some(i) = self.model.resolve(&target.parts, scope) {
                        self.relations(items, Some(i));
                    }
                }
                ModelItem::Relation(def) => {
                    let source = match &def.source {
                        Some(reference) => self.find(reference, scope),
                        None => {
                            if scope.is_none() {
                                self.error(
                                    def.span,
                                    "A relationship outside any element needs a source: a -> b.".into(),
                                );
                            }
                            scope
                        }
                    };
                    let target = self.find(&def.target, scope);
                    let (Some(source), Some(target)) = (source, target) else { continue };
                    if source == target {
                        self.error(def.span, "A relationship needs two different elements.".into());
                        continue;
                    }
                    let (mut style, kind_technology) = match &def.kind {
                        None => (RelationStyle::default(), None),
                        Some(kind) => match self.rel_kinds.get(kind) {
                            Some(spec) => (spec.style.clone(), spec.technology.clone()),
                            None => {
                                self.error(
                                    def.span,
                                    format!(
                                        "Unknown relationship kind '{kind}'. Add it: specification {{ relationship {kind} }}."
                                    ),
                                );
                                (RelationStyle::default(), None)
                            }
                        },
                    };
                    self.relation_style(&def.style, &mut style);
                    self.model.relations.push(Relation {
                        source,
                        target,
                        kind: def.kind.clone(),
                        title: tidy(def.title.as_deref().unwrap_or_default()),
                        description: tidy(def.description.as_deref().unwrap_or_default()),
                        technology: tidy(
                            def.technology.as_deref().or(kind_technology.as_deref()).unwrap_or_default(),
                        ),
                        tags: def.tags.clone(),
                        style,
                        origin: self.origin(def.span),
                    });
                }
            }
        }
    }

    fn view(&mut self, def: &ViewDef, shared_styles: &[ViewRule]) {
        let of = match &def.of {
            Some(reference) => match self.find(reference, None) {
                Some(i) => Some(i),
                None => return,
            },
            None => None,
        };
        let id = match (&def.id, of) {
            (Some(id), _) => id.clone(),
            // An unnamed view is named after what it shows.
            (None, Some(i)) => self.model.elements[i].fqn.replace('.', "_"),
            (None, None) => "index".to_string(),
        };
        if self.model.views.iter().any(|v| v.id == id) {
            self.error(def.span, format!("There is already a view '{id}'."));
            return;
        }
        let title = match (&def.title, of) {
            (Some(title), _) => tidy(title),
            (None, Some(i)) => self.model.elements[i].title.clone(),
            (None, None) if id == "index" => "Landscape".to_string(),
            (None, None) => id.clone(),
        };
        // Check what the rules name now, so a typo is reported where it is
        // written rather than silently showing nothing.
        for rule in &def.rules {
            self.check_rule(rule, of);
        }
        let mut rules = shared_styles.to_vec();
        rules.extend(def.rules.iter().cloned());
        let mut steps = Vec::new();
        let mut count = 0;
        for item in &def.steps {
            count += 1;
            match item {
                ast::StepItem::One(step) => match self.step(step, count.to_string()) {
                    Some(step) => steps.push(step),
                    None => count -= 1,
                },
                ast::StepItem::Parallel(together) => {
                    let mut part = 0;
                    for step in together {
                        if let Some(step) = self.step(step, format!("{count}.{}", part + 1)) {
                            steps.push(step);
                            part += 1;
                        }
                    }
                    if part == 0 {
                        count -= 1;
                    }
                }
            }
        }
        self.model.views.push(View {
            id,
            title,
            description: tidy(def.description.as_deref().unwrap_or_default()),
            of,
            rules,
            extends: def.extends.clone(),
            dynamic: def.dynamic,
            sequence: def.sequence,
            steps,
        });
    }

    /// A step's ends and looks. Said like the relationship the model has
    /// between the two, unless the step says otherwise.
    fn step(&mut self, def: &ast::StepDef, number: String) -> Option<Step> {
        let source = self.find(&def.source, None);
        let target = self.find(&def.target, None);
        let (source, target) = (source?, target?);
        // To itself is a step (something it does by itself); to what it is
        // part of, or contains, is not.
        if self.model.is_ancestor(source, target) || self.model.is_ancestor(target, source) {
            self.error(def.span, "A step cannot go between an element and something inside it: name two elements side by side, or one twice for a step it takes alone.".into());
            return None;
        }
        // An answer going back is not the relationship that goes forward.
        let modelled = (!def.backward)
            .then(|| {
                let exact = self.model.relations.iter().find(|r| r.source == source && r.target == target);
                // Nothing inside an element speaks for a step it takes alone.
                exact.or_else(|| {
                    self.model.relations.iter().filter(|_| source != target).find(|r| {
                        self.model.is_ancestor_or_self(source, r.source)
                            && self.model.is_ancestor_or_self(target, r.target)
                    })
                })
            })
            .flatten();
        let (mut style, mut title, mut technology) = match modelled {
            Some(r) => (r.style.clone(), r.title.clone(), r.technology.clone()),
            None => (RelationStyle::default(), String::new(), String::new()),
        };
        if let Some(kind) = &def.kind {
            match self.rel_kinds.get(kind) {
                Some(spec) => {
                    style = spec.style.clone();
                    if let Some(t) = &spec.technology {
                        technology = t.clone();
                    }
                }
                None => self.error(
                    def.span,
                    format!(
                        "Unknown relationship kind '{kind}'. Add it: specification {{ relationship {kind} }}."
                    ),
                ),
            }
        }
        self.relation_style(&def.style, &mut style);
        if let Some(t) = &def.title {
            title = tidy(t);
        }
        if let Some(t) = &def.technology {
            technology = tidy(t);
        }
        Some(Step {
            number,
            source,
            target,
            title,
            technology,
            notes: tidy(def.notes.as_deref().unwrap_or_default()),
            style,
        })
    }

    fn check_rule(&mut self, rule: &ViewRule, scope: Option<usize>) {
        use ast::{ElementExpr as E, Predicate, RelExpr};
        let mut exprs: Vec<&E> = Vec::new();
        match rule {
            ViewRule::Include(predicates) | ViewRule::Exclude(predicates) => {
                for predicate in predicates {
                    match predicate {
                        Predicate::Element { expr, with } => {
                            exprs.push(expr);
                            let mut scratch = ElementStyle::default();
                            let style_only = StyleProps(
                                with.0
                                    .iter()
                                    .filter(|(k, ..)| {
                                        !matches!(
                                            k.as_str(),
                                            "title" | "description" | "summary" | "technology" | "navigateTo"
                                        )
                                    })
                                    .cloned()
                                    .collect(),
                            );
                            self.element_style(&style_only, &mut scratch);
                        }
                        Predicate::Relation(RelExpr::Direct(a, b)) => exprs.extend([a, b]),
                        Predicate::Relation(
                            RelExpr::Incoming(e) | RelExpr::Outgoing(e) | RelExpr::InOut(e),
                        ) => exprs.push(e),
                    }
                }
            }
            ViewRule::Style { targets, style } => {
                exprs.extend(targets);
                let mut scratch = ElementStyle::default();
                self.element_style(style, &mut scratch);
            }
            ViewRule::AutoLayout(_) => {}
        }
        for expr in exprs {
            match expr {
                E::Ref(r) | E::Children(r) | E::Descendants(r) | E::Expanded(r) => {
                    self.find(r, scope);
                }
                E::Wildcard | E::Kind { .. } | E::Tag { .. } => {}
            }
        }
    }

    /// `view b extends a`: b shows what a does, then applies its own rules
    /// (the rules are joined when the view is computed). Here: check the
    /// base exists, and take over what it is a view of.
    fn extends(&mut self) {
        for _ in 0..self.model.views.len() {
            for i in 0..self.model.views.len() {
                let Some((base, _)) = self.model.views[i].extends.clone() else { continue };
                let base_of = self.model.view_def(&base).and_then(|v| v.of);
                if self.model.views[i].of.is_none() {
                    self.model.views[i].of = base_of;
                }
            }
        }
        for i in 0..self.model.views.len() {
            let Some((base, span)) = self.model.views[i].extends.clone() else { continue };
            if self.model.view_def(&base).is_none() {
                let id = self.model.views[i].id.clone();
                self.error(span, format!("The view '{id}' extends '{base}', which does not exist."));
            }
        }
    }
}
