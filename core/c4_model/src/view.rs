//! Computing a view: which elements and relationships a view shows.
//!
//! The model is a tree of elements with relationships between any two. A
//! view shows some of the elements; an element drawn with some of its
//! descendants also showing becomes a group around them. A relationship is
//! drawn between the nearest showing ancestors of its two ends, so
//! `customer -> cloud.ui` appears in a landscape view as `customer -> cloud`.

use std::collections::BTreeMap;

use crate::ast::{Condition, Direction, ElementExpr, Predicate, RelExpr, StyleProps, ViewRule, Whose};
use crate::model::{
    Arrow, ElementStyle, IconPosition, Line, Model, Relation, RelationStyle, Shape, Size, View,
};

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
    /// What a legend says a box that looks like this is; empty for none.
    pub notation: String,
    pub tags: Vec<String>,
    pub shape: Shape,
    pub color: String,
    pub border: Line,
    pub opacity: f64,
    pub size: Size,
    pub padding: Size,
    pub text_size: Size,
    /// Drawn as a stack: there are several of it.
    pub multiple: bool,
    /// An image to show with its words: a URL, `https:` or `data:`.
    pub icon: Option<String>,
    pub icon_size: Size,
    pub icon_position: IconPosition,
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
    /// In a dynamic view, which step this is (`3`, `3.1`); empty otherwise.
    pub step: String,
    /// What a step's `notes` say.
    pub notes: String,
    /// What a legend says a line that looks like this is; empty for none.
    pub notation: String,
}

#[derive(Debug, Clone)]
pub struct ComputedView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub direction: Direction,
    /// How far apart rows are, and the boxes in a row, if the view says:
    /// `autoLayout TopBottom 120 110`.
    pub rank_gap: Option<f64>,
    pub node_gap: Option<f64>,
    /// A dynamic view: its lines are steps, in the order they happen.
    pub dynamic: bool,
    /// To be drawn as a sequence rather than a diagram.
    pub sequence: bool,
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
    /// Each with its `where`, if it had one.
    excluded: Vec<(RelFilter, Option<Condition>)>,
}

/// What a `where` condition can ask about one thing.
struct Facts<'a> {
    kind: &'a str,
    tags: &'a [String],
    metadata: &'a [(String, String)],
}

/// Whether a condition holds, given whose facts are whose. Asked about
/// something that is not there (an element has no `source`), it does not.
fn holds<'a>(cond: &Condition, facts: &dyn Fn(Whose) -> Option<Facts<'a>>) -> bool {
    match cond {
        Condition::Kind { of, name, eq } => facts(*of).is_some_and(|f| (f.kind == name) == *eq),
        Condition::Tag { of, name, eq } => facts(*of).is_some_and(|f| f.tags.contains(name) == *eq),
        Condition::Metadata { of, key, value, eq } => facts(*of).is_some_and(|f| {
            let found =
                f.metadata.iter().any(|(k, v)| k == key && value.as_ref().is_none_or(|want| v == want));
            found == *eq
        }),
        Condition::Not(inner) => !holds(inner, facts),
        Condition::And(a, b) => holds(a, facts) && holds(b, facts),
        Condition::Or(a, b) => holds(a, facts) || holds(b, facts),
    }
}

impl Model {
    pub fn view_ids(&self) -> Vec<&str> {
        self.views.iter().map(|v| v.id.as_str()).collect()
    }

    /// The rules of a view: what its `views` block says of every view,
    /// then its own, each with those it inherits first.
    fn rules_of<'a>(&'a self, view: &'a View) -> Vec<&'a ViewRule> {
        let mut rules = Vec::new();
        self.collect_rules(view, true, 0, &mut rules);
        self.collect_rules(view, false, 0, &mut rules);
        rules
    }

    fn collect_rules<'a>(
        &'a self,
        view: &'a View,
        shared: bool,
        depth: usize,
        rules: &mut Vec<&'a ViewRule>,
    ) {
        if depth < 16
            && let Some((base, _)) = &view.extends
            && let Some(base) = self.view_def(base)
        {
            self.collect_rules(base, shared, depth + 1, rules);
        }
        for rule in if shared { &view.shared } else { &view.rules } {
            match rule {
                // What a `global` block named, in its place.
                ViewRule::Global { name, style, .. } => {
                    rules.extend(self.globals.get(&(*style, name.clone())).into_iter().flatten());
                }
                _ => rules.push(rule),
            }
        }
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
        let rules = self.rules_of(view);
        // A sequence reads across, unless the view says otherwise.
        let mut direction = if view.dynamic { Direction::LeftRight } else { Direction::TopBottom };
        let (mut rank_gap, mut node_gap) = (None, None);
        for rule in &rules {
            match rule {
                ViewRule::Include(predicates) => predicates.iter().for_each(|p| c.include(p)),
                ViewRule::Exclude(predicates) => predicates.iter().for_each(|p| c.exclude(p)),
                ViewRule::AutoLayout { direction: d, rank_gap: r, node_gap: n } => {
                    (direction, rank_gap, node_gap) = (*d, *r, *n);
                }
                ViewRule::Style { .. } | ViewRule::Global { .. } => {}
            }
        }

        // Whatever a step is between is there, whatever else was said.
        for step in &view.steps {
            c.visible[step.source] = true;
            c.visible[step.target] = true;
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
                description: el.summary_or_description().to_string(),
                technology: el.technology.clone(),
                notation: el.notation.clone(),
                tags: el.tags.clone(),
                shape: style.shape,
                color: String::new(),
                border: style.border,
                opacity: style.opacity,
                size: style.size,
                padding: style.padding,
                text_size: style.text_size,
                multiple: style.multiple,
                icon: None,
                icon_size: style.size,
                icon_position: style.icon_position,
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
                        "description" | "summary" => node.description = value.clone(),
                        "technology" => node.technology = value.clone(),
                        "notation" => node.notation = value.clone(),
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
            (node.size, node.padding, node.text_size) = (style.size, style.padding, style.text_size);
            node.multiple = style.multiple;
            node.icon_size = style.icon_size.unwrap_or(style.size);
            node.icon_position = style.icon_position;
            node.icon = style.icon;
            nodes.push(node);
        }
        let ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        for node in &mut nodes {
            node.group = nodes_have_child(&ids, &shown, self, &node.id);
        }

        // Edges: each relationship, lifted to what is showing.
        let mut edges: Vec<ViewEdge> = Vec::new();
        let mut by_ends: BTreeMap<(usize, usize), usize> = BTreeMap::new();
        for step in &view.steps {
            let (s, t) = (&self.elements[step.source].fqn, &self.elements[step.target].fqn);
            let relations = (0..self.relations.len())
                .filter(|&r| {
                    let rel = &self.relations[r];
                    if step.source == step.target {
                        return rel.source == step.source && rel.target == step.target;
                    }
                    self.is_ancestor_or_self(step.source, rel.source)
                        && self.is_ancestor_or_self(step.target, rel.target)
                })
                .collect();
            let RelationStyle { color, line, head, tail } = step.style.clone();
            edges.push(ViewEdge {
                id: format!("{s}->{t}#{}", step.number),
                source: s.clone(),
                target: t.clone(),
                label: if step.title.is_empty() {
                    step.number.clone()
                } else {
                    format!("{}. {}", step.number, step.title)
                },
                technology: step.technology.clone(),
                relations,
                color,
                line,
                head,
                tail,
                step: step.number.clone(),
                notes: step.notes.clone(),
                notation: step.notation.clone(),
            });
        }
        let lifted = if view.dynamic { &[][..] } else { &self.relations[..] };
        for (r, rel) in lifted.iter().enumerate() {
            let (Some(source), Some(target)) = (nearest_shown(rel.source), nearest_shown(rel.target)) else {
                continue;
            };
            if self.is_ancestor_or_self(source, target) || self.is_ancestor(target, source) {
                continue;
            }
            if c.excluded.iter().any(|(f, cond)| {
                c.filter_matches(f, rel.source, rel.target)
                    && cond.as_ref().is_none_or(|cond| c.relation_holds(cond, rel))
            }) {
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
                    step: String::new(),
                    notes: String::new(),
                    notation: String::new(),
                });
                edges.len() - 1
            });
            edges[index].relations.push(r);
        }
        for edge in edges.iter_mut().filter(|e| e.step.is_empty()) {
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
            edge.notation = lead.notation.clone();
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
            rank_gap,
            node_gap,
            dynamic: view.dynamic,
            sequence: view.dynamic && view.sequence,
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
    for (key, value, span) in &props.0 {
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
            "size" => style.size = Size::parse(value).unwrap_or(style.size),
            "padding" => style.padding = Size::parse(value).unwrap_or(style.padding),
            "textSize" => style.text_size = Size::parse(value).unwrap_or(style.text_size),
            "multiple" => style.multiple = value == "true",
            // What it is was worked out, and any trouble with it reported,
            // when the model was built.
            "icon" if value == "none" => style.icon = None,
            "icon" => {
                if let Some(href) = model.icons.get(&(span.file, value.clone())) {
                    style.icon = Some(href.clone());
                }
            }
            "iconSize" => style.icon_size = Size::parse(value).or(style.icon_size),
            "iconPosition" => style.icon_position = IconPosition::parse(value).unwrap_or(style.icon_position),
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
                    out.extend(self.neighbours(scope, true, true, None));
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
    fn neighbours(&self, x: usize, incoming: bool, outgoing: bool, cond: Option<&Condition>) -> Vec<usize> {
        let model = self.model;
        let mut out = Vec::new();
        for rel in &model.relations {
            if cond.is_some_and(|cond| !self.relation_holds(cond, rel)) {
                continue;
            }
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

    fn element_facts(&self, i: usize) -> Facts<'_> {
        let el = &self.model.elements[i];
        Facts { kind: &el.kind, tags: &el.tags, metadata: &el.metadata }
    }

    fn element_holds(&self, cond: &Condition, i: usize) -> bool {
        holds(cond, &|of| (of == Whose::Own).then(|| self.element_facts(i)))
    }

    fn relation_holds(&self, cond: &Condition, rel: &Relation) -> bool {
        holds(cond, &|of| {
            Some(match of {
                Whose::Own => Facts {
                    kind: rel.kind.as_deref().unwrap_or_default(),
                    tags: &rel.tags,
                    metadata: &rel.metadata,
                },
                Whose::Source => self.element_facts(rel.source),
                Whose::Target => self.element_facts(rel.target),
            })
        })
    }

    fn include(&mut self, predicate: &Predicate) {
        match predicate {
            Predicate::Element { expr, cond, with } => {
                for i in self.elements(expr) {
                    if cond.as_ref().is_some_and(|cond| !self.element_holds(cond, i)) {
                        continue;
                    }
                    self.visible[i] = true;
                    if !with.0.is_empty() {
                        self.overrides.entry(i).or_default().0.extend(with.0.iter().cloned());
                    }
                }
            }
            Predicate::Relation(rel, cond) => {
                let cond = cond.as_ref();
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
                                        && cond.is_none_or(|cond| self.relation_holds(cond, r))
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
                    let others = self.neighbours(x, incoming, outgoing, cond);
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
            Predicate::Element { expr, cond, .. } => {
                for i in 0..self.model.elements.len() {
                    if self.visible[i]
                        && self.matches(expr, i)
                        && cond.as_ref().is_none_or(|cond| self.element_holds(cond, i))
                    {
                        self.visible[i] = false;
                    }
                }
            }
            Predicate::Relation(rel, cond) => {
                let filter = match rel {
                    RelExpr::Incoming(e) => RelFilter::Incoming(self.filter_elements(e)),
                    RelExpr::Outgoing(e) => RelFilter::Outgoing(self.filter_elements(e)),
                    RelExpr::InOut(e) => RelFilter::InOut(self.filter_elements(e)),
                    RelExpr::Direct(a, b) => {
                        RelFilter::Direct(self.filter_elements(a), self.filter_elements(b))
                    }
                };
                self.excluded.push((filter, cond.clone()));
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
    use crate::{Direction, IconPosition, Severity, Size, Source, load};

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
    fn a_box_says_its_summary_and_keeps_the_description_for_later() {
        let source = SHOP
            .replace(
                "api = component 'API' { #next }",
                "api = component 'API' {
                    #next
                    summary 'Takes orders'
                    description 'Takes orders, checks them against the stock and hands them on.'
                }",
            )
            .replace(
                "include customer, -> shop.api ->",
                "include customer, shop.ui with { summary 'Shop front' }, -> shop.api ->",
            );
        let ws = load(&[Source { name: "shop.c4".into(), text: source }]);
        assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
        let api = &ws.model.elements[ws.model.index_of("shop.api").unwrap()];
        assert_eq!(
            (api.summary.as_str(), api.description.as_str()),
            ("Takes orders", "Takes orders, checks them against the stock and hands them on.")
        );
        let view = ws.model.compute_view("picked").unwrap();
        let said = |id: &str| view.nodes.iter().find(|n| n.id == id).unwrap().description.as_str();
        assert_eq!(said("shop.api"), "Takes orders");
        // No summary: the description. One given in the view wins.
        assert_eq!(said("shop.ui"), "Shop front");
        assert_eq!(said("shop.db"), "");
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
    fn a_dynamic_view_shows_its_steps_in_order() {
        let source = SHOP.replace(
            "view index { include * }",
            "view index { include * }
            dynamic view checkout {
                title 'Checking out'
                customer -> shop.ui 'presses Buy'
                shop.ui -> shop.api
                shop.api -> shop.db 'saves the order' 'SQL' { notes 'One transaction.' }
                parallel {
                    shop.api -[async]-> mail.smtp
                    shop.ui <- shop.api 'order number' { line solid }
                }
                customer <- shop.ui -> shop.api 'asks again'
                shop.api -> shop.api 'checks the stock'
                include shop
                variant sequence
                shop -> shop.api
                nobody -> shop
            }",
        );
        let ws = load(&[Source { name: "shop.c4".into(), text: source }]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| d.to_string()).collect();
        assert_eq!(
            messages,
            [
                "shop.c4:41:17: error: A step cannot go between an element and something inside it: name two elements side by side, or one twice for a step it takes alone.",
                "shop.c4:42:17: error: There is no element 'nobody'.",
            ]
        );
        assert!(ws.model.view_ids().contains(&"checkout"));
        let view = ws.model.compute_view("checkout").unwrap();
        assert!(view.dynamic && view.sequence);
        assert_eq!((view.title.as_str(), view.direction), ("Checking out", crate::Direction::LeftRight));
        let (nodes, edges) = summary(&view);
        // The ends of every step, and the group that was asked for; not
        // everything the model has between them.
        assert_eq!(nodes, ["customer", "shop", "shop.ui", "shop.api", "shop.db", "mail.smtp"]);
        assert_eq!(
            edges,
            [
                "customer->shop.ui#1 | 1. presses Buy",
                // Said as the model says it, where the step says nothing.
                "shop.ui->shop.api#2 | 2. calls",
                "shop.api->shop.db#3 | 3. saves the order",
                "shop.api->mail.smtp#4.1 | 4.1. sends receipts",
                "shop.api->shop.ui#4.2 | 4.2. order number",
                // An answer going back is not the relationship going forward.
                "shop.ui->customer#5 | 5",
                "shop.ui->shop.api#6 | 6. asks again",
                // A step it takes alone.
                "shop.api->shop.api#7 | 7. checks the stock",
            ]
        );
        assert_eq!(
            (view.edges[2].technology.as_str(), view.edges[2].notes.as_str()),
            ("SQL", "One transaction.")
        );
        assert_eq!(view.edges[3].line, crate::Line::Dotted);
        assert_eq!(view.edges[4].line, crate::Line::Solid);
        assert_eq!(view.edges[1].relations.len(), 1);
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

    /// What LikeC4 reads and this does not draw is said, never dropped
    /// quietly, and none of it stops the model.
    #[test]
    fn what_is_not_supported_is_a_warning() {
        let ws = load(&[Source {
            name: "m.c4".into(),
            text: "specification { element box { style { icon tech:rust } } }
deployment { node n }
model {
  a = box { icon aws:lambda }
  b = box { style { sizing fixed; icon @/b.svg } }
  c = box { icon ./c.png }
  a -> b
}
views {
  /** @likec4-generated(v1) abc */
  view v { include *
    autoLayout LeftRight 120 110 ortho }
  deployment view d { include * }
}"
            .into(),
        }]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| format!("{}:{}", d.line, d.message)).collect();
        assert!(ws.diagnostics.iter().all(|d| d.severity == Severity::Warning), "{messages:#?}");
        assert_eq!(
            messages,
            [
                "1:Bundled icons ('tech:rust') are not supported yet; use a URL, or the path of an SVG file like './rust.svg'.",
                "2:'deployment' blocks are not supported yet; this one is ignored.",
                "4:Bundled icons ('aws:lambda') are not supported yet; use a URL, or the path of an SVG file like './lambda.svg'.",
                "5:'sizing fixed' is not supported yet; a box grows to fit its words.",
                "5:Image aliases ('@/...') are not supported yet; write the path from this file, like './images/logo.svg'.",
                "6:An icon from a file that is not an SVG ('./c.png') is not supported yet; use an SVG, or a URL.",
                "10:Manual layout is not supported yet; this view is laid out automatically.",
                "12:'ortho' routing is not supported yet; lines are drawn as curves.",
                "13:deployment views are not supported yet; this one is ignored.",
            ]
        );
        assert_eq!((ws.model.elements.len(), ws.model.relations.len(), ws.model.views.len()), (3, 1, 1));
    }

    #[test]
    fn sizes_stacks_and_spacing_reach_the_view() {
        let ws = load(&[Source {
            name: "m.c4".into(),
            text: "specification {
  element box { style { size lg; multiple true } }
  element odd { style { size huge; multiple yes } }
}
model {
  a = box
  b = box { style { size xsmall; padding xl; textSize small; multiple false } }
  c = odd
}
views {
  view v { include *, c with { textSize xl }
    style a { padding sm }
    autoLayout BottomTop 90 30 }
  view w { include *
    autoLayout TopBottom 0 }
}"
            .into(),
        }]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| format!("{}:{}", d.line, d.message)).collect();
        assert_eq!(
            messages,
            [
                "3:'huge' is not a size: use xsmall, small, medium, large or xlarge (or xs, sm, md, lg, xl).",
                "3:'yes' is not true or false: write 'multiple true'.",
                "15:'0' is not a distance: use a number of pixels, like 120.",
            ]
        );
        let view = ws.model.compute_view("v").unwrap();
        assert_eq!(
            (view.direction, view.rank_gap, view.node_gap),
            (Direction::BottomTop, Some(90.0), Some(30.0))
        );
        let looks: Vec<_> = view.nodes.iter().map(|n| (n.size, n.padding, n.text_size, n.multiple)).collect();
        assert_eq!(
            looks,
            [
                (Size::Large, Size::Small, Size::Medium, true),
                (Size::XSmall, Size::XLarge, Size::Small, false),
                (Size::Medium, Size::Medium, Size::XLarge, false),
            ]
        );
    }

    #[test]
    fn an_icon_is_a_url_or_an_svg_among_the_sources() {
        let model = Source {
            name: "shop/arch/m.c4".into(),
            text: "specification {
  element box { style { icon ./icons/box.svg; iconPosition top } }
}
model {
  a = box
  b = box { icon https://example.com/b.png; style { iconSize xl; iconPosition right } }
  c = box { icon none }
  d = box { icon ../shared.svg }
  e = box { style { size lg } }
}
views {
  view v { include *, e with { icon ../shared.svg }
    style c { icon https://example.com/c.svg; iconPosition bottom } }
}"
            .into(),
        };
        let image = |name: &str, text: &str| Source { name: name.into(), text: text.into() };
        // Without the images: each is asked for, by where it would be.
        let ws = load(std::slice::from_ref(&model));
        assert_eq!(ws.wanted, ["shop/arch/icons/box.svg", "shop/shared.svg"]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| format!("{}:{}", d.line, d.message)).collect();
        assert_eq!(
            messages,
            [
                "2:There is no file 'shop/arch/icons/box.svg' for this icon: check the path, or use a URL.",
                "8:There is no file 'shop/shared.svg' for this icon: check the path, or use a URL.",
                "12:There is no file 'shop/shared.svg' for this icon: check the path, or use a URL.",
            ]
        );
        // With them, they are just more sources, and not read as a model.
        let ws = load(&[
            model,
            image("shop/arch/icons/box.svg", "<svg/>"),
            image("shop/shared.svg", "<svg>ab</svg>"),
        ]);
        assert!(ws.diagnostics.is_empty() && ws.wanted.is_empty(), "{:?}", ws.diagnostics);
        let view = ws.model.compute_view("v").unwrap();
        let icons: Vec<_> = view
            .nodes
            .iter()
            .map(|n| (n.icon.as_deref().unwrap_or("-"), n.icon_size, n.icon_position))
            .collect();
        let (boxed, shared) =
            ("data:image/svg+xml;base64,PHN2Zy8+", "data:image/svg+xml;base64,PHN2Zz5hYjwvc3ZnPg==");
        assert_eq!(
            icons,
            [
                (boxed, Size::Medium, IconPosition::Top),
                ("https://example.com/b.png", Size::XLarge, IconPosition::Right),
                // None of its own, then one from the view's style.
                ("https://example.com/c.svg", Size::Medium, IconPosition::Bottom),
                (shared, Size::Medium, IconPosition::Top),
                // As big as its box, unless it says.
                (shared, Size::Large, IconPosition::Top),
            ]
        );
    }

    #[test]
    fn notation_is_the_kinds_unless_something_says_otherwise() {
        let ws = load(&[Source {
            name: "m.c4".into(),
            text: "specification {
  element box { notation 'A box' }
  element plain
  relationship async { notation 'Does not wait' }
}
model {
  a = box
  b = box { notation 'A special box' }
  c = plain
  a -[async]-> b
  a -> c { notation 'By hand' }
  b -> c
}
views {
  view v { include *, c with { notation 'In this view' } }
  dynamic view d { a -> b; a -[async]-> c; b -> c }
}"
            .into(),
        }]);
        assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
        let view = ws.model.compute_view("v").unwrap();
        let nodes: Vec<_> = view.nodes.iter().map(|n| n.notation.as_str()).collect();
        assert_eq!(nodes, ["A box", "A special box", "In this view"]);
        let edges: Vec<_> = view.edges.iter().map(|e| e.notation.as_str()).collect();
        assert_eq!(edges, ["Does not wait", "By hand", ""]);
        // A step: its relationship's, or its own kind's.
        let steps = ws.model.compute_view("d").unwrap();
        let edges: Vec<_> = steps.edges.iter().map(|e| e.notation.as_str()).collect();
        assert_eq!(edges, ["Does not wait", "Does not wait", ""]);
    }

    /// The model the `where` and `global` tests ask about.
    const FLEET: &str = "specification {
            element system
            element service
            element store
            relationship async
            tag old
            tag paid
        }
        model {
            web = service 'Web' { metadata { tier 'front' } }
            api = service 'API' { #paid; metadata { tier 'back'; owner 'core' } }
            jobs = service 'Jobs' { #old }
            db = store 'Database' { #paid }
            legacy = system 'Legacy' { #old }
            web -> api 'calls'
            api -> db 'reads'
            api -[async]-> jobs 'queues' { #old; metadata { via 'kafka' } }
            jobs -> legacy 'feeds'
        }";

    fn fleet(views: &str) -> crate::Workspace {
        let ws = load(&[Source { name: "fleet.c4".into(), text: format!("{FLEET}\n{views}") }]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| d.to_string()).collect();
        assert!(messages.is_empty(), "{messages:#?}");
        ws
    }

    fn shows(ws: &crate::Workspace, view: &str) -> String {
        let view = ws.model.compute_view(view).unwrap();
        view.nodes.iter().map(|n| n.id.as_str()).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn where_keeps_the_elements_a_condition_holds_for() {
        let ws = fleet(
            "views {
                view kind { include * where kind is service }
                view not_kind { include * where kind is not service }
                view tag { include * where tag is #paid }
                view both { include * where kind is service and tag is not #old }
                view either { include * where kind == store or tag == #old }
                view grouped { include * where not (kind is service or kind is store) }
                view order { include * where kind is store or kind is service and tag is #old }
                view meta { include * where metadata.tier is 'back' }
                view has_meta { include * where metadata.tier }
                view no_meta { include * where not metadata.tier }
                view meta_ne { include * where metadata.tier != 'back' }
                view out {
                    include *
                    exclude * where tag is #old
                }
                view with_too { include * where kind is store with { color red } }
            }",
        );
        assert_eq!(shows(&ws, "kind"), "web api jobs");
        assert_eq!(shows(&ws, "not_kind"), "db legacy");
        assert_eq!(shows(&ws, "tag"), "api db");
        assert_eq!(shows(&ws, "both"), "web api");
        assert_eq!(shows(&ws, "either"), "jobs db legacy");
        assert_eq!(shows(&ws, "grouped"), "legacy");
        // `and` binds tighter than `or`.
        assert_eq!(shows(&ws, "order"), "jobs db");
        assert_eq!(shows(&ws, "meta"), "api");
        assert_eq!(shows(&ws, "has_meta"), "web api");
        assert_eq!(shows(&ws, "no_meta"), "jobs db legacy");
        // Not that value: whether it has another, or none.
        assert_eq!(shows(&ws, "meta_ne"), "web jobs db legacy");
        assert_eq!(shows(&ws, "out"), "web api db");
        let view = ws.model.compute_view("with_too").unwrap();
        assert_eq!((view.nodes.len(), view.nodes[0].color.as_str()), (1, "red"));
    }

    #[test]
    fn where_after_a_relationship_asks_about_the_relationship() {
        let ws = fleet(
            "views {
                view kind { include api -> where kind is async }
                view tag { include -> api -> where tag is not #old }
                view ends { include * -> * where source.kind is service and target.tag is #old }
                view meta { include api -> * where metadata.via is 'kafka' }
                view direct { include web -> api where kind is async }
                view cut {
                    include *
                    exclude api -> where kind is async
                    exclude -> legacy where source.tag is #paid
                }
            }",
        );
        assert_eq!(shows(&ws, "kind"), "api jobs");
        assert_eq!(shows(&ws, "tag"), "web api db");
        assert_eq!(shows(&ws, "ends"), "api jobs legacy");
        assert_eq!(shows(&ws, "meta"), "api jobs");
        // Nothing between those two is async.
        assert_eq!(shows(&ws, "direct"), "");
        let view = ws.model.compute_view("cut").unwrap();
        let lines: Vec<&str> = view.edges.iter().map(|e| e.id.as_str()).collect();
        // The second exclude holds for nothing: jobs is not #paid.
        assert_eq!(lines, ["web->api", "api->db", "jobs->legacy"]);
    }

    #[test]
    fn a_condition_that_cannot_be_read_says_how_to_write_it() {
        let ws = load(&[Source {
            name: "m.c4".into(),
            text: format!(
                "{FLEET}
views {{
  view a {{ include * where source.kind is service }}
  view b {{ include * where name is api }}
  view c {{ include * where kind service }}
  view d {{ include * where (kind is service }}
}}"
            ),
        }]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| d.message.as_str()).collect();
        assert_eq!(
            messages,
            [
                "'source.' and 'target.' ask about a relationship's ends: use them after a relationship, like '-> a where source.kind is x'.",
                "Expected 'kind', 'tag' or 'metadata' in a 'where' condition, found 'name'.",
                "Expected 'is' or 'is not', found 'service'.",
                "Expected ')', found '}'.",
            ]
        );
    }

    #[test]
    fn global_rules_are_asked_for_by_name() {
        let ws = fleet(
            "global {
                predicateGroup current {
                    include * where kind is service
                    exclude * where tag is #old
                }
                style quiet * { color muted }
                styleGroup marked {
                    style element.tag = #paid { color green }
                    style db { shape storage }
                }
            }
            views {
                view one {
                    global predicate current
                    include db
                    global style quiet
                    global style marked
                }
                view two {
                    include *
                    global style marked
                    global style quiet
                }
                view three extends one { exclude web }
            }
            views {
                global style quiet
                view shared { include api, jobs; style jobs { color red } }
            }",
        );
        let looks = |id: &str| {
            let view = ws.model.compute_view(id).unwrap();
            view.nodes.iter().map(|n| format!("{}:{}", n.id, n.color)).collect::<Vec<_>>().join(" ")
        };
        // In the order asked for: the later rule has the last word.
        assert_eq!(looks("one"), "web:muted api:green db:green");
        assert_eq!(looks("two"), "web:muted api:muted jobs:muted db:muted legacy:muted");
        assert_eq!(looks("three"), "api:green db:green");
        // Asked for by a whole `views` block: before each view's own rules.
        assert_eq!(looks("shared"), "api:muted jobs:red");
        let one = ws.model.compute_view("one").unwrap();
        assert_eq!(one.nodes.iter().find(|n| n.id == "db").unwrap().shape, crate::Shape::Storage);
    }

    #[test]
    fn a_global_that_is_missing_or_given_twice_is_explained() {
        let ws = load(&[
            Source {
                name: "a.c4".into(),
                text: format!(
                    "{FLEET}
global {{
  style quiet * {{ color muted }}
  predicateGroup all {{ include *, nothing }}
  theme x {{ }}
}}
views {{
  view v {{
    global style loud
    global predicate quiet
    global colour quiet
    include *
  }}
}}"
                ),
            },
            Source {
                name: "b.c4".into(),
                text: "global { styleGroup quiet { style * { color red } } }".into(),
            },
        ]);
        let messages: Vec<_> =
            ws.diagnostics.iter().map(|d| format!("{}:{} {}", d.file, d.line, d.message)).collect();
        assert_eq!(
            messages,
            [
                "a.c4:22 There is no element 'nothing'.",
                "a.c4:23 Expected 'style', 'styleGroup' or 'predicateGroup', found 'theme'.",
                "a.c4:27 There is no global style 'loud'. The ones defined are: quiet.",
                "a.c4:28 There is no global predicate group 'quiet'. The ones defined are: all.",
                "a.c4:29 Expected 'style' or 'predicate' after 'global', found 'colour'.",
                "b.c4:1 There is already a global style 'quiet'.",
            ]
        );
        // What could be understood is still there.
        assert_eq!(shows(&ws, "v"), "web api jobs db legacy");
    }

    #[test]
    fn an_import_names_what_is_already_in_the_model() {
        let ws = load(&[
            Source { name: "fleet.c4".into(), text: FLEET.into() },
            Source {
                name: "mine.c4".into(),
                text: "import { api, ghost } from 'fleet'
model { mine = service 'Mine' { -> api 'calls' } }"
                    .into(),
            },
        ]);
        let messages: Vec<_> = ws.diagnostics.iter().map(|d| format!("{}:{}", d.line, d.message)).collect();
        assert_eq!(
            messages,
            ["1:There is no element 'ghost' to import: read the files that define it along with these."]
        );
        assert_eq!(ws.model.relations.len(), 5);
    }
}
