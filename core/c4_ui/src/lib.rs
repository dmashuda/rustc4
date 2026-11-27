//! The viewer as a state machine: sources in, a [`Screen`] out, taps back.
//!
//! A front end draws the screen (a list of views, the diagram, a panel about
//! whatever is selected, the problems found in the source) and reports two
//! things: [`C4Ui::tap`] with the id of what the user activated, and
//! [`C4Ui::set_source`] when the source is edited. It holds no text of its
//! own and decides nothing: which view is showing, what a click on a box
//! does and what the panel says are all here.
//!
//! Ids are stable and readable, because tests address things by them:
//!
//! | id | what a tap does |
//! | --- | --- |
//! | `view.<view>` | shows that view |
//! | `node.<element>` | selects the element (in a diagram or a list) |
//! | `open.<element>` | goes to the closer view of that element |
//! | `edge.<source>-><target>` | selects the line between two elements |
//! | `edge.<source>-><target>#<step>` | selects a step of a dynamic view |
//! | `canvas` | selects nothing |
//! | `back` | returns to the view before this one |
//! | `legend` | takes the legend over the diagram away, or brings it back |
//! | `details.close` | closes the panel |
//! | `file.<n>` | shows that source file in the editor |
//! | `file.add` | adds an empty source file and shows it |
//! | `file.remove` | removes the source file showing, if it is not the only one |

use c4_layout::Diagram;
use c4_model::{Severity, Source, Workspace};
use serde::Serialize;

/// Something to tap. `hint` says what it does, for a tooltip or a screen
/// reader, when `text` alone does not.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    pub id: String,
    pub text: String,
    /// A second, quieter line.
    pub detail: String,
    pub hint: String,
    pub selected: bool,
    pub enabled: bool,
}

impl Item {
    fn new(id: impl Into<String>, text: impl Into<String>) -> Item {
        Item {
            id: id.into(),
            text: text.into(),
            detail: String::new(),
            hint: String::new(),
            selected: false,
            enabled: true,
        }
    }

    fn detail(mut self, detail: impl Into<String>) -> Item {
        self.detail = detail.into();
        self
    }

    fn hint(mut self, hint: impl Into<String>) -> Item {
        self.hint = hint.into();
        self
    }
}

/// A titled list in the details panel. Rows with an empty id are not
/// tappable.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Section {
    pub title: String,
    pub rows: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Link {
    pub url: String,
    pub text: String,
}

/// The panel about what is selected.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Details {
    pub title: String,
    /// What it is: `service · shop.api`.
    pub subtitle: String,
    pub technology: String,
    pub description: String,
    pub tags: Vec<String>,
    pub links: Vec<Link>,
    /// What can be done from here: open the closer view.
    pub actions: Vec<Item>,
    pub sections: Vec<Section>,
    pub close: Item,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Problem {
    pub file: String,
    pub line: u32,
    pub col: u32,
    /// `true`: the model is missing something. `false`: a warning.
    pub error: bool,
    pub text: String,
}

/// The source, where it can be edited.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Editor {
    pub title: String,
    /// One per source file; the selected one is in `text`.
    pub files: Vec<Item>,
    /// Adds a file: a model may be split over several, a specification in
    /// one and the model in another, as LikeC4 projects usually are.
    pub add: Item,
    /// Removes the file showing; `None` when it is the only one.
    pub remove: Option<Item>,
    /// The index to pass to [`C4Ui::set_source`].
    pub file: u32,
    pub text: String,
}

/// What an export button makes; how is the platform's business (a download,
/// a share sheet, a save panel).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ExportKind {
    Svg,
    Png,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Export {
    pub kind: ExportKind,
    pub text: String,
    pub hint: String,
    /// A name for the file, without a folder: `shop.svg`.
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Screen {
    /// The showing view's title.
    pub title: String,
    pub description: String,
    /// The showing view's id, for a front end that keeps it somewhere (a
    /// URL): pass it back as `view.<id>` to return here.
    pub view: String,
    /// The heading over `views`.
    pub views_title: String,
    pub views: Vec<Item>,
    /// To the view before this one; `None` when there is none.
    pub back: Option<Item>,
    /// Shows or hides the legend; `selected` while it shows. `None` when
    /// the view has nothing a legend would explain.
    pub legend: Option<Item>,
    /// The diagram to draw, also available as [`C4Ui::diagram`] and left out
    /// of the JSON (the web draws the SVG instead).
    #[serde(skip)]
    pub diagram: Option<Diagram>,
    /// What to say where the diagram would be, when there is none.
    pub empty: String,
    /// The id of the selected node or edge, as a tap reported it.
    pub selected: String,
    /// With something selected: the ids (as taps report them) of it and of
    /// everything it touches, the lines to and from it and what is at their
    /// other ends. Draw the rest of the diagram faded, so one element's
    /// connections can be read out of a crowded view. Empty: fade nothing.
    pub related: Vec<String>,
    pub details: Option<Details>,
    pub exports: Vec<Export>,
    pub problems_title: String,
    pub problems: Vec<Problem>,
    pub editor: Option<Editor>,
    /// One line about the whole model.
    pub status: String,
    pub controls: Controls,
}

/// Names for what a front end does by itself to the drawing: it knows how
/// to zoom, but not what to call it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Controls {
    pub zoom_in: String,
    pub zoom_out: String,
    pub fit: String,
}

pub struct C4Ui {
    sources: Vec<Source>,
    workspace: Workspace,
    editable: bool,
    view: String,
    /// Views shown before this one, most recent last.
    history: Vec<String>,
    selected: String,
    file: usize,
    diagram: Option<Diagram>,
    /// Whether the legend is wanted, and whether this view would have one.
    legend: bool,
    has_legend: bool,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl C4Ui {
    /// `editable`: whether the screen offers the source for editing (a
    /// playground) or only shows what it describes (a published site).
    pub fn new(sources: Vec<Source>, editable: bool) -> C4Ui {
        let mut ui = C4Ui {
            sources,
            workspace: Workspace::default(),
            editable,
            view: String::new(),
            history: Vec::new(),
            selected: String::new(),
            file: 0,
            diagram: None,
            legend: true,
            has_legend: false,
        };
        ui.reload();
        ui
    }

    /// Reads the sources again and keeps as much of what was showing as
    /// still exists.
    fn reload(&mut self) {
        self.workspace = c4_model::load(&self.sources);
        let model = &self.workspace.model;
        self.history.retain(|id| model.view_def(id).is_some());
        if model.view_def(&self.view).is_none() {
            self.view = self
                .history
                .pop()
                .or_else(|| model.view_def("index").map(|v| v.id.clone()))
                .or_else(|| model.views.first().map(|v| v.id.clone()))
                .unwrap_or_default();
        }
        self.layout();
        self.drop_stale_selection();
    }

    fn layout(&mut self) {
        let model = &self.workspace.model;
        let view = model.compute_view(&self.view);
        self.has_legend = view.as_ref().is_some_and(c4_layout::has_legend);
        let options = c4_layout::Options { legend: self.legend };
        self.diagram = view.map(|view| c4_layout::layout_with(&view, &model.colors, options));
    }

    fn drop_stale_selection(&mut self) {
        let model = &self.workspace.model;
        let still_there = match self.selected.split_once('.') {
            Some(("node", fqn)) => model.element(fqn).is_some(),
            Some(("edge", id)) => self.diagram.as_ref().is_some_and(|d| d.edges.iter().any(|e| e.id == id)),
            _ => false,
        };
        if !still_there {
            self.selected.clear();
        }
    }

    fn show(&mut self, view: &str) {
        if self.workspace.model.view_def(view).is_none() || view == self.view {
            return;
        }
        if !self.view.is_empty() {
            self.history.push(std::mem::take(&mut self.view));
            // Far enough back for anyone; not forever.
            if self.history.len() > 50 {
                self.history.remove(0);
            }
        }
        self.view = view.to_string();
        self.layout();
        // An element stays selected if the new view shows it too.
        let keeps = match self.selected.split_once('.') {
            Some(("node", fqn)) => self.diagram.as_ref().is_some_and(|d| d.nodes.iter().any(|n| n.id == fqn)),
            _ => false,
        };
        if !keeps {
            self.selected.clear();
        }
    }

    /// The diagram showing, laid out.
    pub fn diagram(&self) -> Option<&Diagram> {
        self.diagram.as_ref()
    }

    /// Any view's diagram, for exporting more than the one showing.
    pub fn diagram_of(&self, view: &str) -> Option<Diagram> {
        let model = &self.workspace.model;
        let options = c4_layout::Options { legend: self.legend };
        model.compute_view(view).map(|v| c4_layout::layout_with(&v, &model.colors, options))
    }

    /// The sources as they stand, edits and all: for a front end that keeps
    /// them between visits.
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }

    pub fn view_ids(&self) -> Vec<String> {
        self.workspace.model.views.iter().map(|v| v.id.clone()).collect()
    }

    /// The user activated the thing with this id. Unknown ids do nothing:
    /// a front end may be a version behind.
    pub fn tap(&mut self, id: &str) -> Screen {
        match id.split_once('.') {
            Some(("view", view)) => self.show(view),
            Some(("node", fqn)) => {
                if self.workspace.model.element(fqn).is_some() {
                    self.selected = id.to_string();
                }
            }
            Some(("open", fqn)) => {
                let target = self
                    .diagram
                    .as_ref()
                    .and_then(|d| d.nodes.iter().find(|n| n.id == fqn))
                    .and_then(|n| n.navigate_to.clone());
                if let Some(view) = target {
                    self.show(&view);
                }
            }
            Some(("edge", edge)) => {
                if self.diagram.as_ref().is_some_and(|d| d.edges.iter().any(|e| e.id == edge)) {
                    self.selected = id.to_string();
                }
            }
            Some(("details", "close")) => self.selected.clear(),
            None if id == "legend" => {
                self.legend = !self.legend;
                self.layout();
            }
            Some(("file", "add")) if self.editable => {
                let taken = |name: &str| self.sources.iter().any(|s| s.name == name);
                let name = (self.sources.len() + 1..).map(|n| format!("file-{n}.c4")).find(|n| !taken(n));
                self.sources.push(Source { name: name.expect("a name not taken"), text: String::new() });
                self.file = self.sources.len() - 1;
                self.reload();
            }
            Some(("file", "remove")) if self.editable && self.sources.len() > 1 => {
                self.sources.remove(self.file);
                self.file = self.file.min(self.sources.len() - 1);
                self.reload();
            }
            Some(("file", n)) => {
                if let Some(n) = n.parse::<usize>().ok().filter(|&n| n < self.sources.len()) {
                    self.file = n;
                }
            }
            None if id == "canvas" => self.selected.clear(),
            None if id == "back" => {
                if let Some(view) = self.history.pop() {
                    self.view = view;
                    self.layout();
                    self.drop_stale_selection();
                }
            }
            _ => {}
        }
        self.screen()
    }

    /// The text of source file `file` changed.
    pub fn set_source(&mut self, file: usize, text: &str) -> Screen {
        if let Some(source) = self.sources.get_mut(file)
            && source.text != text
        {
            source.text = text.to_string();
            self.reload();
        }
        self.screen()
    }

    /// Other sources altogether: another model was opened, or the files of
    /// this one changed on disk. As with an edit, what was showing stays if
    /// it still exists.
    pub fn set_sources(&mut self, sources: Vec<Source>, editable: bool) -> Screen {
        let same = self.editable == editable
            && self.sources.len() == sources.len()
            && self.sources.iter().zip(&sources).all(|(a, b)| a.name == b.name && a.text == b.text);
        if !same {
            self.sources = sources;
            self.editable = editable;
            self.file = self.file.min(self.sources.len().saturating_sub(1));
            self.reload();
        }
        self.screen()
    }

    pub fn screen(&self) -> Screen {
        let model = &self.workspace.model;
        let showing = model.view_def(&self.view);
        let views = model
            .views
            .iter()
            .map(|v| {
                let mut item = Item::new(format!("view.{}", v.id), &v.title).hint(&v.description);
                item.selected = v.id == self.view;
                item
            })
            .collect();
        let back = self
            .history
            .last()
            .and_then(|id| model.view_def(id))
            .map(|v| Item::new("back", "Back").hint(format!("Back to {}", v.title)));
        let errors = self.workspace.diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
        let status = if errors > 0 {
            plural(errors, "problem", "problems")
        } else {
            format!(
                "{} · {} · {}",
                plural(model.elements.len(), "element", "elements"),
                plural(model.relations.len(), "relationship", "relationships"),
                plural(model.views.len(), "view", "views")
            )
        };
        let empty = match (&self.diagram, showing) {
            (Some(d), _) if d.nodes.is_empty() => "This view includes nothing yet. Add a line like: include *".to_string(),
            (Some(_), _) => String::new(),
            (None, _) if errors > 0 => "Nothing to draw until the problems in the source are fixed.".to_string(),
            // Nothing was given and nothing can be typed: a viewer waiting
            // to be shown a model.
            (None, _) if !self.editable && self.sources.is_empty() => {
                "No model is open. Open a .c4 file, or a folder of them: this follows it as it is edited."
                    .to_string()
            }
            (None, _) => "Nothing to draw yet. Describe a model: specification { element system } model { a = system 'A' }".to_string(),
        };
        let exports = match &self.diagram {
            Some(d) if !d.nodes.is_empty() => vec![
                Export {
                    kind: ExportKind::Svg,
                    text: "SVG".into(),
                    hint: "Save this diagram as an SVG image".into(),
                    file_name: format!("{}.svg", d.id),
                },
                Export {
                    kind: ExportKind::Png,
                    text: "PNG".into(),
                    hint: "Save this diagram as a PNG image".into(),
                    file_name: format!("{}.png", d.id),
                },
            ],
            _ => Vec::new(),
        };
        let editor = self.editable.then(|| Editor {
            title: "Source".into(),
            files: self
                .sources
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    let mut item = Item::new(format!("file.{i}"), &s.name);
                    item.selected = i == self.file;
                    item
                })
                .collect(),
            add: Item::new("file.add", "+").hint("Add a source file"),
            remove: (self.sources.len() > 1).then(|| {
                let name = self.sources.get(self.file).map(|s| s.name.as_str()).unwrap_or_default();
                Item::new("file.remove", "Remove").hint(format!("Remove {name}"))
            }),
            file: self.file as u32,
            text: self.sources.get(self.file).map(|s| s.text.clone()).unwrap_or_default(),
        });
        Screen {
            title: showing.map(|v| v.title.clone()).unwrap_or_default(),
            description: showing.map(|v| v.description.clone()).unwrap_or_default(),
            view: self.view.clone(),
            views_title: "Views".into(),
            views,
            back,
            legend: self.has_legend.then(|| {
                let mut item = Item::new("legend", "Legend").hint(if self.legend {
                    "Hide what the shapes and lines mean"
                } else {
                    "Show what the shapes and lines mean"
                });
                item.selected = self.legend;
                item
            }),
            diagram: self.diagram.clone(),
            empty,
            selected: self.selected.clone(),
            related: self.related(),
            details: self.details(),
            exports,
            problems_title: if self.workspace.diagnostics.is_empty() {
                String::new()
            } else {
                plural(self.workspace.diagnostics.len(), "problem", "problems")
            },
            problems: self
                .workspace
                .diagnostics
                .iter()
                .map(|d| Problem {
                    file: d.file.clone(),
                    line: d.line,
                    col: d.col,
                    error: d.severity == Severity::Error,
                    text: d.message.clone(),
                })
                .collect(),
            editor,
            status,
            controls: Controls {
                zoom_in: "Zoom in".into(),
                zoom_out: "Zoom out".into(),
                fit: "Fit to window".into(),
            },
        }
    }

    fn related(&self) -> Vec<String> {
        let Some(diagram) = &self.diagram else { return Vec::new() };
        let mut out = Vec::new();
        match self.selected.split_once('.') {
            Some(("node", fqn)) => {
                // The element, and whatever is drawn inside it.
                let inside =
                    |id: &str| id == fqn || id.strip_prefix(fqn).is_some_and(|rest| rest.starts_with('.'));
                if !diagram.nodes.iter().any(|n| n.id == fqn) {
                    return out;
                }
                let mut nodes: Vec<&str> =
                    diagram.nodes.iter().map(|n| n.id.as_str()).filter(|id| inside(id)).collect();
                for edge in &diagram.edges {
                    let (from, to) = (inside(&edge.source), inside(&edge.target));
                    if from || to {
                        out.push(format!("edge.{}", edge.id));
                        nodes.push(if from { &edge.target } else { &edge.source });
                        if from && to {
                            nodes.push(&edge.source);
                        }
                    }
                }
                // What holds any of them, so a group is not faded over
                // a member that is not.
                for node in &diagram.nodes {
                    let around = |id: &&str| {
                        id.strip_prefix(node.id.as_str()).is_some_and(|rest| rest.starts_with('.'))
                    };
                    if nodes.contains(&node.id.as_str()) || nodes.iter().any(around) {
                        out.push(format!("node.{}", node.id));
                    }
                }
            }
            Some(("edge", id)) => {
                if let Some(edge) = diagram.edges.iter().find(|e| e.id == id) {
                    out.push(format!("edge.{}", edge.id));
                    for node in &diagram.nodes {
                        let holds = |end: &str| {
                            end == node.id
                                || end.strip_prefix(node.id.as_str()).is_some_and(|r| r.starts_with('.'))
                        };
                        if holds(&edge.source) || holds(&edge.target) {
                            out.push(format!("node.{}", node.id));
                        }
                    }
                }
            }
            _ => {}
        }
        out
    }

    fn details(&self) -> Option<Details> {
        let model = &self.workspace.model;
        let close = Item::new("details.close", "Close");
        match self.selected.split_once('.')? {
            ("node", fqn) => {
                let index = model.index_of(fqn)?;
                let el = &model.elements[index];
                let mut actions = Vec::new();
                let here = self.diagram.as_ref().and_then(|d| d.nodes.iter().find(|n| n.id == fqn));
                if let Some(view) =
                    here.and_then(|n| n.navigate_to.as_deref()).and_then(|id| model.view_def(id))
                {
                    actions.push(
                        Item::new(format!("open.{fqn}"), format!("Open “{}”", view.title))
                            .hint("A closer view of this element"),
                    );
                }
                let mut sections = Vec::new();
                // A relationship of something inside it counts, and says
                // which part it is about.
                let row = |other: usize, near: usize, rel: &c4_model::Relation, arrow: &str| {
                    let them = &model.elements[other];
                    let mut detail = rel.title.clone();
                    if !rel.technology.is_empty() {
                        detail = if detail.is_empty() {
                            rel.technology.clone()
                        } else {
                            format!("{detail} · {}", rel.technology)
                        };
                    }
                    if near != index {
                        let via = format!("via {}", model.elements[near].title);
                        detail = if detail.is_empty() { via } else { format!("{detail} · {via}") };
                    }
                    Item::new(format!("node.{}", them.fqn), format!("{arrow} {}", them.title))
                        .detail(detail)
                        .hint(&them.fqn)
                };
                let (mut outgoing, mut incoming) = (Vec::new(), Vec::new());
                for rel in &model.relations {
                    let from_here = model.is_ancestor_or_self(index, rel.source);
                    let to_here = model.is_ancestor_or_self(index, rel.target);
                    match (from_here, to_here) {
                        (true, false) => outgoing.push(row(rel.target, rel.source, rel, "→")),
                        (false, true) => incoming.push(row(rel.source, rel.target, rel, "←")),
                        _ => {}
                    }
                }
                if !outgoing.is_empty() {
                    sections.push(Section { title: "Uses".into(), rows: outgoing });
                }
                if !incoming.is_empty() {
                    sections.push(Section { title: "Used by".into(), rows: incoming });
                }
                if !el.children.is_empty() {
                    let rows = el
                        .children
                        .iter()
                        .map(|&c| {
                            let child = &model.elements[c];
                            Item::new(format!("node.{}", child.fqn), &child.title)
                                .detail(&child.kind)
                                .hint(&child.fqn)
                        })
                        .collect();
                    sections.push(Section { title: "Contains".into(), rows });
                }
                let in_views: Vec<Item> = model
                    .views
                    .iter()
                    .filter(|v| v.id != self.view)
                    .filter(|v| {
                        model.compute_view(&v.id).is_some_and(|c| c.nodes.iter().any(|n| n.id == fqn))
                    })
                    .map(|v| Item::new(format!("view.{}", v.id), &v.title).hint(&v.description))
                    .collect();
                if !in_views.is_empty() {
                    sections.push(Section { title: "Also in".into(), rows: in_views });
                }
                if !el.metadata.is_empty() {
                    let rows = el.metadata.iter().map(|(k, v)| Item::new("", k).detail(v)).collect();
                    sections.push(Section { title: "Metadata".into(), rows });
                }
                Some(Details {
                    title: el.title.clone(),
                    subtitle: format!("{} · {}", el.kind, el.fqn),
                    technology: el.technology.clone(),
                    description: if el.description.is_empty() { &el.summary } else { &el.description }
                        .clone(),
                    tags: el.tags.clone(),
                    links: el
                        .links
                        .iter()
                        .map(|(url, title)| Link {
                            url: url.clone(),
                            text: if title.is_empty() { url.clone() } else { title.clone() },
                        })
                        .collect(),
                    actions,
                    sections,
                    close,
                })
            }
            ("edge", id) => {
                let view = model.compute_view(&self.view)?;
                let edge = view.edges.iter().find(|e| e.id == id)?;
                let (source, target) = (model.element(&edge.source)?, model.element(&edge.target)?);
                let rows = edge
                    .relations
                    .iter()
                    .map(|&r| {
                        let rel = &model.relations[r];
                        let (from, to) = (&model.elements[rel.source], &model.elements[rel.target]);
                        let text =
                            if rel.title.is_empty() { "(untitled)".to_string() } else { rel.title.clone() };
                        let mut detail = format!("{} → {}", from.title, to.title);
                        if !rel.technology.is_empty() {
                            detail = format!("{detail} · {}", rel.technology);
                        }
                        Item::new("", text).detail(detail).hint(&rel.description)
                    })
                    .collect::<Vec<_>>();
                let ends = vec![
                    Item::new(format!("node.{}", source.fqn), &source.title)
                        .detail(&source.kind)
                        .hint(&source.fqn),
                    Item::new(format!("node.{}", target.fqn), &target.title)
                        .detail(&target.kind)
                        .hint(&target.fqn),
                ];
                // A step says what happens; the relationships it rides on
                // are in the model, if anyone wrote them.
                let (subtitle, description) = if edge.step.is_empty() {
                    (plural(rows.len(), "relationship", "relationships"), String::new())
                } else {
                    let said = edge.label.split_once(". ").map(|(_, title)| title).unwrap_or_default();
                    let text: Vec<&str> =
                        [said, edge.notes.as_str()].into_iter().filter(|t| !t.is_empty()).collect();
                    (format!("Step {}", edge.step), text.join("\n\n"))
                };
                // Every link of every relationship the line stands for, once.
                let mut links: Vec<Link> = Vec::new();
                for (url, title) in edge.relations.iter().flat_map(|&r| &model.relations[r].links) {
                    if !links.iter().any(|l| l.url == *url) {
                        links.push(Link {
                            url: url.clone(),
                            text: if title.is_empty() { url.clone() } else { title.clone() },
                        });
                    }
                }
                let mut sections = Vec::new();
                if !rows.is_empty() || edge.step.is_empty() {
                    sections.push(Section { title: "Relationships".into(), rows });
                }
                sections.push(Section { title: "Between".into(), rows: ends });
                Some(Details {
                    title: format!("{} → {}", source.title, target.title),
                    subtitle,
                    technology: edge.technology.clone(),
                    description,
                    tags: Vec::new(),
                    links,
                    actions: Vec::new(),
                    sections,
                    close,
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "../tests/lib.rs"]
mod tests;
