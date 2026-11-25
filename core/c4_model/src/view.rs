//! Computing a view: which elements and relationships a view shows.
//!
//! The model is a tree of elements with relationships between any two. A
//! view shows some of the elements; an element drawn with some of its
//! descendants also showing becomes a group around them. A relationship is
//! drawn between the nearest showing ancestors of its two ends, so
//! `customer -> cloud.ui` appears in a landscape view as `customer -> cloud`.

use std::collections::BTreeMap;

use crate::ast::{Condition, Direction, ElementExpr, Predicate, Rank, RelExpr, StyleProps, ViewRule, Whose};
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
    /// What the view's `rank` blocks ask, in the order written.
    pub ranks: Vec<ViewRank>,
    /// A dynamic view: its lines are steps, in the order they happen.
    pub dynamic: bool,
    /// To be drawn as a sequence rather than a diagram.
    pub sequence: bool,
    /// Parents before their children.
    pub nodes: Vec<ViewNode>,
    pub edges: Vec<ViewEdge>,
}

/// `rank same { a, b }`, with the names resolved to nodes of the view.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewRank {
    pub rank: Rank,
    /// The ids of the nodes it names that the view shows, each once.
    pub nodes: Vec<String>,
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
    /// From `include a -> b with { }`: which relationships, and what is
    /// said of the lines that draw them, in the order written.
    restyled: Vec<(RelFilter, Option<Condition>, StyleProps)>,
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
            restyled: Vec::new(),
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
                ViewRule::Style { .. } | ViewRule::Global { .. } | ViewRule::Rank { .. } => {}
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
            // What a style says it is, over what its kind does; what
            // `with` says, below, over both.
            if let Some(notation) = &style.notation {
                node.notation = notation.clone();
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
            if c.excluded.iter().any(|(f, cond)| c.filter_takes(f, cond.as_ref(), rel)) {
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
            // Then what the view says of this line: `include a -> b with { }`.
            for (filter, cond, props) in &c.restyled {
                if !rels.iter().any(|rel| c.filter_takes(filter, cond.as_ref(), rel)) {
                    continue;
                }
                for (key, value, _) in &props.0 {
                    match key.as_str() {
                        "title" => edge.label = value.trim().to_string(),
                        "technology" => edge.technology = value.trim().to_string(),
                        "notation" => edge.notation = value.trim().to_string(),
                        "color"
                            if crate::model::THEME_COLORS.contains(&value.as_str())
                                || self.colors.contains_key(value) =>
                        {
                            edge.color = value.clone();
                        }
                        // A relationship always has a line.
                        "line" => {
                            edge.line = Line::parse(value).filter(|l| *l != Line::None).unwrap_or(edge.line)
                        }
                        "head" => edge.head = Arrow::parse(value).unwrap_or(edge.head),
                        "tail" => edge.tail = Arrow::parse(value).unwrap_or(edge.tail),
                        _ => {}
                    }
                }
            }
        }

        // What `rank` names, as far as it is still in the view.
        let mut ranks: Vec<ViewRank> = Vec::new();
        for rule in &rules {
            let ViewRule::Rank { rank, targets } = rule else { continue };
            let mut named: Vec<String> = Vec::new();
            for target in targets {
                let Some(i) = c.find(target).filter(|&i| c.visible[i]) else { continue };
                let id = &self.elements[i].fqn;
                if !named.contains(id) {
                    named.push(id.clone());
                }
            }
            if !named.is_empty() {
                ranks.push(ViewRank { rank: *rank, nodes: named });
            }
        }

        Some(ComputedView {
            id: view.id.clone(),
            title: view.title.clone(),
            description: view.description.clone(),
            direction,
            rank_gap,
            node_gap,
            ranks,
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
            "border" => style.border = Line::parse(value).unwrap_or(style.border),
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
            "notation" => style.notation = Some(value.trim().to_string()),
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
            // One that goes both ways comes in and goes out at each end.
            let other = match (source_in, target_in) {
                (true, false) if outgoing || rel.both => rel.target,
                (false, true) if incoming || rel.both => rel.source,
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
            Predicate::Relation(rel, cond, with) => {
                if !with.0.is_empty() {
                    let filter = self.rel_filter(rel);
                    self.restyled.push((filter, cond.clone(), with.clone()));
                }
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
                                let model = self.model;
                                let joins = |a: usize, b: usize, r: &Relation| {
                                    model.is_ancestor_or_self(a, r.source)
                                        && model.is_ancestor_or_self(b, r.target)
                                };
                                let related = model.relations.iter().any(|r| {
                                    (joins(s, t, r) || (r.both && joins(t, s, r)))
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
            Predicate::Relation(rel, cond, _) => {
                let filter = self.rel_filter(rel);
                self.excluded.push((filter, cond.clone()));
            }
        }
    }

    /// A relationship predicate with its elements found.
    fn rel_filter(&self, rel: &RelExpr) -> RelFilter {
        match rel {
            RelExpr::Incoming(e) => RelFilter::Incoming(self.filter_elements(e)),
            RelExpr::Outgoing(e) => RelFilter::Outgoing(self.filter_elements(e)),
            RelExpr::InOut(e) => RelFilter::InOut(self.filter_elements(e)),
            RelExpr::Direct(a, b) => RelFilter::Direct(self.filter_elements(a), self.filter_elements(b)),
        }
    }

    /// Whether a predicate, with its `where`, is about this relationship:
    /// one that goes both ways is taken from either end.
    fn filter_takes(&self, filter: &RelFilter, cond: Option<&Condition>, rel: &Relation) -> bool {
        (self.filter_matches(filter, rel.source, rel.target)
            || (rel.both && self.filter_matches(filter, rel.target, rel.source)))
            && cond.is_none_or(|cond| self.relation_holds(cond, rel))
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
#[path = "../tests/view.rs"]
mod tests;
