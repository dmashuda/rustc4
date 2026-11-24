//! Computing a view: which elements and relationships a view shows.
//!
//! The model is a tree of elements with relationships between any two. A
//! view shows some of the elements; an element drawn with some of its
//! descendants also showing becomes a group around them. A relationship is
//! drawn between the nearest showing ancestors of its two ends, so
//! `customer -> cloud.ui` appears in a landscape view as `customer -> cloud`.

use std::collections::BTreeMap;

use crate::ast::{Direction, ElementExpr, Predicate, RelExpr, StyleProps, ViewRule};
use crate::model::{Arrow, ElementStyle, Line, Model, RelationStyle, Shape, View};

#[derive(Debug, Clone)]
pub struct ViewNode {
    /// The element's fully qualified name.
    pub id: String,
    /// The nearest ancestor that is also in the view.
    pub parent: Option<String>,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub technology: String,
    pub tags: Vec<String>,
    pub shape: Shape,
    pub color: String,
    pub border: Line,
    pub opacity: f64,
    /// Whether any of its descendants are in the view: drawn as a group.
    pub group: bool,
    /// Another view of this element, to go to from here.
    pub navigate_to: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ViewEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: String,
    pub technology: String,
    /// Indexes into [`Model::relations`]: every relationship this line
    /// stands for.
    pub relations: Vec<usize>,
    pub color: String,
    pub line: Line,
    pub head: Arrow,
    pub tail: Arrow,
}

#[derive(Debug, Clone)]
pub struct ComputedView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub direction: Direction,
    /// Parents before their children.
    pub nodes: Vec<ViewNode>,
    pub edges: Vec<ViewEdge>,
}

/// One relationship rule from an `exclude`, with its elements resolved.
enum RelFilter {
    Incoming(Vec<usize>),
    Outgoing(Vec<usize>),
    InOut(Vec<usize>),
    Direct(Vec<usize>, Vec<usize>),
}

struct Computing<'a> {
    model: &'a Model,
    scope: Option<usize>,
    visible: Vec<bool>,
    overrides: BTreeMap<usize, StyleProps>,
    excluded: Vec<RelFilter>,
}

impl Model {
    pub fn view_ids(&self) -> Vec<&str> {
        self.views.iter().map(|v| v.id.as_str()).collect()
    }

    /// The rules of a view, with those it inherits first.
    fn rules_of<'a>(&'a self, view: &'a View, depth: usize) -> Vec<&'a ViewRule> {
        let mut rules = Vec::new();
        if depth < 16
            && let Some((base, _)) = &view.extends
            && let Some(base) = self.view_def(base)
        {
            rules = self.rules_of(base, depth + 1);
        }
        rules.extend(&view.rules);
        rules
    }

    pub fn compute_view(&self, id: &str) -> Option<ComputedView> {
        let view = self.view_def(id)?;
        let mut c = Computing {
            model: self,
            scope: view.of,
            visible: vec![false; self.elements.len()],
            overrides: BTreeMap::new(),
            excluded: Vec::new(),
        };
        let rules = self.rules_of(view, 0);
        let mut direction = Direction::TopBottom;
        for rule in &rules {
            match rule {
                ViewRule::Include(predicates) => predicates.iter().for_each(|p| c.include(p)),
                ViewRule::Exclude(predicates) => predicates.iter().for_each(|p| c.exclude(p)),
                ViewRule::AutoLayout(d) => direction = *d,
                ViewRule::Style { .. } => {}
            }
        }

        // Nodes, in the model's order (parents first).
        let shown: Vec<usize> = (0..self.elements.len()).filter(|&i| c.visible[i]).collect();
        let nearest_shown = |mut i: usize| -> Option<usize> {
            loop {
                if c.visible[i] {
                    return Some(i);
                }
                i = self.elements[i].parent?;
            }
        };
        let mut nodes: Vec<ViewNode> = Vec::new();
        for &i in &shown {
            let el = &self.elements[i];
            let parent = el.parent.and_then(nearest_shown);
            let mut style = el.style.clone();
            let mut node = ViewNode {
                id: el.fqn.clone(),
                parent: parent.map(|p| self.elements[p].fqn.clone()),
                kind: el.kind.clone(),
                title: el.title.clone(),
                description: el.description.clone(),
                technology: el.technology.clone(),
                tags: el.tags.clone(),
                shape: style.shape,
                color: String::new(),
                border: style.border,
                opacity: style.opacity,
                group: false,
                navigate_to: self
                    .views
                    .iter()
                    .chain(&self.generated_views)
                    .find(|v| v.of == Some(i) && v.id != view.id)
                    .map(|v| v.id.clone()),
            };
            // Style rules in order, then what `include x with { }` said.
            for rule in &rules {
                if let ViewRule::Style { targets, style: props } = rule
                    && targets.iter().any(|t| c.matches(t, i))
                {
                    apply_style(self, props, &mut style);
                }
            }
            if let Some(props) = c.overrides.get(&i) {
                apply_style(self, props, &mut style);
                for (key, value, _) in &props.0 {
                    match key.as_str() {
                        "title" => node.title = value.clone(),
                        "description" => node.description = value.clone(),
                        "technology" => node.technology = value.clone(),
                        "navigateTo" if self.view_def(value).is_some() => {
                            node.navigate_to = Some(value.clone())
                        }
                        _ => {}
                    }
                }
            }
            node.shape = style.shape;
            node.color = style.color;
            node.border = style.border;
            node.opacity = style.opacity;
            nodes.push(node);
        }
        let ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        for node in &mut nodes {
            node.group = nodes_have_child(&ids, &shown, self, &node.id);
        }

        // Edges: each relationship, lifted to what is showing.
        let mut edges: Vec<ViewEdge> = Vec::new();
        let mut by_ends: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for (r, rel) in self.relations.iter().enumerate() {
            let (Some(source), Some(target)) = (nearest_shown(rel.source), nearest_shown(rel.target)) else {
                continue;
            };
            if self.is_ancestor_or_self(source, target) || self.is_ancestor(target, source) {
                continue;
            }
            if c.excluded.iter().any(|f| c.filter_matches(f, rel.source, rel.target)) {
                continue;
            }
            let index = *by_ends.entry((source, target)).or_insert_with(|| {
                let (s, t) = (&self.elements[source].fqn, &self.elements[target].fqn);
                edges.push(ViewEdge {
                    id: format!("{s}->{t}"),
                    source: s.clone(),
                    target: t.clone(),
                    label: String::new(),
                    technology: String::new(),
                    relations: Vec::new(),
                    color: String::new(),
                    line: Line::Dashed,
                    head: Arrow::Normal,
                    tail: Arrow::None,
                });
                edges.len() - 1
            });
            edges[index].relations.push(r);
        }
        for edge in &mut edges {
            let rels: Vec<_> = edge.relations.iter().map(|&r| &self.relations[r]).collect();
            // Drawn like the relationship written between exactly these two,
            // if there is one; otherwise like the first.
            let (source, target) = (self.index_of(&edge.source), self.index_of(&edge.target));
            let lead = rels
                .iter()
                .find(|r| Some(r.source) == source && Some(r.target) == target)
                .unwrap_or(&rels[0]);
            let RelationStyle { color, line, head, tail } = lead.style.clone();
            (edge.color, edge.line, edge.head, edge.tail) = (color, line, head, tail);
            let mut titles: Vec<&str> =
                rels.iter().map(|r| r.title.as_str()).filter(|t| !t.is_empty()).collect();
            titles.dedup();
            let unique = titles.iter().all(|t| *t == titles[0]);
            edge.label = match titles.len() {
                0 => String::new(),
                _ if unique => titles[0].to_string(),
                // Several different things go on between these two: say so,
                // and leave the list to whoever asks for the details.
                _ => "[...]".to_string(),
            };
            if rels.len() == 1 || rels.iter().all(|r| r.technology == rels[0].technology) {
                edge.technology = rels[0].technology.clone();
            }
        }

        Some(ComputedView {
            id: view.id.clone(),
            title: view.title.clone(),
            description: view.description.clone(),
            direction,
            nodes,
            edges,
        })
    }
}

fn nodes_have_child(ids: &[String], shown: &[usize], model: &Model, id: &str) -> bool {
    let Some(me) = model.index_of(id) else { return false };
    debug_assert_eq!(ids.len(), shown.len());
    shown.iter().any(|&other| model.is_ancestor(me, other))
}

/// Style keys only; anything unknown was reported when the model was built.
fn apply_style(model: &Model, props: &StyleProps, style: &mut ElementStyle) {
    for (key, value, _) in &props.0 {
        match key.as_str() {
            "shape" => {
                style.shape = match value.as_str() {
                    "person" => Shape::Person,
                    "browser" => Shape::Browser,
                    "mobile" => Shape::Mobile,
                    "cylinder" => Shape::Cylinder,
                    "storage" => Shape::Storage,
                    "queue" => Shape::Queue,
                    "rectangle" | "component" | "document" | "bucket" => Shape::Rectangle,
                    _ => style.shape,
                }
            }
            "color"
                if crate::model::THEME_COLORS.contains(&value.as_str())
                    || model.colors.contains_key(value) =>
            {
                style.color = value.clone();
            }
            "border" => {
                style.border = match value.as_str() {
                    "dashed" => Line::Dashed,
                    "dotted" => Line::Dotted,
                    "solid" | "none" => Line::Solid,
                    _ => style.border,
                }
            }
            "opacity" => {
                if let Ok(n) = value.trim_end_matches('%').parse::<f64>() {
                    style.opacity = (n / 100.0).clamp(0.0, 1.0);
                }
            }
            _ => {}
        }
    }
}

impl Computing<'_> {
    fn find(&self, reference: &crate::ast::Ref) -> Option<usize> {
        self.model.resolve(&reference.parts, self.scope)
    }

    fn descendants(&self, of: usize) -> Vec<usize> {
        (0..self.model.elements.len()).filter(|&i| self.model.is_ancestor(of, i)).collect()
    }

    /// The elements an expression names in an `include`.
    fn elements(&self, expr: &ElementExpr) -> Vec<usize> {
        let model = self.model;
        match expr {
            ElementExpr::Wildcard => match self.scope {
                Some(scope) => {
                    let mut out = vec![scope];
                    out.extend(&model.elements[scope].children);
                    out.extend(self.neighbours(scope, true, true));
                    out
                }
                None => (0..model.elements.len()).filter(|&i| model.elements[i].parent.is_none()).collect(),
            },
            ElementExpr::Ref(r) => self.find(r).into_iter().collect(),
            ElementExpr::Children(r) => {
                self.find(r).map(|i| model.elements[i].children.clone()).unwrap_or_default()
            }
            ElementExpr::Descendants(r) => self.find(r).map(|i| self.descendants(i)).unwrap_or_default(),
            ElementExpr::Expanded(r) => self
                .find(r)
                .map(|i| {
                    let mut out = vec![i];
                    out.extend(&model.elements[i].children);
                    out
                })
                .unwrap_or_default(),
            ElementExpr::Kind { .. } | ElementExpr::Tag { .. } => {
                (0..model.elements.len()).filter(|&i| self.matches(expr, i)).collect()
            }
        }
    }

    /// Whether an expression covers this element: for `exclude` and `style`.
    fn matches(&self, expr: &ElementExpr, i: usize) -> bool {
        let model = self.model;
        let el = &model.elements[i];
        match expr {
            ElementExpr::Wildcard => true,
            ElementExpr::Ref(r) => self.find(r) == Some(i),
            ElementExpr::Children(r) => self.find(r).is_some() && el.parent == self.find(r),
            ElementExpr::Descendants(r) => self.find(r).is_some_and(|a| model.is_ancestor(a, i)),
            ElementExpr::Expanded(r) => self.find(r).is_some_and(|a| a == i || el.parent == Some(a)),
            ElementExpr::Kind { name, eq } => (el.kind == *name) == *eq,
            ElementExpr::Tag { name, eq } => el.tags.contains(name) == *eq,
        }
    }

    /// What `x` has relationships with, outside itself: for each, the
    /// outermost element around the other end that does not also contain
    /// `x`. So the neighbours of `cloud.api` are `cloud.ui` and `customer`,
    /// not `cloud` and not something deep inside another system. Where part
    /// of that other system is already showing, nothing is added: the
    /// relationship will be drawn to that.
    fn neighbours(&self, x: usize, incoming: bool, outgoing: bool) -> Vec<usize> {
        let model = self.model;
        let mut out = Vec::new();
        for rel in &model.relations {
            let source_in = model.is_ancestor_or_self(x, rel.source);
            let target_in = model.is_ancestor_or_self(x, rel.target);
            let other = match (source_in, target_in) {
                (true, false) if outgoing => rel.target,
                (false, true) if incoming => rel.source,
                _ => continue,
            };
            if model.is_ancestor(other, x) {
                continue;
            }
            let mut top = other;
            while let Some(p) = model.elements[top].parent {
                if model.is_ancestor(p, x) {
                    break;
                }
                top = p;
            }
            let already = (0..model.elements.len()).any(|i| {
                self.visible[i]
                    && (model.is_ancestor_or_self(top, i) || model.is_ancestor(i, top))
                    && model.is_ancestor_or_self(i, other)
            });
            if !already && !out.contains(&top) {
                out.push(top);
            }
        }
        out
    }

    fn include(&mut self, predicate: &Predicate) {
        match predicate {
            Predicate::Element { expr, with } => {
                for i in self.elements(expr) {
                    self.visible[i] = true;
                    if !with.0.is_empty() {
                        self.overrides.entry(i).or_default().0.extend(with.0.iter().cloned());
                    }
                }
            }
            Predicate::Relation(rel) => {
                let (expr, incoming, outgoing) = match rel {
                    RelExpr::Incoming(e) => (e, true, false),
                    RelExpr::Outgoing(e) => (e, false, true),
                    RelExpr::InOut(e) => (e, true, true),
                    RelExpr::Direct(a, b) => {
                        // Both ends, where anything goes from one to the other.
                        let (from, to) = (self.elements(a), self.elements(b));
                        for &s in &from {
                            for &t in &to {
                                let related = self.model.relations.iter().any(|r| {
                                    self.model.is_ancestor_or_self(s, r.source)
                                        && self.model.is_ancestor_or_self(t, r.target)
                                });
                                if related && s != t {
                                    self.visible[s] = true;
                                    self.visible[t] = true;
                                }
                            }
                        }
                        return;
                    }
                };
                for x in self.elements(expr) {
                    let others = self.neighbours(x, incoming, outgoing);
                    if !others.is_empty() {
                        self.visible[x] = true;
                    }
                    for other in others {
                        self.visible[other] = true;
                    }
                }
            }
        }
    }

    fn exclude(&mut self, predicate: &Predicate) {
        match predicate {
            Predicate::Element { expr, .. } => {
                for i in 0..self.model.elements.len() {
                    if self.visible[i] && self.matches(expr, i) {
                        self.visible[i] = false;
                    }
                }
            }
            Predicate::Relation(rel) => {
                let filter = match rel {
                    RelExpr::Incoming(e) => RelFilter::Incoming(self.filter_elements(e)),
                    RelExpr::Outgoing(e) => RelFilter::Outgoing(self.filter_elements(e)),
                    RelExpr::InOut(e) => RelFilter::InOut(self.filter_elements(e)),
                    RelExpr::Direct(a, b) => {
                        RelFilter::Direct(self.filter_elements(a), self.filter_elements(b))
                    }
                };
                self.excluded.push(filter);
            }
        }
    }

    /// For a relationship filter, `*` means anything at all.
    fn filter_elements(&self, expr: &ElementExpr) -> Vec<usize> {
        match expr {
            ElementExpr::Wildcard => {
                (0..self.model.elements.len()).filter(|&i| self.model.elements[i].parent.is_none()).collect()
            }
            _ => self.elements(expr),
        }
    }

    fn filter_matches(&self, filter: &RelFilter, source: usize, target: usize) -> bool {
        let within = |set: &[usize], i: usize| set.iter().any(|&a| self.model.is_ancestor_or_self(a, i));
        match filter {
            RelFilter::Incoming(x) => within(x, target) && !within(x, source),
            RelFilter::Outgoing(x) => within(x, source) && !within(x, target),
            RelFilter::InOut(x) => within(x, source) != within(x, target),
            RelFilter::Direct(a, b) => within(a, source) && within(b, target),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Source, load};

    const SHOP: &str = "
        specification {
            element actor { style { shape person; color green } }
            element system
            element component
            relationship async { line dotted; color amber }
            tag next
        }
        model {
            customer = actor 'Customer'
            shop = system 'Shop' {
                ui = component 'Web app' 'What the customer sees' 'React' { style { shape browser } }
                api = component 'API' { #next }
                db = component 'Database' { style { shape storage } }
                ui -> api 'calls'
                api -> db 'reads and writes'
            }
            mail = system 'Mail' {
                smtp = component 'SMTP'
            }
            customer -> shop.ui 'browses'
            customer -> shop 'buys from'
            shop.api -[async]-> mail.smtp 'sends receipts'
            mail -> customer 'delivers'
        }
        views {
            view index { include * }
            view shop of shop { include * ; autoLayout LeftRight }
            view api of shop.api { include * }
            view styled extends index {
                exclude mail
                include shop.*
                style shop.* { color muted }
                style element.tag = #next { color red }
            }
            view picked {
                include customer, -> shop.api ->
                exclude -> mail
            }
        }";

    fn shop() -> crate::Workspace {
        let ws = load(&[Source { name: "shop.c4".into(), text: SHOP.into() }]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| d.to_string()).collect();
        assert!(messages.is_empty(), "{messages:#?}");
        ws
    }

    fn summary(view: &crate::ComputedView) -> (Vec<&str>, Vec<String>) {
        (
            view.nodes.iter().map(|n| n.id.as_str()).collect(),
            view.edges.iter().map(|e| format!("{} | {}", e.id, e.label)).collect(),
        )
    }

    #[test]
    fn a_landscape_lifts_relationships_to_what_shows() {
        let ws = shop();
        let view = ws.model.compute_view("index").unwrap();
        let (nodes, edges) = summary(&view);
        assert_eq!(nodes, ["customer", "shop", "mail"]);
        // Two different things go from the customer to the shop.
        assert_eq!(
            edges,
            ["customer->shop | [...]", "shop->mail | sends receipts", "mail->customer | delivers"]
        );
        assert_eq!(view.title, "Landscape");
        assert_eq!(view.nodes[0].shape, crate::Shape::Person);
        assert_eq!(view.nodes[1].navigate_to.as_deref(), Some("shop"));
        assert_eq!(view.edges[1].line, crate::Line::Dotted);
        assert!(view.nodes.iter().all(|n| !n.group));
    }

    #[test]
    fn a_view_of_an_element_shows_its_children_and_neighbours() {
        let ws = shop();
        let view = ws.model.compute_view("shop").unwrap();
        let (nodes, edges) = summary(&view);
        assert_eq!(nodes, ["customer", "shop", "shop.ui", "shop.api", "shop.db", "mail"]);
        assert_eq!(
            edges,
            [
                "shop.ui->shop.api | calls",
                "shop.api->shop.db | reads and writes",
                "customer->shop.ui | browses",
                "customer->shop | buys from",
                "shop.api->mail | sends receipts",
                "mail->customer | delivers",
            ]
        );
        assert!(view.nodes[1].group);
        assert_eq!(view.nodes[2].parent.as_deref(), Some("shop"));
        assert_eq!(view.nodes[1].navigate_to, None, "not to the view it is in");
        assert_eq!(view.nodes[3].navigate_to.as_deref(), Some("api"));
        assert_eq!(view.direction, crate::Direction::LeftRight);
        assert_eq!(view.title, "Shop");
    }

    #[test]
    fn neighbours_are_as_far_out_as_they_can_be() {
        let ws = shop();
        let view = ws.model.compute_view("api").unwrap();
        let (nodes, edges) = summary(&view);
        // Siblings stay siblings; another system shows as the system.
        assert_eq!(nodes, ["shop.ui", "shop.api", "shop.db", "mail"]);
        assert_eq!(
            edges,
            [
                "shop.ui->shop.api | calls",
                "shop.api->shop.db | reads and writes",
                "shop.api->mail | sends receipts"
            ]
        );
    }

    #[test]
    fn extending_excluding_and_styling() {
        let ws = shop();
        let view = ws.model.compute_view("styled").unwrap();
        let (nodes, _) = summary(&view);
        assert_eq!(nodes, ["customer", "shop", "shop.ui", "shop.api", "shop.db"]);
        let color = |id: &str| view.nodes.iter().find(|n| n.id == id).unwrap().color.as_str();
        assert_eq!(color("customer"), "green");
        assert_eq!(color("shop.ui"), "muted");
        assert_eq!(color("shop.api"), "red", "later rules win");

        let view = ws.model.compute_view("picked").unwrap();
        let (nodes, edges) = summary(&view);
        assert_eq!(nodes, ["customer", "shop.ui", "shop.api", "shop.db", "mail"]);
        assert!(!edges.iter().any(|e| e.contains("->mail")), "{edges:?}");
    }

    #[test]
    fn names_resolve_from_where_they_are_written() {
        let ws = load(&[
            Source {
                name: "a.c4".into(),
                text: "specification { element box }
                       model { a = box { inner = box; inner -> other } }"
                    .into(),
            },
            Source {
                name: "b.c4".into(),
                text: "model { b = box { other = box } extend a { extra = box { -> inner } } nowhere -> a }"
                    .into(),
            },
        ]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| d.to_string()).collect();
        assert_eq!(messages, ["b.c4:1:71: error: There is no element 'nowhere'."]);
        let ends: Vec<_> = ws
            .model
            .relations
            .iter()
            .map(|r| (ws.model.elements[r.source].fqn.as_str(), ws.model.elements[r.target].fqn.as_str()))
            .collect();
        assert_eq!(ends, [("a.inner", "b.other"), ("a.extra", "a.inner")]);
        // No views written: a landscape is there anyway, and a way into
        // each element that has something inside.
        assert_eq!(ws.model.view_ids(), ["index"]);
        let landscape = ws.model.compute_view("index").unwrap();
        assert_eq!(landscape.nodes[0].navigate_to.as_deref(), Some("a"));
        let inside: Vec<_> = ws.model.compute_view("a").unwrap().nodes.iter().map(|n| n.id.clone()).collect();
        assert_eq!(inside, ["a", "a.inner", "b", "a.extra"]);
        assert_eq!(ws.model.compute_view("a").unwrap().title, "a");
    }

    #[test]
    fn text_over_several_lines_loses_its_indentation() {
        let ws = load(&[Source {
            name: "t.c4".into(),
            text: "specification { element box }
                model {
                    a = box '  A  ' {
                        description '
                            First line
                              and the second

                            after a gap
                        '
                    }
                    b = box ''
                }"
            .into(),
        }]);
        assert!(ws.diagnostics.is_empty());
        let a = ws.model.element("a").unwrap();
        assert_eq!(a.title, "A");
        assert_eq!(a.description, "First line\nand the second\n\nafter a gap");
        assert_eq!(ws.model.element("b").unwrap().title, "b", "an empty title is no title");
    }

    #[test]
    fn files_may_repeat_a_specification_but_not_contradict_it() {
        let file = |name: &str, text: &str| Source { name: name.into(), text: text.into() };
        let spec =
            "specification { element box { style { color green } } relationship async { line dotted } }";
        let ws = load(&[
            file("one/a.c4", &format!("{spec} model {{ a = box }}")),
            file("two/b.c4", &format!("{spec} model {{ b = box; b -[async]-> a }}")),
            file("three/c.c4", "specification { element box { style { color red } } }"),
        ]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| d.to_string()).collect();
        assert_eq!(
            messages,
            ["three/c.c4:1:17: error: The element kind 'box' is specified twice, and differently."]
        );
        assert_eq!(ws.model.relations.len(), 1);
        // Each remembers the file it was written in.
        assert_eq!(ws.model.element("b").unwrap().origin.to_string(), "two/b.c4:1");
        assert_eq!(ws.model.relations[0].origin.file, "two/b.c4");
    }

    #[test]
    fn mistakes_are_explained() {
        let ws = load(&[Source {
            name: "m.c4".into(),
            text: "specification { element box { style { shape circle; color pink } } }
model {
  a = box
  a = box
  b = widget
  a -> a
}
views { view v of missing { include * } }"
                .into(),
        }]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| format!("{}:{}", d.line, d.message)).collect();
        assert_eq!(messages.len(), 6, "{messages:#?}");
        assert!(messages[0].starts_with("1:Unknown shape 'circle'"));
        assert!(messages[1].starts_with("1:Unknown color 'pink'"));
        assert_eq!(messages[2], "4:There is already an element 'a'.");
        assert_eq!(messages[3], "5:Unknown element kind 'widget'. The kinds specified are: box.");
        assert_eq!(messages[4], "6:A relationship needs two different elements.");
        assert_eq!(messages[5], "8:There is no element 'missing'.");
    }
}
