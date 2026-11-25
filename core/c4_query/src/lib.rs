//! Questions about a model, answered in plain text.
//!
//! The answers are for reading by someone (or something) that will ask
//! again: every element is given by its full name, which every question
//! takes, so one answer leads to the next. They are short on purpose. A
//! model of a whole company is thousands of lines; an answer should be the
//! twenty that matter.
//!
//! Dependencies are followed at the level of the element asked about. Ask
//! about a service and its components' relationships count as its own, and
//! what it reaches is named as services; ask about one component and only
//! that component's own relationships are followed.

use std::collections::BTreeMap;

use c4_model::{Model, Relation};

/// Which way to follow relationships from an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// What it uses.
    Out,
    /// What uses it.
    In,
    Both,
}

/// No answer runs longer than this many list lines; the rest are counted.
const MAX_LINES: usize = 60;

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// `shop.api (API, service)`
fn name(model: &Model, i: usize) -> String {
    let el = &model.elements[i];
    if el.title == el.name {
        format!("{} ({})", el.fqn, el.kind)
    } else {
        format!("{} ({}, {})", el.fqn, el.title, el.kind)
    }
}

/// The first line of a description, cut to a sensible length.
fn brief(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() > 120 {
        format!("{}…", line.chars().take(117).collect::<String>())
    } else {
        line.to_string()
    }
}

fn depth(model: &Model, mut i: usize) -> usize {
    let mut d = 0;
    while let Some(p) = model.elements[i].parent {
        d += 1;
        i = p;
    }
    d
}

/// The element around `i` that is `level` deep, or `i` itself if it is no
/// deeper than that.
fn unit(model: &Model, mut i: usize, level: usize) -> usize {
    let mut d = depth(model, i);
    while d > level {
        i = model.elements[i].parent.expect("deeper than zero has a parent");
        d -= 1;
    }
    i
}

/// What a relationship says, beyond its two ends.
fn says(rel: &Relation) -> String {
    match (rel.title.is_empty(), rel.technology.is_empty()) {
        (true, true) => String::new(),
        (false, true) => rel.title.clone(),
        (true, false) => format!("[{}]", rel.technology),
        (false, false) => format!("{} [{}]", rel.title, rel.technology),
    }
}

/// Lines, cut at [`MAX_LINES`] with a count of what was left out.
fn listed(lines: Vec<String>) -> String {
    let more = lines.len().saturating_sub(MAX_LINES);
    let mut out = lines.into_iter().take(MAX_LINES).collect::<Vec<_>>().join("\n");
    if more > 0 {
        out.push_str(&format!("\n… and {more} more. Ask about something narrower to see them."));
    }
    out
}

/// The element a question names, or what to say when there is none: the
/// nearest things there are.
fn resolve(model: &Model, asked: &str) -> Result<usize, String> {
    let asked = asked.trim();
    if let Some(i) = model.find(asked) {
        return Ok(i);
    }
    let last = asked.rsplit('.').next().unwrap_or(asked);
    let near: Vec<String> =
        hits(model, last, None, None).into_iter().take(5).map(|(i, _)| name(model, i)).collect();
    Err(if near.is_empty() {
        format!("There is no element '{asked}'. Use search to find what there is.")
    } else {
        format!("There is no element '{asked}'. Did you mean:\n- {}", near.join("\n- "))
    })
}

// --------------------------------------------------------------- overview --

/// What the model is, in a screenful: the place to start.
pub fn overview(model: &Model) -> String {
    let mut out = format!(
        "{}, {}, {}.",
        plural(model.elements.len(), "element", "elements"),
        plural(model.relations.len(), "relationship", "relationships"),
        plural(model.views.len(), "view", "views"),
    );
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for el in &model.elements {
        *kinds.entry(el.kind.as_str()).or_default() += 1;
    }
    if !kinds.is_empty() {
        let mut kinds: Vec<_> = kinds.into_iter().collect();
        kinds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let list: Vec<String> = kinds.iter().map(|(kind, n)| format!("{kind} {n}")).collect();
        out.push_str(&format!("\nKinds: {}.", list.join(", ")));
    }
    let top: Vec<String> = (0..model.elements.len())
        .filter(|&i| model.elements[i].parent.is_none())
        .map(|i| {
            let el = &model.elements[i];
            let inside = (0..model.elements.len()).filter(|&j| model.is_ancestor(i, j)).count();
            let mut line = format!("- {}", name(model, i));
            if inside > 0 {
                line.push_str(&format!(", {inside} inside"));
            }
            if !el.summary_or_description().is_empty() {
                line.push_str(&format!(": {}", brief(el.summary_or_description())));
            }
            line
        })
        .collect();
    if !top.is_empty() {
        out.push_str(&format!("\n\nTop level:\n{}", listed(top)));
    }
    out
}

// ----------------------------------------------------------------- search --

/// Elements matching the words of `query`, best first, with how well.
///
/// A word matches a name outright, or appears in a title, a tag, a
/// technology, a kind, a description or a metadata value. An element
/// matching more of the words comes before one matching fewer.
fn hits(model: &Model, query: &str, kind: Option<&str>, tag: Option<&str>) -> Vec<(usize, u32)> {
    let words: Vec<String> = query
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect();
    let tag = tag.map(|t| t.trim_start_matches('#'));
    let mut found: Vec<(usize, u32)> = Vec::new();
    for (i, el) in model.elements.iter().enumerate() {
        if kind.is_some_and(|k| !el.kind.eq_ignore_ascii_case(k))
            || tag.is_some_and(|t| !el.tags.iter().any(|x| x == t))
        {
            continue;
        }
        if words.is_empty() {
            // Only a filter: everything that passes it.
            found.push((i, 0));
            continue;
        }
        let (fqn, title) = (el.fqn.to_lowercase(), el.title.to_lowercase());
        let name = el.name.to_lowercase();
        let rest = format!(
            "{} {} {} {}",
            el.technology,
            el.kind,
            el.tags.join(" "),
            el.metadata.iter().map(|(_, v)| v.as_str()).collect::<Vec<_>>().join(" ")
        )
        .to_lowercase();
        let description = format!("{} {}", el.summary, el.description).to_lowercase();
        let (mut matched, mut score) = (0u32, 0u32);
        for word in &words {
            let points = if name == *word || title == *word {
                12
            } else if name.contains(word.as_str()) || title.contains(word.as_str()) {
                8
            } else if fqn.contains(word.as_str()) {
                5
            } else if rest.contains(word.as_str()) {
                4
            } else if description.contains(word.as_str()) {
                2
            } else {
                0
            };
            if points > 0 {
                matched += 1;
                score += points;
            }
        }
        if matched > 0 {
            found.push((i, matched * 100 + score));
        }
    }
    found.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    found
}

/// Finds elements, and relationships, by what is written about them.
pub fn search(model: &Model, query: &str, kind: Option<&str>, tag: Option<&str>, limit: usize) -> String {
    let limit = limit.clamp(1, MAX_LINES);
    let found = hits(model, query, kind, tag);
    let mut out = String::new();
    if !found.is_empty() {
        let lines: Vec<String> = found
            .iter()
            .take(limit)
            .map(|&(i, _)| {
                let el = &model.elements[i];
                let mut line = format!("- {}", name(model, i));
                if !el.technology.is_empty() {
                    line.push_str(&format!(" [{}]", el.technology));
                }
                if !el.summary_or_description().is_empty() {
                    line.push_str(&format!(": {}", brief(el.summary_or_description())));
                }
                line
            })
            .collect();
        out = format!("{}:\n{}", plural(found.len(), "element", "elements"), lines.join("\n"));
        if found.len() > limit {
            out.push_str(&format!("\n… and {} more.", found.len() - limit));
        }
    }
    // What is said on the lines between things, too: "refund" may be
    // nobody's name and still be what one service asks of another.
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if !words.is_empty() && kind.is_none() && tag.is_none() {
        let lines: Vec<String> = model
            .relations
            .iter()
            .filter(|rel| {
                let text = format!("{} {} {}", rel.title, rel.technology, rel.description).to_lowercase();
                words.iter().all(|w| text.contains(w.as_str()))
            })
            .map(|rel| {
                format!(
                    "- {} -> {}: {}",
                    model.elements[rel.source].fqn,
                    model.elements[rel.target].fqn,
                    says(rel)
                )
            })
            .collect();
        if !lines.is_empty() {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            let count = plural(lines.len(), "relationship", "relationships");
            let shown: Vec<String> = lines.into_iter().take(limit).collect();
            out.push_str(&format!("{count}:\n{}", shown.join("\n")));
        }
    }
    if out.is_empty() {
        return format!(
            "Nothing matches '{query}'. Try fewer or different words; overview lists what is at the top."
        );
    }
    out
}

// ---------------------------------------------------------------- element --

/// Everything about one element: what it is, where it was written, what is
/// in it, what it uses and what uses it.
pub fn element(model: &Model, asked: &str) -> Result<String, String> {
    let i = resolve(model, asked)?;
    let el = &model.elements[i];
    let mut out = name(model, i);
    let mut line = |label: &str, value: String| {
        if !value.is_empty() {
            out.push_str(&format!("\n{label}: {value}"));
        }
    };
    line("Technology", el.technology.clone());
    line("Summary", el.summary.replace('\n', " "));
    line("Description", el.description.replace('\n', " "));
    line("Tags", el.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(" "));
    line("Written in", el.origin.to_string());
    line("Inside", el.parent.map(|p| name(model, p)).unwrap_or_default());
    line(
        "Links",
        el.links
            .iter()
            .map(|(url, title)| if title.is_empty() { url.clone() } else { format!("{title} <{url}>") })
            .collect::<Vec<_>>()
            .join(", "),
    );
    line("Metadata", el.metadata.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", "));
    let views: Vec<String> = model
        .views
        .iter()
        .filter(|v| model.compute_view(&v.id).is_some_and(|c| c.nodes.iter().any(|n| n.id == el.fqn)))
        .map(|v| v.id.clone())
        .collect();
    line("In views", views.join(", "));

    if !el.children.is_empty() {
        let lines: Vec<String> = el.children.iter().map(|&c| format!("- {}", name(model, c))).collect();
        out.push_str(&format!("\n\nContains:\n{}", listed(lines)));
    }
    for (title, outgoing) in [("Uses", true), ("Used by", false)] {
        let lines: Vec<String> = model
            .relations
            .iter()
            .filter_map(|rel| {
                let (near, far) = if outgoing { (rel.source, rel.target) } else { (rel.target, rel.source) };
                if !model.is_ancestor_or_self(i, near) || model.is_ancestor_or_self(i, far) {
                    return None;
                }
                let mut line = format!("- {}", name(model, far));
                let said = says(rel);
                if !said.is_empty() {
                    line.push_str(&format!(": {said}"));
                }
                // Which part of this element it is about, when it is a part.
                if near != i {
                    line.push_str(&format!(" (through {})", model.elements[near].fqn));
                }
                Some(line)
            })
            .collect();
        if !lines.is_empty() {
            out.push_str(&format!("\n\n{title}:\n{}", listed(lines)));
        }
    }
    Ok(out)
}

// -------------------------------------------------------------- following --

/// The elements one step from `from`, at `level`: each with the
/// relationship that leads there.
fn steps(model: &Model, from: usize, level: usize, outgoing: bool) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (r, rel) in model.relations.iter().enumerate() {
        let (near, far) = if outgoing { (rel.source, rel.target) } else { (rel.target, rel.source) };
        if !model.is_ancestor_or_self(from, near) {
            continue;
        }
        let next = unit(model, far, level);
        // Not to itself, nor to what it is part of.
        if model.is_ancestor_or_self(from, next) || model.is_ancestor_or_self(next, from) {
            continue;
        }
        if !out.iter().any(|&(seen, _)| seen == next) {
            out.push((next, r));
        }
    }
    out
}

/// What an element depends on, or what depends on it, out to `depth`
/// steps: the answer to "what breaks if this changes".
pub fn neighbors(
    model: &Model,
    asked: &str,
    direction: Direction,
    depth_limit: usize,
) -> Result<String, String> {
    let start = resolve(model, asked)?;
    let level = depth(model, start);
    let depth_limit = depth_limit.clamp(1, 10);
    let mut out = name(model, start);
    let ways: &[(bool, &str, &str)] = match direction {
        Direction::Out => &[(true, "uses", "Nothing: it uses nothing outside itself.")],
        Direction::In => &[(false, "is used by", "Nothing uses it.")],
        Direction::Both => &[
            (true, "uses", "Nothing: it uses nothing outside itself."),
            (false, "is used by", "Nothing uses it."),
        ],
    };
    for &(outgoing, verb, nothing) in ways {
        // Breadth first: everything one step away, then two, and so on.
        let mut seen = vec![start];
        let mut frontier = vec![start];
        let mut total = 0;
        out.push_str(&format!("\n\n{} {verb}:", model.elements[start].fqn));
        for step in 1..=depth_limit {
            let mut next: Vec<usize> = Vec::new();
            let mut lines: Vec<String> = Vec::new();
            for &at in &frontier {
                for (to, r) in steps(model, at, level, outgoing) {
                    if seen.contains(&to) {
                        continue;
                    }
                    seen.push(to);
                    next.push(to);
                    let mut line = format!("- {}", name(model, to));
                    let said = says(&model.relations[r]);
                    if !said.is_empty() {
                        line.push_str(&format!(": {said}"));
                    }
                    if step > 1 {
                        line.push_str(&format!(" (through {})", model.elements[at].fqn));
                    }
                    lines.push(line);
                }
            }
            if lines.is_empty() {
                break;
            }
            total += lines.len();
            let heading = if step == 1 { "Directly".to_string() } else { format!("{step} steps away") };
            out.push_str(&format!("\n{heading} ({}):\n{}", lines.len(), listed(lines)));
            frontier = next;
        }
        if total == 0 {
            out.push_str(&format!("\n{nothing}"));
        }
    }
    Ok(out)
}

/// How one element reaches another: the shortest chain of relationships
/// from `from` to `to`, or failing that the other way round.
pub fn path(model: &Model, from: &str, to: &str) -> Result<String, String> {
    let (a, b) = (resolve(model, from)?, resolve(model, to)?);
    if model.is_ancestor_or_self(a, b) || model.is_ancestor_or_self(b, a) {
        return Ok(format!(
            "{} and {} are the same thing, or one is part of the other.",
            name(model, a),
            name(model, b)
        ));
    }
    let search = |start: usize, goal: usize| -> Option<Vec<(usize, usize)>> {
        let level = depth(model, start);
        // For each element reached: where from, and by which relationship.
        let mut came: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(at) = queue.pop_front() {
            if model.is_ancestor_or_self(goal, at) || model.is_ancestor_or_self(at, goal) {
                let mut hops = Vec::new();
                let mut back = at;
                while back != start {
                    let (previous, r) = came[&back];
                    hops.push((back, r));
                    back = previous;
                }
                hops.reverse();
                return Some(hops);
            }
            for (next, r) in steps(model, at, level, true) {
                if next != start && !came.contains_key(&next) {
                    came.insert(next, (at, r));
                    queue.push_back(next);
                }
            }
        }
        None
    };
    let describe = |start: usize, hops: &[(usize, usize)]| -> String {
        let mut lines = vec![name(model, start)];
        for &(to, r) in hops {
            let rel = &model.relations[r];
            let said = says(rel);
            let how = if said.is_empty() { String::new() } else { format!(" {said}") };
            lines.push(format!(
                "  ->{how}  ({} -> {})\n{}",
                model.elements[rel.source].fqn,
                model.elements[rel.target].fqn,
                name(model, to)
            ));
        }
        lines.join("\n")
    };
    if let Some(hops) = search(a, b) {
        return Ok(format!("{}:\n{}", plural(hops.len(), "step", "steps"), describe(a, &hops)));
    }
    if let Some(hops) = search(b, a) {
        return Ok(format!(
            "Nothing leads from {} to {}, but the other way round there is a path of {}:\n{}",
            model.elements[a].fqn,
            model.elements[b].fqn,
            plural(hops.len(), "step", "steps"),
            describe(b, &hops)
        ));
    }
    Ok(format!(
        "No chain of relationships connects {} and {} in either direction.",
        name(model, a),
        name(model, b)
    ))
}

// ------------------------------------------------------------------ views --

/// The views there are to ask for.
pub fn views(model: &Model) -> String {
    if model.views.is_empty() {
        return "There are no views.".into();
    }
    let lines: Vec<String> = model
        .views
        .iter()
        .map(|v| {
            let mut line = format!("- {}: {}", v.id, v.title);
            if v.dynamic {
                line.push_str(" (dynamic: steps in order)");
            }
            if let Some(of) = v.of {
                line.push_str(&format!(" (of {})", model.elements[of].fqn));
            }
            if !v.description.is_empty() {
                line.push_str(&format!(". {}", brief(&v.description)));
            }
            line
        })
        .collect();
    let mut out = listed(lines);
    if !model.generated_views.is_empty() {
        out.push_str(&format!(
            "\nAnd one of each element with something inside it ({} more), by the element's name with '_' for '.'.",
            model.generated_views.len()
        ));
    }
    out
}

/// One view as text: what a diagram of it shows, without the drawing. The
/// boxes, indented by what is inside what, then the lines between them.
pub fn view(model: &Model, id: &str) -> Result<String, String> {
    let Some(view) = model.compute_view(id.trim()) else {
        return Err(format!("There is no view '{id}'. The views are:\n{}", views(model)));
    };
    let mut out = format!("{}: {}", view.id, view.title);
    if !view.description.is_empty() {
        out.push_str(&format!("\n{}", view.description.replace('\n', " ")));
    }
    let mut lines = Vec::new();
    for node in &view.nodes {
        let mut indent = 0;
        let mut parent = node.parent.as_deref();
        while let Some(p) = parent {
            indent += 1;
            parent = view.nodes.iter().find(|n| n.id == p).and_then(|n| n.parent.as_deref());
        }
        let mut line = format!("{}- {} ({}, {})", "  ".repeat(indent), node.id, node.title, node.kind);
        if !node.technology.is_empty() {
            line.push_str(&format!(" [{}]", node.technology));
        }
        if let Some(closer) = &node.navigate_to {
            line.push_str(&format!(" (closer: view {closer})"));
        }
        lines.push(line);
    }
    out.push_str(&format!("\n\n{}:\n{}", plural(view.nodes.len(), "element", "elements"), listed(lines)));
    let lines: Vec<String> = view
        .edges
        .iter()
        .map(|edge| {
            if !edge.step.is_empty() {
                // A step says for itself what happens.
                let said = edge.label.split_once(". ").map(|(_, title)| title).unwrap_or_default();
                let mut line = format!("- {}. {} -> {}", edge.step, edge.source, edge.target);
                if !said.is_empty() {
                    line.push_str(&format!(": {said}"));
                }
                if !edge.technology.is_empty() {
                    line.push_str(&format!(" [{}]", edge.technology));
                }
                if !edge.notes.is_empty() {
                    line.push_str(&format!(" ({})", edge.notes.replace('\n', " ")));
                }
                return line;
            }
            let mut line = format!("- {} -> {}", edge.source, edge.target);
            // A line standing for several relationships: say each.
            let said: Vec<String> =
                edge.relations.iter().map(|&r| says(&model.relations[r])).filter(|s| !s.is_empty()).collect();
            let mut unique = said.clone();
            unique.dedup();
            if !unique.is_empty() {
                line.push_str(&format!(": {}", unique.join("; ")));
            }
            line
        })
        .collect();
    if !lines.is_empty() {
        out.push_str(&format!(
            "\n\n{}:\n{}",
            if view.dynamic {
                plural(lines.len(), "step, in order", "steps, in order")
            } else {
                plural(lines.len(), "relationship", "relationships")
            },
            listed(lines)
        ));
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../tests/lib.rs"]
mod tests;
