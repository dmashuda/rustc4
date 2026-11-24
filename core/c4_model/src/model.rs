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
    /// Not drawn: `border none`, for a box. A relationship always has a line.
    None,
}

impl Line {
    pub(crate) fn parse(value: &str) -> Option<Line> {
        Some(match value {
            "solid" => Line::Solid,
            "dashed" => Line::Dashed,
            "dotted" => Line::Dotted,
            "none" => Line::None,
            _ => return None,
        })
    }
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
    /// A filled arrowhead with a notch in its back.
    Vee,
    /// A crow's foot: three lines spreading to the end.
    Crow,
}

impl Arrow {
    pub(crate) fn parse(value: &str) -> Option<Arrow> {
        Some(match value {
            "none" => Arrow::None,
            "normal" => Arrow::Normal,
            "onormal" => Arrow::ONormal,
            "open" => Arrow::Open,
            "diamond" => Arrow::Diamond,
            "odiamond" => Arrow::ODiamond,
            "dot" => Arrow::Dot,
            "odot" => Arrow::ODot,
            "vee" => Arrow::Vee,
            "crow" => Arrow::Crow,
            _ => return None,
        })
    }
}

/// One of five steps, for a box's `size`, `padding` and `textSize`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Size {
    XSmall,
    Small,
    #[default]
    Medium,
    Large,
    XLarge,
}

impl Size {
    /// `small` or `sm`, and so on.
    pub(crate) fn parse(value: &str) -> Option<Size> {
        Some(match value {
            "xsmall" | "xs" => Size::XSmall,
            "small" | "sm" => Size::Small,
            "medium" | "md" => Size::Medium,
            "large" | "lg" => Size::Large,
            "xlarge" | "xl" => Size::XLarge,
            _ => return None,
        })
    }
}

/// Which side of a box's words its icon is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IconPosition {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl IconPosition {
    pub(crate) fn parse(value: &str) -> Option<IconPosition> {
        Some(match value {
            "left" => IconPosition::Left,
            "right" => IconPosition::Right,
            "top" => IconPosition::Top,
            "bottom" => IconPosition::Bottom,
            _ => return None,
        })
    }
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
    /// How big its box is at the least. At `XSmall` it says only its title.
    pub size: Size,
    /// How much room its box leaves around its words.
    pub padding: Size,
    pub text_size: Size,
    /// Drawn as a stack: there are several of it.
    pub multiple: bool,
    /// An image to show with its words: an `https:` URL, or a `data:` URL
    /// holding the file the source named.
    pub icon: Option<String>,
    /// How big the icon is; as `size`, unless it says.
    pub icon_size: Option<Size>,
    pub icon_position: IconPosition,
    /// What a legend says of it, where a style says: `style a { notation
    /// 'Deprecated' }`. Otherwise its own, or its kind's.
    pub notation: Option<String>,
}

impl Default for ElementStyle {
    fn default() -> Self {
        ElementStyle {
            shape: Shape::Rectangle,
            color: "primary".into(),
            border: Line::Solid,
            opacity: 1.0,
            size: Size::Medium,
            padding: Size::Medium,
            text_size: Size::Medium,
            multiple: false,
            icon: None,
            icon_size: None,
            icon_position: IconPosition::Left,
            notation: None,
        }
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
    /// What a legend says a box that looks like this is: its own, or its
    /// kind's. Empty: it is not in the legend.
    pub notation: String,
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
    /// What a legend says a line that looks like this is. Empty: nothing.
    pub notation: String,
    pub tags: Vec<String>,
    pub metadata: Vec<(String, String)>,
    /// (url, title)
    pub links: Vec<(String, String)>,
    /// Written `a <-> b`: it goes both ways, and is as much one from its
    /// target to its source.
    pub both: bool,
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
    /// What the `views` block it is in says of all its views.
    pub(crate) shared: Vec<ViewRule>,
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
    pub notation: String,
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
    /// What `global { }` blocks named, by (is it a style, its name).
    pub(crate) globals: BTreeMap<(bool, String), Vec<ViewRule>>,
    /// What each `icon` written in the sources turned out to be, by (the
    /// file it is written in, what is written).
    pub(crate) icons: BTreeMap<(usize, String), String>,
    pub(crate) by_fqn: BTreeMap<String, usize>,
}

/// A parsed set of files. Always usable: whatever could not be understood is
/// in `diagnostics` and left out of the model.
#[derive(Debug, Default)]
pub struct Workspace {
    pub model: Model,
    pub diagnostics: Vec<Diagnostic>,
    /// Files the sources name (an icon's image) that were not among them,
    /// each by the name it would have beside the source that asks for it.
    /// Read them, give them to [`load`] with the rest, and they are found.
    pub wanted: Vec<String>,
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
    notation: Option<String>,
    tags: Vec<String>,
}

#[derive(PartialEq)]
struct RelKindSpec {
    style: RelationStyle,
    title: Option<String>,
    description: Option<String>,
    technology: Option<String>,
    notation: Option<String>,
    tags: Vec<String>,
}

struct Builder<'a> {
    sources: &'a [Source],
    model: Model,
    diagnostics: Vec<Diagnostic>,
    kinds: BTreeMap<String, KindSpec>,
    rel_kinds: BTreeMap<String, RelKindSpec>,
    wanted: Vec<String>,
}

/// Whether a source is something the others may name (an icon's image, a
/// description's text), rather than part of the model.
fn is_image(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with(".svg") || is_text(&name)
}

/// What a `descriptionFile` may be.
fn is_text(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [".md", ".markdown", ".mdx", ".txt"].iter().any(|ext| name.ends_with(ext))
}

/// The name of the file a source means by a path written in it.
fn beside(source: &str, path: &str) -> String {
    let mut parts: Vec<&str> = source.split('/').collect();
    parts.pop();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| !p.is_empty() && *p != "..") => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

/// An SVG's text as a URL that holds it.
fn data_url(svg: &str) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::from("data:image/svg+xml;base64,");
    for chunk in svg.as_bytes().chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, &b)| n | (b as u32) << (16 - 8 * i));
        for k in 0..4 {
            out.push(if k <= chunk.len() { DIGITS[(n >> (18 - 6 * k)) as usize & 63] as char } else { '=' });
        }
    }
    out
}

/// Reads every source into one model. A source named `.svg` is an image
/// for the others' icons; the rest are the model's files.
pub fn load(sources: &[Source]) -> Workspace {
    let mut b = Builder {
        sources,
        model: Model::default(),
        diagnostics: Vec::new(),
        kinds: BTreeMap::new(),
        rel_kinds: BTreeMap::new(),
        wanted: Vec::new(),
    };
    let mut files = Vec::new();
    for (i, source) in sources.iter().enumerate() {
        if is_image(&source.name) {
            files.push(ast::File::default());
            continue;
        }
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
                match parse_color(value) {
                    Some((hex, solid)) => {
                        if !solid {
                            b.report(
                                *span,
                                Severity::Warning,
                                format!(
                                    "A see-through color ('{value}') is not supported yet; it is drawn solid."
                                ),
                            );
                        }
                        b.model.colors.insert(name.clone(), hex);
                    }
                    None => b.error(
                        *span,
                        format!("'{value}' is not a color: write it as #rrggbb, or rgb(r, g, b)."),
                    ),
                }
            }
        }
    }
    for file in &files {
        for spec in &file.specs {
            match spec {
                SpecItem::Element(def) => {
                    let mut resolved = ElementStyle::default();
                    b.element_style(&def.style, &mut resolved);
                    // The same kind, specified the same way, in more than
                    // one file is fine: repositories gathered into one
                    // model each carry the specification they were
                    // checked against. Two that disagree is a problem.
                    let spec = KindSpec {
                        style: resolved,
                        technology: def.technology.clone(),
                        notation: def.notation.clone(),
                        tags: def.tags.clone(),
                    };
                    match b.kinds.get(&def.name) {
                        Some(known) if *known == spec => {}
                        Some(_) => b.error(
                            def.span,
                            format!("The element kind '{}' is specified twice, and differently.", def.name),
                        ),
                        None => {
                            b.kinds.insert(def.name.clone(), spec);
                        }
                    }
                }
                SpecItem::Relationship(def) => {
                    let mut resolved = RelationStyle::default();
                    b.relation_style(&def.style, &mut resolved);
                    let spec = RelKindSpec {
                        style: resolved,
                        title: def.title.clone(),
                        description: def.description.clone(),
                        technology: def.technology.clone(),
                        notation: def.notation.clone(),
                        tags: def.tags.clone(),
                    };
                    match b.rel_kinds.get(&def.name) {
                        Some(known) if *known == spec => {}
                        Some(_) => b.error(
                            def.span,
                            format!(
                                "The relationship kind '{}' is specified twice, and differently.",
                                def.name
                            ),
                        ),
                        None => {
                            b.rel_kinds.insert(def.name.clone(), spec);
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
        b.extend_relations(&file.model, None);
    }
    for file in &files {
        for name in &file.imports {
            if b.model.resolve(&name.parts, None).is_none() {
                b.error(
                    name.span,
                    format!(
                        "There is no element '{}' to import: read the files that define it along with these.",
                        name.text()
                    ),
                );
            }
        }
        for global in &file.globals {
            let what = if global.style { "style" } else { "predicate group" };
            let key = (global.style, global.name.clone());
            if b.model.globals.contains_key(&key) {
                b.error(global.span, format!("There is already a global {what} '{}'.", global.name));
                continue;
            }
            for rule in &global.rules {
                b.check_rule(rule, None);
            }
            b.model.globals.insert(key, global.rules.clone());
        }
    }
    for file in &files {
        for rule in file.view_styles.iter().flatten() {
            b.check_rule(rule, None);
        }
        for view in &file.views {
            b.view(view, &file.view_styles[view.block]);
        }
    }
    b.extends();
    let everything = || {
        vec![ViewRule::Include(vec![ast::Predicate::Element {
            expr: ast::ElementExpr::Wildcard,
            cond: None,
            with: StyleProps::default(),
        }])]
    };
    if b.model.views.is_empty() && !b.model.elements.is_empty() {
        b.model.views.push(View {
            id: "index".into(),
            title: "Landscape".into(),
            description: String::new(),
            of: None,
            shared: Vec::new(),
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
            shared: Vec::new(),
            rules: everything(),
            extends: None,
            dynamic: false,
            sequence: false,
            steps: Vec::new(),
        };
        b.model.generated_views.push(view);
    }
    b.diagnostics.sort_by(|x, y| (&x.file, x.line, x.col).cmp(&(&y.file, y.line, y.col)));
    Workspace { model: b.model, diagnostics: b.diagnostics, wanted: b.wanted }
}

/// Text as it was meant: a string written over several indented lines
/// loses the indentation and the empty lines around it.
fn tidy(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let first = lines.iter().position(|l| !l.is_empty()).unwrap_or(lines.len());
    let last = lines.iter().rposition(|l| !l.is_empty()).map_or(first, |i| i + 1);
    lines[first..last].join("\n")
}

/// A color as `#rrggbb`, and whether it is solid: from `#rgb`, `#rrggbb`,
/// `#rrggbbaa`, `rgb(r,g,b)` or `rgba(r,g,b,a)` (as the parser leaves them).
fn parse_color(value: &str) -> Option<(String, bool)> {
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 8 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some((format!("#{}", &hex[..6]), hex[6..].eq_ignore_ascii_case("ff")));
        }
        return parse_hex(value).map(|(r, g, b)| (format!("#{r:02x}{g:02x}{b:02x}"), true));
    }
    let inside = value.strip_prefix("rgba(").or(value.strip_prefix("rgb("))?.strip_suffix(')')?;
    let parts: Vec<&str> = inside.split(',').collect();
    if !(3..=4).contains(&parts.len()) {
        return None;
    }
    let mut rgb = [0u8; 3];
    for (channel, part) in rgb.iter_mut().zip(&parts) {
        *channel = part.parse::<f64>().ok().filter(|n| (0.0..=255.0).contains(n))?.round() as u8;
    }
    let solid = match parts.get(3) {
        None => true,
        Some(a) => match a.strip_suffix('%') {
            Some(percent) => percent.parse::<f64>().ok()? >= 100.0,
            None => a.parse::<f64>().ok()? >= 1.0,
        },
    };
    Some((format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]), solid))
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

    /// The text of a `descriptionFile`: a text file among the sources,
    /// named from the file that asks for it.
    fn description_file(&mut self, path: &str, span: Span) -> Option<String> {
        if !is_text(path) {
            self.report(
                span,
                Severity::Warning,
                format!(
                    "A description from a file like '{path}' is not supported yet; use a .md or .txt file."
                ),
            );
            return None;
        }
        let source = self.sources.get(span.file).map(|s| s.name.as_str()).unwrap_or_default();
        let name = beside(source, path);
        match self.sources.iter().find(|s| s.name == name) {
            Some(file) => Some(file.text.clone()),
            None => {
                if !self.wanted.contains(&name) {
                    self.wanted.push(name.clone());
                }
                self.error(
                    span,
                    format!("There is no file '{name}' for this description: check the path, or write the text as its description."),
                );
                None
            }
        }
    }

    /// What an `icon` means: `Some(None)` for no icon (`icon none`),
    /// `None` when it could not be had, which is then reported.
    fn icon(&mut self, value: &str, span: Span) -> Option<Option<String>> {
        if value == "none" {
            return Some(None);
        }
        let lower = value.to_ascii_lowercase();
        let source = self.sources.get(span.file).map(|s| s.name.as_str()).unwrap_or_default();
        let href = if ["https://", "http://", "data:image/"].iter().any(|p| lower.starts_with(p)) {
            value.to_string()
        } else if value.starts_with("@/") {
            self.report(
                span,
                Severity::Warning,
                "Image aliases ('@/...') are not supported yet; write the path from this file, like './images/logo.svg'.".into(),
            );
            return None;
        } else if value.starts_with("./") || value.starts_with("../") {
            if !lower.ends_with(".svg") {
                self.report(
                    span,
                    Severity::Warning,
                    format!("An icon from a file that is not an SVG ('{value}') is not supported yet; use an SVG, or a URL."),
                );
                return None;
            }
            let name = beside(source, value);
            match self.sources.iter().find(|s| s.name == name) {
                Some(image) => data_url(&image.text),
                None => {
                    if !self.wanted.contains(&name) {
                        self.wanted.push(name.clone());
                    }
                    self.error(
                        span,
                        format!("There is no file '{name}' for this icon: check the path, or use a URL."),
                    );
                    return None;
                }
            }
        } else {
            self.report(
                span,
                Severity::Warning,
                format!("Bundled icons ('{value}') are not supported yet; use a URL, or the path of an SVG file like './{}.svg'.", value.rsplit(':').next().unwrap_or(value)),
            );
            return None;
        };
        self.model.icons.insert((span.file, value.to_string()), href.clone());
        Some(Some(href))
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

    /// `border`: whether it is a box's border, which may be `none`.
    fn line(&mut self, value: &str, span: Span, border: bool) -> Option<Line> {
        match Line::parse(value) {
            Some(Line::None) if !border => {
                self.error(span, "A relationship always has a line: use solid, dashed or dotted.".into());
                None
            }
            Some(line) => Some(line),
            None => {
                let or_none = if border { ", or none" } else { "" };
                self.error(span, format!("Unknown line '{value}': use solid, dashed or dotted{or_none}."));
                None
            }
        }
    }

    pub(crate) fn element_style(&mut self, props: &StyleProps, style: &mut ElementStyle) {
        for (key, value, span) in &props.0 {
            match key.as_str() {
                "shape" => match value.as_str() {
                    "rectangle" => style.shape = Shape::Rectangle,
                    "component" | "document" | "bucket" | "agent" => {
                        self.report(
                            *span,
                            Severity::Warning,
                            format!("The shape '{value}' is not supported yet; it is drawn as a rectangle."),
                        );
                        style.shape = Shape::Rectangle;
                    }
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
                    if let Some(line) = self.line(value, *span, true) {
                        style.border = line;
                    }
                }
                "opacity" => match value.trim_end_matches('%').parse::<f64>() {
                    Ok(n) if (0.0..=100.0).contains(&n) => style.opacity = n / 100.0,
                    _ => self.error(*span, format!("'{value}' is not an opacity: use 0% to 100%.")),
                },
                "size" | "padding" | "textSize" => match Size::parse(value) {
                    Some(size) => match key.as_str() {
                        "size" => style.size = size,
                        "padding" => style.padding = size,
                        _ => style.text_size = size,
                    },
                    None => self.error(
                        *span,
                        format!(
                            "'{value}' is not a size: use xsmall, small, medium, large or xlarge (or xs, sm, md, lg, xl)."
                        ),
                    ),
                },
                "multiple" => match value.as_str() {
                    "true" => style.multiple = true,
                    "false" => style.multiple = false,
                    _ => self.error(*span, format!("'{value}' is not true or false: write 'multiple true'.")),
                },
                "icon" => {
                    if let Some(icon) = self.icon(value, *span) {
                        style.icon = icon;
                    }
                }
                "notation" => style.notation = Some(tidy(value)),
                "iconSize" => match Size::parse(value) {
                    Some(size) => style.icon_size = Some(size),
                    None => self.error(
                        *span,
                        format!(
                            "'{value}' is not a size: use xsmall, small, medium, large or xlarge (or xs, sm, md, lg, xl)."
                        ),
                    ),
                },
                "iconPosition" => match IconPosition::parse(value) {
                    Some(position) => style.icon_position = position,
                    None => self.error(
                        *span,
                        format!("'{value}' is not a side: use left, right, top or bottom."),
                    ),
                },
                // Only the icons LikeC4 bundles take a color, and those are
                // reported where they are asked for.
                "iconColor" => {
                    self.color(value, *span);
                }
                "sizing" => match value.as_str() {
                    "auto" => {}
                    "fixed" => self.report(
                        *span,
                        Severity::Warning,
                        "'sizing fixed' is not supported yet; a box grows to fit its words.".into(),
                    ),
                    _ => self.error(*span, format!("'{value}' is not a sizing: use auto or fixed.")),
                },
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
                    if let Some(line) = self.line(value, *span, false) {
                        style.line = line;
                    }
                }
                "head" | "tail" => {
                    let Some(arrow) = Arrow::parse(value) else {
                        self.error(
                            *span,
                            format!(
                                "Unknown arrow '{value}': use none, normal, onormal, open, vee, crow, diamond, odiamond, dot or odot."
                            ),
                        );
                        continue;
                    };
                    if key == "head" {
                        style.head = arrow;
                    } else {
                        style.tail = arrow;
                    }
                }
                "notation" => {}
                "navigateTo" => self.report(
                    *span,
                    Severity::Warning,
                    "'navigateTo' on a relationship is not supported yet; it goes nowhere.".into(),
                ),
                "multiple" => self.report(
                    *span,
                    Severity::Warning,
                    "'multiple' on a relationship is not supported yet; relationships between the same two boxes are drawn as one line.".into(),
                ),
                _ => self.error(*span, format!("Unknown relationship property '{key}'.")),
            }
        }
    }

    fn elements(&mut self, items: &[ModelItem], parent: Option<usize>) {
        for item in items {
            match item {
                ModelItem::Element(def) => self.element(def, parent),
                ModelItem::Extend { .. } | ModelItem::Relation(_) | ModelItem::ExtendRelation(_) => {}
            }
        }
        // `extend` after the elements beside it, so it can come first in
        // the file.
        for item in items {
            if let ModelItem::Extend { target, more } = item {
                match self.model.resolve(&target.parts, parent) {
                    Some(i) => {
                        // More of the same element: its tags once each,
                        // its metadata and links added to.
                        let el = &mut self.model.elements[i];
                        for tag in &more.tags {
                            if !el.tags.contains(tag) {
                                el.tags.push(tag.clone());
                            }
                        }
                        el.metadata.extend(more.metadata.iter().cloned());
                        el.links.extend(
                            more.links
                                .iter()
                                .map(|(url, title)| (url.clone(), title.clone().unwrap_or_default())),
                        );
                        self.elements(&more.items, Some(i));
                    }
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
        let mut tags = self.kinds.get(&def.kind).map(|spec| spec.tags.clone()).unwrap_or_default();
        tags.extend(def.tags.iter().filter(|t| !tags.contains(t)).cloned().collect::<Vec<_>>());
        let (mut style, kind_technology, kind_notation) = match self.kinds.get(&def.kind) {
            Some(spec) => (spec.style.clone(), spec.technology.clone(), spec.notation.clone()),
            None => {
                let known: Vec<_> = self.kinds.keys().cloned().collect();
                let hint = if known.is_empty() {
                    format!("Add it: specification {{ element {} }}.", def.kind)
                } else {
                    format!("The kinds specified are: {}.", known.join(", "))
                };
                self.error(def.span, format!("Unknown element kind '{}'. {hint}", def.kind));
                (ElementStyle::default(), None, None)
            }
        };
        self.element_style(&def.style, &mut style);
        let mut description = tidy(def.description.as_deref().unwrap_or_default());
        if description.is_empty()
            && let Some((path, span)) = &def.description_file
            && let Some(text) = self.description_file(path, *span)
        {
            description = tidy(&text);
        }
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
            description,
            summary: tidy(def.summary.as_deref().unwrap_or_default()),
            technology: tidy(def.technology.as_deref().or(kind_technology.as_deref()).unwrap_or_default()),
            notation: tidy(def.notation.as_deref().or(kind_notation.as_deref()).unwrap_or_default()),
            tags,
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
                ModelItem::Extend { target, more } => {
                    if let Some(i) = self.model.resolve(&target.parts, scope) {
                        self.relations(&more.items, Some(i));
                    }
                }
                // After every relationship is in: see `extend_relations`.
                ModelItem::ExtendRelation(_) => {}
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
                    // What its kind says of every relationship of that
                    // kind: said again here, it is this one's instead;
                    // tags are both.
                    let of_kind = def.kind.as_ref().and_then(|kind| self.rel_kinds.get(kind));
                    let kind_title = of_kind.and_then(|spec| spec.title.clone());
                    let kind_description = of_kind.and_then(|spec| spec.description.clone());
                    let mut tags = of_kind.map(|spec| spec.tags.clone()).unwrap_or_default();
                    tags.extend(def.tags.iter().filter(|t| !tags.contains(t)).cloned().collect::<Vec<_>>());
                    let (mut style, kind_technology, kind_notation) = match &def.kind {
                        None => (RelationStyle::default(), None, None),
                        Some(kind) => match self.rel_kinds.get(kind) {
                            Some(spec) => {
                                (spec.style.clone(), spec.technology.clone(), spec.notation.clone())
                            }
                            None => {
                                self.error(
                                    def.span,
                                    format!(
                                        "Unknown relationship kind '{kind}'. Add it: specification {{ relationship {kind} }}."
                                    ),
                                );
                                (RelationStyle::default(), None, None)
                            }
                        },
                    };
                    self.relation_style(&def.style, &mut style);
                    // Both ways: the same mark at each end, unless it says.
                    if def.both && style.tail == Arrow::None {
                        style.tail = style.head;
                    }
                    self.model.relations.push(Relation {
                        source,
                        target,
                        kind: def.kind.clone(),
                        title: tidy(def.title.as_deref().or(kind_title.as_deref()).unwrap_or_default()),
                        description: tidy(
                            def.description.as_deref().or(kind_description.as_deref()).unwrap_or_default(),
                        ),
                        technology: tidy(
                            def.technology.as_deref().or(kind_technology.as_deref()).unwrap_or_default(),
                        ),
                        notation: tidy(
                            def.notation.as_deref().or(kind_notation.as_deref()).unwrap_or_default(),
                        ),
                        tags,
                        metadata: def.metadata.clone(),
                        links: def
                            .links
                            .iter()
                            .map(|(url, title)| (url.clone(), title.clone().unwrap_or_default()))
                            .collect(),
                        both: def.both,
                        style,
                        origin: self.origin(def.span),
                    });
                }
            }
        }
    }

    /// `extend a -> b 'title' { ... }`: more tags and metadata for the
    /// relationship between those two, of that kind, and with that title
    /// if one is given.
    fn extend_relations(&mut self, items: &[ModelItem], scope: Option<usize>) {
        for item in items {
            match item {
                ModelItem::Element(def) => {
                    let fqn = match scope {
                        Some(p) => format!("{}.{}", self.model.elements[p].fqn, def.name),
                        None => def.name.clone(),
                    };
                    if let Some(&i) = self.model.by_fqn.get(&fqn) {
                        self.extend_relations(&def.items, Some(i));
                    }
                }
                ModelItem::Extend { target, more } => {
                    if let Some(i) = self.model.resolve(&target.parts, scope) {
                        self.extend_relations(&more.items, Some(i));
                    }
                }
                ModelItem::Relation(_) => {}
                ModelItem::ExtendRelation(def) => {
                    let source = def.source.as_ref().and_then(|r| self.find(r, scope));
                    let target = self.find(&def.target, scope);
                    let (Some(source), Some(target)) = (source, target) else { continue };
                    let title = def.title.as_deref().map(tidy);
                    let mut found = false;
                    for rel in &mut self.model.relations {
                        if rel.source != source
                            || rel.target != target
                            || rel.kind != def.kind
                            || title.as_ref().is_some_and(|t| *t != rel.title)
                        {
                            continue;
                        }
                        found = true;
                        for tag in &def.tags {
                            if !rel.tags.contains(tag) {
                                rel.tags.push(tag.clone());
                            }
                        }
                        rel.metadata.extend(def.metadata.iter().cloned());
                        rel.links.extend(
                            def.links
                                .iter()
                                .map(|(url, title)| (url.clone(), title.clone().unwrap_or_default())),
                        );
                    }
                    if !found {
                        self.error(
                            def.span,
                            "There is no such relationship to extend: name the same two elements, the same kind and the same title as where it is written.".into(),
                        );
                    }
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
            shared: shared_styles.to_vec(),
            rules: def.rules.clone(),
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
        let (mut style, mut title, mut technology, mut notation) = match modelled {
            Some(r) => (r.style.clone(), r.title.clone(), r.technology.clone(), r.notation.clone()),
            None => (RelationStyle::default(), String::new(), String::new(), String::new()),
        };
        if let Some(kind) = &def.kind {
            match self.rel_kinds.get(kind) {
                Some(spec) => {
                    style = spec.style.clone();
                    if let Some(t) = &spec.technology {
                        technology = t.clone();
                    }
                    notation = spec.notation.clone().unwrap_or_default();
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
            notation,
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
                        Predicate::Element { expr, with, .. } => {
                            exprs.push(expr);
                            let mut scratch = ElementStyle::default();
                            let style_only = StyleProps(
                                with.0
                                    .iter()
                                    .filter(|(k, ..)| {
                                        !matches!(
                                            k.as_str(),
                                            "title"
                                                | "description"
                                                | "summary"
                                                | "technology"
                                                | "navigateTo"
                                                | "notation"
                                        )
                                    })
                                    .cloned()
                                    .collect(),
                            );
                            self.element_style(&style_only, &mut scratch);
                        }
                        Predicate::Relation(rel, _, with) => {
                            match rel {
                                RelExpr::Direct(a, b) => exprs.extend([a, b]),
                                RelExpr::Incoming(e) | RelExpr::Outgoing(e) | RelExpr::InOut(e) => {
                                    exprs.push(e)
                                }
                            }
                            // What `with` says of the lines: looks, checked
                            // as a relationship's are; words are as written.
                            let mut scratch = RelationStyle::default();
                            let style_only = StyleProps(
                                with.0
                                    .iter()
                                    .filter(|(k, ..)| {
                                        !matches!(
                                            k.as_str(),
                                            "title" | "description" | "technology" | "notes"
                                        )
                                    })
                                    .cloned()
                                    .collect(),
                            );
                            self.relation_style(&style_only, &mut scratch);
                        }
                    }
                }
            }
            ViewRule::Style { targets, style } => {
                exprs.extend(targets);
                let mut scratch = ElementStyle::default();
                self.element_style(style, &mut scratch);
            }
            ViewRule::AutoLayout { .. } => {}
            ViewRule::Global { name, style, span } => {
                if !self.model.globals.contains_key(&(*style, name.clone())) {
                    let what = if *style { "style" } else { "predicate group" };
                    let known: Vec<&str> = self
                        .model
                        .globals
                        .keys()
                        .filter(|(s, _)| s == style)
                        .map(|(_, n)| n.as_str())
                        .collect();
                    let hint = if known.is_empty() {
                        "Define it in a 'global { }' block.".to_string()
                    } else {
                        format!("The ones defined are: {}.", known.join(", "))
                    };
                    self.error(*span, format!("There is no global {what} '{name}'. {hint}"));
                }
            }
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
