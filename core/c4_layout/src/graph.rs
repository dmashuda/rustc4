//! The layout itself: boxes, groups of boxes and the lines between them go
//! in; a position for each comes out.
//!
//! It is a layered ("Sugiyama") layout, with groups:
//!
//! 1. **Rank.** Every box gets a row, so that lines point down. Cycles are
//!    broken first by turning some lines around.
//! 2. **Fill in.** A line that crosses several rows gets a placeholder in
//!    each one it passes, so a row knows what runs through it. One
//!    placeholder carries the line's label.
//! 3. **Order.** Each row is put in the order that crosses the fewest
//!    lines, keeping a group's members side by side and two groups the same
//!    way round in every row they share.
//! 4. **Place.** Left-to-right positions are found by pulling everything
//!    toward what it is connected to, subject to nothing overlapping and
//!    nothing sitting inside a group it does not belong to.
//! 5. **Route.** Each line runs through its placeholders, then as straight
//!    as it can between its ends without coming near anything else.
//!
//! Everything is worked out as if lines ran top to bottom; other directions
//! turn the result at the end.

use c4_model::{Direction, Rank};

/// Gap between two boxes in a row, and between a box and a line passing it.
const NODE_SEP: f64 = 46.0;
const EDGE_SEP: f64 = 22.0;
/// Gap around a group, when something outside it is beside it.
const GROUP_SEP: f64 = 30.0;
/// Between two rows of boxes.
const RANK_SEP: f64 = 72.0;
/// Inside a group's border; more at the top, where its title is.
pub(crate) const GROUP_PAD: f64 = 24.0;
pub(crate) const GROUP_PAD_TITLE: f64 = 46.0;
const MARGIN: f64 = 24.0;
/// How far a line runs straight out of a box before it may turn.
const STUB: f64 = 14.0;
/// A label sits this far to the side of its line.
const LABEL_GAP: f64 = 7.0;
/// How near a line may come to a box it has nothing to do with, and to a
/// group it is not in, when it takes a straighter way than through its
/// placeholders.
const CLEAR_BOX: f64 = 10.0;
const CLEAR_GROUP: f64 = 6.0;
/// How far from a turn a straightened line starts to round it.
const CORNER: f64 = 27.0;
/// A line from a box to itself: how far its loop reaches out of the side.
const LOOP: f64 = 26.0;

pub(crate) struct InNode {
    /// Its group: an earlier node.
    pub parent: Option<usize>,
    pub group: bool,
    /// A box's size. For a group, `w` is the least width its title needs.
    pub w: f64,
    pub h: f64,
}

pub(crate) struct InEdge {
    pub source: usize,
    pub target: usize,
    /// Zero for no label.
    pub label_w: f64,
    pub label_h: f64,
}

/// `rank same { a, b }`: what the view asks of the rows.
pub(crate) struct InRank {
    pub rank: Rank,
    /// Input nodes; a group among them is left where it is.
    pub nodes: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    fn contains(&self, p: (f64, f64)) -> bool {
        let e = 1e-6;
        p.0 >= self.x - e && p.0 <= self.x + self.w + e && p.1 >= self.y - e && p.1 <= self.y + self.h + e
    }
}

pub(crate) struct Route {
    /// From the source to the target: corners for a smooth curve to follow.
    pub points: Vec<(f64, f64)>,
    pub label: Option<Rect>,
}

pub(crate) struct Output {
    pub nodes: Vec<Rect>,
    pub edges: Vec<Route>,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    /// A box from the input.
    Real,
    /// Where a line to or from a group as a whole ends, inside the group.
    /// `late`: below everything in it (the line leaves the group) rather
    /// than above (the line arrives).
    Anchor { late: bool },
    /// A line passing through a row.
    Dummy,
    /// The same, carrying the line's label.
    Label,
    /// Keeps a group's place in a row where it has nothing else.
    Spine,
}

struct Node {
    kind: Kind,
    cluster: usize,
    /// How far it reaches left and right of `x`.
    wl: f64,
    wr: f64,
    h: f64,
    rank: i32,
    x: f64,
    y: f64,
}

const ROOT: usize = 0;

struct Cluster {
    parent: usize,
    children: Vec<usize>,
    depth: usize,
    /// Padding inside the border: left, right, top, bottom.
    pad: [f64; 4],
    min_w: f64,
    /// First and last row it has anything in.
    r1: i32,
    r2: i32,
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
    /// Where it stands among its siblings, the same in every row.
    key: f64,
}

struct Edge {
    from: usize,
    to: usize,
    minlen: i32,
    weight: f64,
    /// The input edge this draws; `None` only holds an anchor in place.
    input: Option<usize>,
    reversed: bool,
    /// `from`, a placeholder per row between, `to`.
    chain: Vec<usize>,
}

struct Graph {
    nodes: Vec<Node>,
    clusters: Vec<Cluster>,
    edges: Vec<Edge>,
    /// Node indexes per row, left to right.
    ranks: Vec<Vec<usize>>,
    /// Between two rows of boxes, and between two boxes in a row.
    rank_sep: f64,
    node_sep: f64,
    /// For each node, the one it shares a row with because a `rank` says
    /// so: all of a set name the same node. Itself, for most.
    set: Vec<usize>,
    /// The set a `rank` puts on the first row, and the one on the last;
    /// `true` when nothing else may be on that row.
    first: Option<(usize, bool)>,
    last: Option<(usize, bool)>,
}

#[cfg(test)]
pub(crate) fn layout(nodes: &[InNode], edges: &[InEdge], direction: Direction) -> Output {
    layout_spaced(nodes, edges, direction, None, None, &[])
}

/// `rank_gap` and `node_gap`: how far apart rows are, and the boxes in a
/// row, where the view says rather than leaving it to the layout.
pub(crate) fn layout_spaced(
    nodes: &[InNode],
    edges: &[InEdge],
    direction: Direction,
    rank_gap: Option<f64>,
    node_gap: Option<f64>,
    ranks: &[InRank],
) -> Output {
    let horizontal = matches!(direction, Direction::LeftRight | Direction::RightLeft);
    if nodes.is_empty() {
        return Output { nodes: Vec::new(), edges: Vec::new(), width: 2.0 * MARGIN, height: 2.0 * MARGIN };
    }

    // ------------------------------------------------------------ build --
    let pad = match direction {
        Direction::TopBottom => [GROUP_PAD, GROUP_PAD, GROUP_PAD_TITLE, GROUP_PAD],
        Direction::BottomTop => [GROUP_PAD, GROUP_PAD, GROUP_PAD, GROUP_PAD_TITLE],
        // Turned: the title's side becomes the left.
        Direction::LeftRight | Direction::RightLeft => [GROUP_PAD_TITLE, GROUP_PAD, GROUP_PAD, GROUP_PAD],
    };
    let mut g = Graph {
        nodes: Vec::new(),
        clusters: vec![Cluster {
            parent: ROOT,
            children: Vec::new(),
            depth: 0,
            pad: [0.0; 4],
            min_w: 0.0,
            r1: i32::MAX,
            r2: i32::MIN,
            left: 0.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
            key: 0.0,
        }],
        edges: Vec::new(),
        ranks: Vec::new(),
        rank_sep: rank_gap.unwrap_or(RANK_SEP),
        node_sep: node_gap.unwrap_or(NODE_SEP),
        set: Vec::new(),
        first: None,
        last: None,
    };
    // For each input node: its cluster (a group) or its node (a box).
    let mut cluster_of = vec![usize::MAX; nodes.len()];
    let mut node_of = vec![usize::MAX; nodes.len()];
    for (i, input) in nodes.iter().enumerate() {
        let parent = input.parent.map(|p| cluster_of[p]).filter(|&c| c != usize::MAX).unwrap_or(ROOT);
        if input.group {
            let index = g.clusters.len();
            g.clusters.push(Cluster {
                parent,
                children: Vec::new(),
                depth: g.clusters[parent].depth + 1,
                pad,
                min_w: if horizontal { 0.0 } else { input.w },
                r1: i32::MAX,
                r2: i32::MIN,
                left: 0.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
                key: index as f64,
            });
            g.clusters[parent].children.push(index);
            cluster_of[i] = index;
        } else {
            let (w, h) = if horizontal { (input.h, input.w) } else { (input.w, input.h) };
            node_of[i] = g.add(Kind::Real, parent, w / 2.0, w / 2.0, h);
        }
    }
    let real_count = g.nodes.len();
    // A line from a box to itself is a loop out of its side. It takes no
    // part in ranking or ordering; the box only reaches further to the
    // side (before turning: to the right), for the loop and its words.
    let mut loops: Vec<usize> = Vec::new();
    let mut room = vec![0.0; nodes.len()];
    for (i, edge) in edges.iter().enumerate().filter(|(_, e)| e.source == e.target) {
        loops.push(i);
        let words = if horizontal { edge.label_h } else { edge.label_w };
        let need = LOOP + if words > 0.0 { LABEL_GAP + words } else { 0.0 };
        room[edge.source] = f64::max(room[edge.source], need);
    }
    for (i, &extra) in room.iter().enumerate() {
        if node_of[i] != usize::MAX {
            g.nodes[node_of[i]].wr += extra;
        }
    }
    for (i, edge) in edges.iter().enumerate().filter(|(_, e)| e.source != e.target) {
        let end = |g: &mut Graph, input: usize, late: bool| -> usize {
            if node_of[input] != usize::MAX {
                return node_of[input];
            }
            // A group: the line ends at an anchor inside it, held above
            // (or below) every box in the group.
            let cluster = cluster_of[input];
            let anchor = g.add(Kind::Anchor { late }, cluster, 0.0, 0.0, 0.0);
            for member in 0..real_count {
                if g.in_cluster(member, cluster) {
                    let (from, to) = if late { (member, anchor) } else { (anchor, member) };
                    g.edges.push(Edge {
                        from,
                        to,
                        minlen: 1,
                        weight: 0.0,
                        input: None,
                        reversed: false,
                        chain: Vec::new(),
                    });
                }
            }
            anchor
        };
        let from = end(&mut g, edge.source, true);
        let to = end(&mut g, edge.target, false);
        g.edges.push(Edge {
            from,
            to,
            minlen: 2,
            weight: 1.0,
            input: Some(i),
            reversed: false,
            chain: Vec::new(),
        });
    }

    // What `rank` says: the boxes of one block rank as one. Blocks that
    // share a box are one block; every `min` and `source` is one block, on
    // the first row, and every `max` and `sink` one on the last.
    g.set = (0..g.nodes.len()).collect();
    for rule in ranks {
        let mut members = rule.nodes.iter().map(|&i| node_of[i]).filter(|&n| n != usize::MAX);
        let Some(one) = members.next() else { continue };
        for other in members {
            g.join(one, other);
        }
        let alone = matches!(rule.rank, Rank::Source | Rank::Sink);
        let end = match rule.rank {
            Rank::Same => continue,
            Rank::Min | Rank::Source => &mut g.first,
            Rank::Max | Rank::Sink => &mut g.last,
        };
        let (before, was_alone) = end.unwrap_or((one, false));
        *end = Some((one, alone || was_alone));
        g.join(one, before);
    }
    for v in 0..g.set.len() {
        g.set[v] = g.find(v);
    }
    g.first = g.first.map(|(v, alone)| (g.set[v], alone));
    // One block cannot be on both the first row and the last.
    g.last = g.last.map(|(v, alone)| (g.set[v], alone)).filter(|&(v, _)| Some(v) != g.first.map(|f| f.0));

    g.break_cycles();
    g.rank();
    g.across(edges, horizontal);
    g.spans();
    g.fill_in(edges, horizontal);
    g.order();
    g.place_x();
    let bands = g.place_y();

    // ------------------------------------------------------------ route --
    let turn = |x: f64, y: f64| -> (f64, f64) {
        match direction {
            Direction::TopBottom => (x, y),
            Direction::BottomTop => (x, -y),
            Direction::LeftRight => (y, x),
            Direction::RightLeft => (-y, x),
        }
    };
    let turn_rect = |x0: f64, y0: f64, x1: f64, y1: f64| -> Rect {
        let (ax, ay) = turn(x0, y0);
        let (bx, by) = turn(x1, y1);
        Rect { x: ax.min(bx), y: ay.min(by), w: (ax - bx).abs(), h: (ay - by).abs() }
    };

    let mut out_nodes: Vec<Rect> = Vec::with_capacity(nodes.len());
    for i in 0..nodes.len() {
        out_nodes.push(if node_of[i] != usize::MAX {
            let n = &g.nodes[node_of[i]];
            turn_rect(n.x - n.wl, n.y - n.h / 2.0, n.x + n.wr - room[i], n.y + n.h / 2.0)
        } else {
            let c = &g.clusters[cluster_of[i]];
            turn_rect(c.left, c.top, c.right, c.bottom)
        });
    }

    // What a line must stay clear of: (left, top, right, bottom), and the
    // box, or after the boxes the group, it is.
    let mut boxes: Vec<([f64; 4], usize)> = Vec::new();
    for (i, n) in g.nodes.iter().enumerate() {
        match n.kind {
            Kind::Real => boxes.push((
                [
                    n.x - n.wl - CLEAR_BOX,
                    n.y - n.h / 2.0 - CLEAR_BOX,
                    n.x + n.wr + CLEAR_BOX,
                    n.y + n.h / 2.0 + CLEAR_BOX,
                ],
                i,
            )),
            // The words of a line, beside it.
            Kind::Label => boxes.push(([n.x + 2.0, n.y - n.h / 2.0, n.x + n.wr, n.y + n.h / 2.0], i)),
            _ => {}
        }
    }
    let groups: Vec<[f64; 4]> = g
        .clusters
        .iter()
        .map(|c| [c.left - CLEAR_GROUP, c.top - CLEAR_GROUP, c.right + CLEAR_GROUP, c.bottom + CLEAR_GROUP])
        .collect();

    // First every line through its placeholders, then each in turn as
    // straight as the others leave room for.
    struct Draft {
        input: usize,
        a: usize,
        b: usize,
        flat: bool,
        reversed: bool,
        pts: Vec<(f64, f64)>,
        /// The corners it keeps whatever else it may leave out.
        kept: Vec<usize>,
        /// The placeholder with its words, and that one's corners.
        words: Option<(usize, Vec<usize>)>,
        label: Option<Rect>,
    }
    let mut drafts: Vec<Draft> = Vec::new();
    for edge in &g.edges {
        let Some(input) = edge.input else { continue };
        let chain = &edge.chain;
        let (a, b) = (chain[0], chain[chain.len() - 1]);
        // Between two boxes of one row: out of the bottom of one, round
        // under, and into the bottom of the other.
        let flat = g.set[a] == g.set[b];
        let port = |n: usize, other: usize, below: bool| -> (f64, f64) {
            let node = &g.nodes[n];
            if node.kind != Kind::Real {
                return (node.x, node.y);
            }
            // Leave from the side facing the other row, shifted toward
            // where the line is going so lines from one box fan out.
            let reach = (node.wl - 18.0).max(0.0);
            let shift = ((g.nodes[other].x - node.x) * 0.35).clamp(-reach, reach);
            (node.x + shift, if below { node.y + node.h / 2.0 } else { node.y - node.h / 2.0 })
        };
        let start = port(a, chain[1], true);
        let end = port(b, chain[chain.len() - 2], flat);
        let mut pts = vec![start];
        // The corners a line keeps whatever else it may leave out: its
        // ends, straight out of a box and into one, and beside its words.
        let mut kept = vec![0];
        if g.nodes[a].kind == Kind::Real {
            pts.push((start.0, start.1 + STUB));
            kept.push(1);
            // A box shorter than its row: straight on to the row's edge
            // before turning, or the line would cut across its neighbours.
            let bottom = bands[g.nodes[a].rank as usize].1;
            if bottom > start.1 + STUB + 1.0 {
                pts.push((start.0, bottom));
            }
        }
        let mut label = None;
        // The placeholder with the line's words, and its corners.
        let mut words: Option<(usize, Vec<usize>)> = None;
        for &m in &chain[1..chain.len() - 1] {
            let node = &g.nodes[m];
            let (top, bottom) = bands[node.rank as usize];
            if bottom - top > 0.5 && !flat {
                // Straight down through a row that has boxes in it.
                pts.push((node.x, top));
                pts.push((node.x, bottom));
            } else {
                pts.push((node.x, node.y));
            }
            if node.kind == Kind::Label {
                let from = if bottom - top > 0.5 && !flat { pts.len() - 2 } else { pts.len() - 1 };
                words = Some((m, (from..pts.len()).collect()));
                label = Some(turn_rect(
                    node.x + LABEL_GAP,
                    node.y - node.h / 2.0,
                    node.x + node.wr - LABEL_GAP,
                    node.y + node.h / 2.0,
                ));
            }
        }
        if flat {
            let bottom = bands[g.nodes[b].rank as usize].1;
            if bottom > end.1 + STUB + 1.0 {
                pts.push((end.0, bottom));
            }
            pts.push((end.0, end.1 + STUB));
        } else if g.nodes[b].kind == Kind::Real {
            let top = bands[g.nodes[b].rank as usize].0;
            if top < end.1 - STUB - 1.0 {
                pts.push((end.0, top));
            }
            pts.push((end.0, end.1 - STUB));
            kept.push(pts.len() - 1);
        }
        pts.push(end);
        kept.push(pts.len() - 1);
        drafts.push(Draft { input, a, b, flat, reversed: edge.reversed, pts, kept, words, label });
    }

    let mut routes: Vec<Option<Route>> = (0..edges.len()).map(|_| None).collect();
    let mut lines: Vec<Vec<(f64, f64)>> = drafts.iter().map(|d| d.pts.clone()).collect();
    for (at, draft) in drafts.into_iter().enumerate() {
        let Draft { input, a, b, flat, reversed, mut pts, mut kept, words, mut label } = draft;
        // Through its placeholders a line turns a little at every row.
        // Between two corners it keeps it goes as straight as it can
        // without coming near a box or its words, or into a group neither
        // of its ends is in.
        if !flat {
            let (ca, cb) = (g.nodes[a].cluster, g.nodes[b].cluster);
            let straighten = |boxes: &[([f64; 4], usize)], kept: &[usize], but: usize| {
                let clear = |p: (f64, f64), q: (f64, f64)| {
                    boxes.iter().all(|&(r, n)| n == a || n == b || n == but || !crosses(p, q, r))
                        && (1..g.clusters.len())
                            .all(|c| g.around(c, ca) || g.around(c, cb) || !crosses(p, q, groups[c]))
                };
                let mut straight = vec![pts[0]];
                let mut i = 0;
                while i + 1 < pts.len() {
                    let limit = kept.iter().copied().find(|&k| k > i).unwrap_or(pts.len() - 1);
                    let next = (i + 2..=limit).rev().find(|&j| clear(pts[i], pts[j])).unwrap_or(i + 1);
                    straight.push(pts[next]);
                    i = next;
                }
                straight
            };
            // Its words go with it, beside wherever it now passes their
            // row, if there is room for them there, clear of every other
            // line; if not, the line keeps to where its words are.
            let mut moved = None;
            if let Some((m, _)) = &words {
                let node = &g.nodes[*m];
                let free = straighten(&boxes, &kept, *m);
                let (top, bottom) = (node.y - node.h / 2.0, node.y + node.h / 2.0);
                let x = x_at(&free, top).max(x_at(&free, bottom));
                let place = [x + 2.0, top, x + node.wr, bottom];
                let over =
                    |r: [f64; 4]| place[0] < r[2] && r[0] < place[2] && place[1] < r[3] && r[1] < place[3];
                let within = |r: [f64; 4]| {
                    place[0] >= r[0] && place[2] <= r[2] && place[1] >= r[1] && place[3] <= r[3]
                };
                let pad = [place[0] - 3.0, place[1] - 3.0, place[2] + 3.0, place[3] + 3.0];
                let fits = boxes.iter().all(|&(r, n)| n == *m || !over(r))
                    && g.clusters.iter().skip(1).all(|c| {
                        !over([c.left - 4.0, c.top - 4.0, c.right + 4.0, c.bottom + 4.0])
                            || within([c.left + 4.0, c.top + 4.0, c.right - 4.0, c.bottom - 4.0])
                    })
                    && lines.iter().enumerate().all(|(other, line)| {
                        other == at || line.windows(2).all(|w| !crosses(w[0], w[1], pad))
                    });
                if fits {
                    moved = Some((free, *m, place));
                    label = Some(turn_rect(x + LABEL_GAP, top, x + node.wr - LABEL_GAP, bottom));
                }
            }
            if let Some((free, m, place)) = moved {
                pts = free;
                if let Some(entry) = boxes.iter_mut().find(|(_, n)| *n == m) {
                    entry.0 = place;
                }
            } else {
                if let Some((_, corners)) = &words {
                    kept.extend(corners);
                    kept.sort_unstable();
                }
                pts = straighten(&boxes, &kept, usize::MAX);
            }
            pts = cornered(&pts);
        }
        // A line to a group stops at the group's border.
        if g.nodes[a].kind != Kind::Real {
            clip_start(&mut pts, &g.cluster_rect(g.nodes[a].cluster));
        }
        if g.nodes[b].kind != Kind::Real {
            pts.reverse();
            clip_start(&mut pts, &g.cluster_rect(g.nodes[b].cluster));
            pts.reverse();
        }
        lines[at] = pts.clone();
        if reversed {
            pts.reverse();
        }
        pts.dedup_by(|p, q| (p.0 - q.0).abs() < 0.01 && (p.1 - q.1).abs() < 0.01);
        routes[input] = Some(Route { points: pts.into_iter().map(|(x, y)| turn(x, y)).collect(), label });
    }
    // Several loops on one box share its side, one under another.
    for (k, &input) in loops.iter().enumerate() {
        let edge = &edges[input];
        let on_box = loops.iter().filter(|&&j| edges[j].source == edge.source).count();
        let nth = loops[..k].iter().filter(|&&j| edges[j].source == edge.source).count();
        let (right, top, h) = if node_of[edge.source] != usize::MAX {
            let n = &g.nodes[node_of[edge.source]];
            (n.x + n.wr - room[edge.source], n.y - n.h / 2.0, n.h)
        } else {
            let c = &g.clusters[cluster_of[edge.source]];
            (c.right, c.top, c.bottom - c.top)
        };
        let slice = h / on_box as f64;
        let middle = top + slice * (nth as f64 + 0.5);
        let half = (slice / 2.0 - 4.0).clamp(2.0, 14.0);
        let (words_w, words_h) =
            if horizontal { (edge.label_h, edge.label_w) } else { (edge.label_w, edge.label_h) };
        let label = (words_w > 0.0).then(|| {
            let x = right + LOOP + LABEL_GAP;
            turn_rect(x, middle - words_h / 2.0, x + words_w, middle + words_h / 2.0)
        });
        let pts = [
            (right, middle - half),
            (right + LOOP, middle - half),
            (right + LOOP, middle + half),
            (right, middle + half),
        ];
        routes[input] = Some(Route { points: pts.into_iter().map(|(x, y)| turn(x, y)).collect(), label });
    }
    let mut out_edges: Vec<Route> =
        routes.into_iter().map(|r| r.unwrap_or(Route { points: Vec::new(), label: None })).collect();

    // --------------------------------------------------------- finish --
    // Move everything so the drawing starts at the margin.
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    let mut cover = |r: &Rect| {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x + r.w);
        y1 = y1.max(r.y + r.h);
    };
    out_nodes.iter().for_each(&mut cover);
    for route in &out_edges {
        route.label.iter().for_each(&mut cover);
        for &(x, y) in &route.points {
            cover(&Rect { x, y, w: 0.0, h: 0.0 });
        }
    }
    let (dx, dy) = (MARGIN - x0, MARGIN - y0);
    for rect in &mut out_nodes {
        rect.x += dx;
        rect.y += dy;
    }
    for route in &mut out_edges {
        for p in &mut route.points {
            p.0 += dx;
            p.1 += dy;
        }
        if let Some(label) = &mut route.label {
            label.x += dx;
            label.y += dy;
        }
    }
    Output {
        nodes: out_nodes,
        edges: out_edges,
        width: x1 - x0 + 2.0 * MARGIN,
        height: y1 - y0 + 2.0 * MARGIN,
    }
}

/// The same path with a corner more before and after each turn. The curve
/// drawn through a path cuts its corners, the further the longer the
/// straights either side: this keeps it within a third of `CORNER` of
/// the path, which is nearer than a line is let come to a box.
fn cornered(pts: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out = Vec::with_capacity(pts.len() * 3);
    for pair in pts.windows(2) {
        let (p, q) = (pair[0], pair[1]);
        out.push(p);
        let length = ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt();
        if length > 2.0 * CORNER {
            let t = CORNER / length;
            out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
            out.push((q.0 - (q.0 - p.0) * t, q.1 - (q.1 - p.1) * t));
        }
    }
    out.extend(pts.last());
    out
}

/// Where a path that only goes down is, across, when it is `y` down.
fn x_at(pts: &[(f64, f64)], y: f64) -> f64 {
    for pair in pts.windows(2) {
        let (p, q) = (pair[0], pair[1]);
        if y <= q.1 {
            let t = if q.1 - p.1 > 1e-9 { ((y - p.1) / (q.1 - p.1)).clamp(0.0, 1.0) } else { 0.0 };
            return p.0 + (q.0 - p.0) * t;
        }
    }
    pts.last().map_or(0.0, |p| p.0)
}

/// Whether the straight line from `p` to `q` touches the rectangle
/// (left, top, right, bottom).
fn crosses(p: (f64, f64), q: (f64, f64), r: [f64; 4]) -> bool {
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    // For each side: how far along the line it is crossed, going in or out.
    for (d, near, far) in [(dx, r[0] - p.0, r[2] - p.0), (dy, r[1] - p.1, r[3] - p.1)] {
        if d.abs() < 1e-9 {
            if near > 0.0 || far < 0.0 {
                return false;
            }
        } else {
            let (a, b) = (near / d, far / d);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

/// Cuts a path that starts inside `rect` back to where it last leaves it.
fn clip_start(pts: &mut Vec<(f64, f64)>, rect: &Rect) {
    for i in (0..pts.len().saturating_sub(1)).rev() {
        let (p, q) = (pts[i], pts[i + 1]);
        if rect.contains(p) && !rect.contains(q) {
            // How far along p -> q the border is.
            let mut t: f64 = 1.0;
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            if dx > 0.0 {
                t = t.min((rect.x + rect.w - p.0) / dx);
            } else if dx < 0.0 {
                t = t.min((rect.x - p.0) / dx);
            }
            if dy > 0.0 {
                t = t.min((rect.y + rect.h - p.1) / dy);
            } else if dy < 0.0 {
                t = t.min((rect.y - p.1) / dy);
            }
            let t = t.clamp(0.0, 1.0);
            let cut = (p.0 + dx * t, p.1 + dy * t);
            pts.drain(..=i);
            pts.insert(0, cut);
            return;
        }
    }
}

impl Graph {
    fn add(&mut self, kind: Kind, cluster: usize, wl: f64, wr: f64, h: f64) -> usize {
        self.nodes.push(Node { kind, cluster, wl, wr, h, rank: 0, x: 0.0, y: 0.0 });
        self.nodes.len() - 1
    }

    /// The node that stands for `v`'s set.
    fn find(&self, mut v: usize) -> usize {
        while self.set[v] != v {
            v = self.set[v];
        }
        v
    }

    /// Makes one set of `a`'s and `b`'s.
    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        self.set[a.max(b)] = a.min(b);
    }

    /// Each step of a line, as (the node in the upper row, the one below).
    fn hops(&self, edge: &Edge) -> Vec<(usize, usize)> {
        let chain = &edge.chain;
        if chain.len() == 3 && self.set[chain[0]] == self.set[chain[2]] {
            return vec![(chain[0], chain[1]), (chain[2], chain[1])];
        }
        chain.windows(2).map(|pair| (pair[0], pair[1])).collect()
    }

    /// Whether cluster `a` is `b` or around it.
    fn around(&self, a: usize, mut b: usize) -> bool {
        loop {
            if a == b {
                return true;
            }
            if b == ROOT {
                return false;
            }
            b = self.clusters[b].parent;
        }
    }

    fn in_cluster(&self, node: usize, cluster: usize) -> bool {
        self.around(cluster, self.nodes[node].cluster)
    }

    fn cluster_rect(&self, c: usize) -> Rect {
        let c = &self.clusters[c];
        Rect { x: c.left, y: c.top, w: c.right - c.left, h: c.bottom - c.top }
    }

    /// The child of `ancestor` that `c` is, or is inside.
    fn child_toward(&self, ancestor: usize, mut c: usize) -> usize {
        while self.clusters[c].parent != ancestor {
            c = self.clusters[c].parent;
        }
        c
    }

    fn common(&self, mut a: usize, mut b: usize) -> usize {
        while a != b {
            if self.clusters[a].depth >= self.clusters[b].depth {
                a = self.clusters[a].parent;
            } else {
                b = self.clusters[b].parent;
            }
        }
        a
    }

    // ----------------------------------------------------------- 1. rank --

    /// Turns around every line that closes a loop, found depth first.
    /// Boxes a `rank` puts on one row count as one box here.
    fn break_cycles(&mut self) {
        let n = self.nodes.len();
        // Nothing comes before the first row, or after the last.
        let (first, last) = (self.first.map(|f| f.0), self.last.map(|l| l.0));
        for e in 0..self.edges.len() {
            let (from, to) = (self.set[self.edges[e].from], self.set[self.edges[e].to]);
            if from != to && self.edges[e].input.is_some() && (Some(to) == first || Some(from) == last) {
                self.turn(e);
            }
        }
        // Each pass ends early only by turning a line that had not been
        // turned in it; the bound is for safety.
        for _ in 0..=self.edges.len() {
            let mut out: Vec<Vec<usize>> = vec![Vec::new(); n];
            for (i, e) in self.edges.iter().enumerate() {
                if self.set[e.from] != self.set[e.to] {
                    out[self.set[e.from]].push(i);
                }
            }
            // 0 unseen, 1 on the path being walked, 2 done.
            let mut state = vec![0u8; n];
            let mut flip = Vec::new();
            let mut again = false;
            'walk: for root in 0..n {
                if state[root] != 0 || self.set[root] != root {
                    continue;
                }
                // (node, its next line out, the line that led here)
                let mut stack = vec![(root, 0usize, usize::MAX)];
                state[root] = 1;
                while let Some(&mut (v, ref mut next, _)) = stack.last_mut() {
                    if *next < out[v].len() {
                        let e = out[v][*next];
                        *next += 1;
                        let w = self.set[self.edges[e].to];
                        match state[w] {
                            0 => {
                                state[w] = 1;
                                stack.push((w, 0, e));
                            }
                            1 if self.edges[e].input.is_some() => flip.push(e),
                            1 => {
                                // What holds an anchor over its group is
                                // not a line to turn: turn the last real
                                // one on the way round, and look again.
                                let start = stack.iter().position(|s| s.0 == w).unwrap_or(0);
                                let real = stack[start + 1..]
                                    .iter()
                                    .rev()
                                    .map(|s| s.2)
                                    .find(|&l| self.edges[l].input.is_some() && !flip.contains(&l));
                                flip.push(real.unwrap_or(e));
                                again = real.is_some();
                                if again {
                                    break 'walk;
                                }
                            }
                            _ => {}
                        }
                    } else {
                        state[v] = 2;
                        stack.pop();
                    }
                }
            }
            for e in flip {
                self.turn(e);
            }
            if !again {
                break;
            }
        }
    }

    fn turn(&mut self, e: usize) {
        let edge = &mut self.edges[e];
        std::mem::swap(&mut edge.from, &mut edge.to);
        edge.reversed = !edge.reversed;
    }

    fn rank(&mut self) {
        let n = self.nodes.len();
        // A set of boxes on one row is ranked as the node that stands for
        // it; a line between two of them says nothing about rows.
        let set = self.set.clone();
        let mut ins: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut outs: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, e) in self.edges.iter().enumerate() {
            if set[e.from] != set[e.to] {
                outs[set[e.from]].push(i);
                ins[set[e.to]].push(i);
            }
        }
        // Longest path from the top: every box as high as its lines allow.
        let mut waiting: Vec<usize> = ins.iter().map(|v| v.len()).collect();
        let mut ready: Vec<usize> = (0..n).filter(|&v| waiting[v] == 0).collect();
        let mut rank = vec![0i32; n];
        let mut at = 0;
        while at < ready.len() {
            let v = ready[at];
            at += 1;
            for &e in &outs[v] {
                let edge = &self.edges[e];
                let to = set[edge.to];
                rank[to] = rank[to].max(rank[v] + edge.minlen);
                waiting[to] -= 1;
                if waiting[to] == 0 {
                    ready.push(to);
                }
            }
        }
        let (first, last) = (self.first.map(|f| f.0), self.last.map(|l| l.0));
        if let Some(last) = last {
            rank[last] = (0..n).filter(|&v| set[v] == v).map(|v| rank[v]).max().unwrap_or(0);
        }
        // Then shorten: a box with more lines going down than coming in is
        // moved down to meet them, and the other way round. Each move makes
        // the lines shorter in total, so this ends. What a `rank` holds at
        // the first or last row stays there.
        for _ in 0..100 {
            let mut changed = false;
            for v in 0..n {
                if set[v] != v || Some(v) == first || Some(v) == last {
                    continue;
                }
                let (mut lo, mut hi) = (i32::MIN, i32::MAX);
                let (mut w_in, mut w_out) = (0.0, 0.0);
                for &e in &ins[v] {
                    let edge = &self.edges[e];
                    lo = lo.max(rank[set[edge.from]] + edge.minlen);
                    w_in += edge.weight;
                }
                for &e in &outs[v] {
                    let edge = &self.edges[e];
                    hi = hi.min(rank[set[edge.to]] - edge.minlen);
                    w_out += edge.weight;
                }
                let target = match self.nodes[v].kind {
                    Kind::Anchor { late: false } => hi,
                    Kind::Anchor { late: true } => lo,
                    _ if w_in > w_out => lo,
                    _ if w_out > w_in => hi,
                    _ => rank[v],
                };
                if target != i32::MIN
                    && target != i32::MAX
                    && target != rank[v]
                    && target >= lo
                    && target <= hi
                {
                    rank[v] = target;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // `source` and `sink`: a row of their own, a whole step (a row of
        // boxes and the row of labels before it) away from the rest.
        let shared = |rank: &[i32], v: usize| {
            (0..n).any(|u| self.nodes[u].kind == Kind::Real && set[u] != v && rank[set[u]] == rank[v])
        };
        if let Some((v, true)) = self.first
            && shared(&rank, v)
            && ins[v].iter().all(|&e| rank[set[self.edges[e].from]] + self.edges[e].minlen <= rank[v] - 2)
        {
            rank[v] -= 2;
        }
        if let Some((v, true)) = self.last
            && shared(&rank, v)
            && outs[v].iter().all(|&e| rank[set[self.edges[e].to]] - self.edges[e].minlen >= rank[v] + 2)
        {
            rank[v] += 2;
        }
        let top = (0..n).map(|v| rank[set[v]]).min().unwrap_or(0);
        for (v, node) in self.nodes.iter_mut().enumerate() {
            node.rank = rank[set[v]] - top;
        }
    }

    /// A line between two boxes of one row goes round under them: it gets
    /// its one placeholder, in the row below, before groups learn their rows.
    fn across(&mut self, inputs: &[InEdge], horizontal: bool) {
        for e in 0..self.edges.len() {
            let (from, to) = (self.edges[e].from, self.edges[e].to);
            let Some(input) = self.edges[e].input else { continue };
            if self.set[from] != self.set[to] {
                continue;
            }
            let (label_w, label_h) = match (inputs[input].label_w, inputs[input].label_h) {
                (w, h) if horizontal => (h, w),
                (w, h) => (w, h),
            };
            let cluster = self.common(self.nodes[from].cluster, self.nodes[to].cluster);
            let node = if label_w > 0.0 && label_h > 0.0 {
                self.add(Kind::Label, cluster, LABEL_GAP, label_w + 2.0 * LABEL_GAP, label_h)
            } else {
                self.add(Kind::Dummy, cluster, 0.0, 0.0, 0.0)
            };
            self.nodes[node].rank = self.nodes[from].rank + 1;
            self.set.push(node);
            self.edges[e].chain = vec![from, node, to];
        }
    }

    /// The rows each group covers.
    fn spans(&mut self) {
        let last = self.nodes.iter().map(|n| n.rank).max().unwrap_or(0);
        for i in 0..self.nodes.len() {
            let rank = self.nodes[i].rank;
            let mut c = self.nodes[i].cluster;
            while c != ROOT {
                self.clusters[c].r1 = self.clusters[c].r1.min(rank);
                self.clusters[c].r2 = self.clusters[c].r2.max(rank);
                c = self.clusters[c].parent;
            }
        }
        self.clusters[ROOT].r1 = 0;
        self.clusters[ROOT].r2 = last;
    }

    // -------------------------------------------------------- 2. fill in --

    /// The innermost cluster around `node` that covers row `rank`.
    fn covering(&self, node: usize, rank: i32) -> usize {
        let mut c = self.nodes[node].cluster;
        while c != ROOT && !(self.clusters[c].r1 <= rank && rank <= self.clusters[c].r2) {
            c = self.clusters[c].parent;
        }
        c
    }

    fn fill_in(&mut self, inputs: &[InEdge], horizontal: bool) {
        for e in 0..self.edges.len() {
            let Some(input) = self.edges[e].input else { continue };
            if !self.edges[e].chain.is_empty() {
                continue;
            }
            let (from, to) = (self.edges[e].from, self.edges[e].to);
            let (r_from, r_to) = (self.nodes[from].rank, self.nodes[to].rank);
            let (label_w, label_h) = match (inputs[input].label_w, inputs[input].label_h) {
                (w, h) if horizontal => (h, w),
                (w, h) => (w, h),
            };
            let label_rank = if label_w > 0.0 && label_h > 0.0 { (r_from + r_to) / 2 } else { -1 };
            let mut chain = vec![from];
            for rank in r_from + 1..r_to {
                // A line into a group runs inside the group from the first
                // row the group covers: it comes in through the border
                // above its target, not across the group's other members.
                let (cu, cv) = (self.covering(from, rank), self.covering(to, rank));
                let cluster = if self.around(cu, cv) {
                    cv
                } else if self.around(cv, cu) || rank - r_from <= r_to - rank {
                    cu
                } else {
                    cv
                };
                let node = if rank == label_rank {
                    self.add(Kind::Label, cluster, LABEL_GAP, label_w + 2.0 * LABEL_GAP, label_h)
                } else {
                    self.add(Kind::Dummy, cluster, 0.0, 0.0, 0.0)
                };
                self.nodes[node].rank = rank;
                self.set.push(node);
                chain.push(node);
            }
            chain.push(to);
            self.edges[e].chain = chain;
        }

        // A group with nothing in one of its rows still needs a place in
        // that row, or whatever is there would not know which side of the
        // group to be on. Innermost groups first: one's placeholder also
        // stands for the groups around it.
        let rows = self.clusters[ROOT].r2 as usize + 1;
        let mut present = vec![vec![false; rows]; self.clusters.len()];
        for node in &self.nodes {
            let mut c = node.cluster;
            while c != ROOT {
                present[c][node.rank as usize] = true;
                c = self.clusters[c].parent;
            }
        }
        let mut by_depth: Vec<usize> = (1..self.clusters.len()).collect();
        by_depth.sort_by_key(|&c| std::cmp::Reverse(self.clusters[c].depth));
        for c in by_depth {
            for rank in self.clusters[c].r1..=self.clusters[c].r2 {
                if !present[c][rank as usize] {
                    let node = self.add(Kind::Spine, c, 0.0, 0.0, 0.0);
                    self.nodes[node].rank = rank;
                    self.set.push(node);
                    let mut up = c;
                    while up != ROOT {
                        present[up][rank as usize] = true;
                        up = self.clusters[up].parent;
                    }
                }
            }
        }

        self.ranks = vec![Vec::new(); rows];
        for (i, node) in self.nodes.iter().enumerate() {
            self.ranks[node.rank as usize].push(i);
        }
    }

    // ---------------------------------------------------------- 3. order --

    /// Sorts one row by `value`, keeping each group's members together and
    /// sibling groups in the order of their keys.
    fn arrange(&self, row: &[usize], value: &[f64]) -> Vec<usize> {
        let count = self.clusters.len();
        // For each cluster: its own nodes in this row, and the sum and
        // number of values anywhere inside it.
        let mut own: Vec<Vec<usize>> = vec![Vec::new(); count];
        let mut sum = vec![0.0; count];
        let mut inside = vec![0usize; count];
        for &n in row {
            own[self.nodes[n].cluster].push(n);
            let mut c = self.nodes[n].cluster;
            loop {
                sum[c] += value[n];
                inside[c] += 1;
                if c == ROOT {
                    break;
                }
                c = self.clusters[c].parent;
            }
        }
        let mut out = Vec::with_capacity(row.len());
        self.emit(ROOT, &own, &sum, &inside, value, &mut out);
        out
    }

    fn emit(
        &self,
        c: usize,
        own: &[Vec<usize>],
        sum: &[f64],
        inside: &[usize],
        value: &[f64],
        out: &mut Vec<usize>,
    ) {
        // (value, is a cluster, index)
        let mut items: Vec<(f64, bool, usize)> = own[c].iter().map(|&n| (value[n], false, n)).collect();
        for &child in &self.clusters[c].children {
            if inside[child] > 0 {
                items.push((sum[child] / inside[child] as f64, true, child));
            }
        }
        items.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        // The groups among them, back into the places groups hold, in the
        // one order they keep in every row.
        let slots: Vec<usize> = (0..items.len()).filter(|&i| items[i].1).collect();
        let mut groups: Vec<(f64, bool, usize)> = slots.iter().map(|&i| items[i]).collect();
        groups.sort_by(|a, b| {
            let (ka, kb) = (self.clusters[a.2].key, self.clusters[b.2].key);
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal).then(a.2.cmp(&b.2))
        });
        for (slot, group) in slots.into_iter().zip(groups) {
            items[slot] = group;
        }
        for (_, is_cluster, index) in items {
            if is_cluster {
                self.emit(index, own, sum, inside, value, out);
            } else {
                out.push(index);
            }
        }
    }

    fn order(&mut self) {
        let n = self.nodes.len();
        let mut up: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut down: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut segments: Vec<(usize, usize)> = Vec::new();
        for edge in &self.edges {
            if edge.input.is_some() {
                for (a, b) in self.hops(edge) {
                    down[a].push(b);
                    up[b].push(a);
                    segments.push((a, b));
                }
            } else if self.nodes[edge.to].rank - self.nodes[edge.from].rank == 1 {
                down[edge.from].push(edge.to);
                up[edge.to].push(edge.from);
            }
        }

        let mut pos = vec![0.0f64; n];
        let set_pos = |ranks: &[Vec<usize>], pos: &mut Vec<f64>| {
            for row in ranks {
                for (i, &node) in row.iter().enumerate() {
                    pos[node] = i as f64;
                }
            }
        };
        let rank_of: Vec<usize> = self.nodes.iter().map(|node| node.rank as usize).collect();
        let crossings = |ranks: &[Vec<usize>], pos: &[f64]| -> usize {
            let mut per_row: Vec<Vec<(f64, f64)>> = vec![Vec::new(); ranks.len()];
            for &(a, b) in &segments {
                per_row[rank_of[a]].push((pos[a], pos[b]));
            }
            let mut total = 0;
            for row in &per_row {
                for (i, p) in row.iter().enumerate() {
                    for q in &row[i + 1..] {
                        if (p.0 < q.0 && p.1 > q.1) || (p.0 > q.0 && p.1 < q.1) {
                            total += 1;
                        }
                    }
                }
            }
            total
        };

        // Start with groups together, in the order things were written.
        set_pos(&self.ranks, &mut pos);
        for r in 0..self.ranks.len() {
            self.ranks[r] = self.arrange(&self.ranks[r], &pos);
        }
        set_pos(&self.ranks, &mut pos);
        let mut best = (crossings(&self.ranks, &pos), self.ranks.clone());

        for sweep in 0..24 {
            if best.0 == 0 {
                break;
            }
            let downward = sweep % 2 == 0;
            let rows: Vec<usize> = if downward {
                (1..self.ranks.len()).collect()
            } else {
                (0..self.ranks.len().saturating_sub(1)).rev().collect()
            };
            for r in rows {
                // Each node goes where the nodes it connects to in the row
                // just done are, on average; one with none stays put.
                let mut value = pos.clone();
                for &node in &self.ranks[r] {
                    let towards = if downward { &up[node] } else { &down[node] };
                    if !towards.is_empty() {
                        value[node] = towards.iter().map(|&t| pos[t]).sum::<f64>() / towards.len() as f64;
                    }
                }
                self.ranks[r] = self.arrange(&self.ranks[r], &value);
                for (i, &node) in self.ranks[r].iter().enumerate() {
                    pos[node] = i as f64;
                }
            }
            self.rekey(&pos);
            let count = crossings(&self.ranks, &pos);
            if count < best.0 {
                best = (count, self.ranks.clone());
            }
        }

        // Settle on the best, and make every row agree on which way round
        // sibling groups go: positions depend on it.
        self.ranks = best.1;
        set_pos(&self.ranks, &mut pos);
        self.rekey(&pos);
        for r in 0..self.ranks.len() {
            self.ranks[r] = self.arrange(&self.ranks[r], &pos);
        }
        set_pos(&self.ranks, &mut pos);

        // Then two neighbours in a row change places wherever their lines
        // cross fewer times that way. Only two of one group: groups stay
        // whole and in their order.
        let mut ends: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(a, b) in &segments {
            ends[a].push(b);
            ends[b].push(a);
        }
        // How often lines from `u` cross lines from `v`, `u` being first.
        let crossed = |pos: &[f64], u: usize, v: usize| -> usize {
            let mut total = 0;
            for &p in &ends[u] {
                for &q in &ends[v] {
                    if rank_of[p] == rank_of[q] && pos[p] > pos[q] {
                        total += 1;
                    }
                }
            }
            total
        };
        for _ in 0..12 {
            let mut changed = false;
            for r in 0..self.ranks.len() {
                for i in 0..self.ranks[r].len().saturating_sub(1) {
                    let (u, v) = (self.ranks[r][i], self.ranks[r][i + 1]);
                    if self.nodes[u].cluster == self.nodes[v].cluster
                        && crossed(&pos, v, u) < crossed(&pos, u, v)
                    {
                        self.ranks[r].swap(i, i + 1);
                        pos.swap(u, v);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Each group's key: how far along its rows its members are, on average.
    fn rekey(&mut self, pos: &[f64]) {
        let mut sum = vec![0.0; self.clusters.len()];
        let mut count = vec![0usize; self.clusters.len()];
        for (i, node) in self.nodes.iter().enumerate() {
            let len = self.ranks[node.rank as usize].len() as f64;
            let mut c = node.cluster;
            while c != ROOT {
                sum[c] += (pos[i] + 0.5) / len;
                count[c] += 1;
                c = self.clusters[c].parent;
            }
        }
        for c in 1..self.clusters.len() {
            if count[c] > 0 {
                self.clusters[c].key = sum[c] / count[c] as f64;
            }
        }
    }

    // ---------------------------------------------------------- 4. place --

    fn place_x(&mut self) {
        let n = self.nodes.len();
        let left_of = |c: usize| n + 2 * c;
        let right_of = |c: usize| n + 2 * c + 1;
        let vars = n + 2 * self.clusters.len();

        // What must be left of what, and by how much.
        let mut cons: Vec<(usize, usize, f64)> = Vec::new();
        for c in 1..self.clusters.len() {
            let cluster = &self.clusters[c];
            cons.push((left_of(c), right_of(c), cluster.min_w));
            if cluster.parent != ROOT {
                let outer = &self.clusters[cluster.parent];
                cons.push((left_of(cluster.parent), left_of(c), outer.pad[0]));
                cons.push((right_of(c), right_of(cluster.parent), outer.pad[1]));
            }
        }
        for (i, node) in self.nodes.iter().enumerate() {
            if node.cluster != ROOT {
                let cluster = &self.clusters[node.cluster];
                cons.push((left_of(node.cluster), i, cluster.pad[0] + node.wl));
                cons.push((i, right_of(node.cluster), node.wr + cluster.pad[1]));
            }
        }
        let node_sep = self.node_sep;
        let sep = |kind: Kind| if matches!(kind, Kind::Dummy | Kind::Label) { EDGE_SEP } else { node_sep };
        for row in &self.ranks {
            for pair in row.windows(2) {
                let (a, b) = (&self.nodes[pair[0]], &self.nodes[pair[1]]);
                if a.cluster == b.cluster {
                    cons.push((pair[0], pair[1], a.wr + (sep(a.kind) + sep(b.kind)) / 2.0 + b.wl));
                    continue;
                }
                let shared = self.common(a.cluster, b.cluster);
                let (left, reach_l) = if a.cluster == shared {
                    (pair[0], a.wr)
                } else {
                    (right_of(self.child_toward(shared, a.cluster)), 0.0)
                };
                let (right, reach_r) = if b.cluster == shared {
                    (pair[1], b.wl)
                } else {
                    (left_of(self.child_toward(shared, b.cluster)), 0.0)
                };
                cons.push((left, right, reach_l + GROUP_SEP + reach_r));
            }
        }

        let mut ins: Vec<Vec<usize>> = vec![Vec::new(); vars];
        let mut outs: Vec<Vec<usize>> = vec![Vec::new(); vars];
        for (i, &(l, r, _)) in cons.iter().enumerate() {
            outs[l].push(i);
            ins[r].push(i);
        }
        let mut waiting: Vec<usize> = ins.iter().map(|v| v.len()).collect();
        let mut topo: Vec<usize> = (0..vars).filter(|&v| waiting[v] == 0).collect();
        let mut at = 0;
        while at < topo.len() {
            let v = topo[at];
            at += 1;
            for &c in &outs[v] {
                let r = cons[c].1;
                waiting[r] -= 1;
                if waiting[r] == 0 {
                    topo.push(r);
                }
            }
        }
        // Cannot happen while rows agree on the order of groups; if it ever
        // did, an overlap is better than no drawing.
        if topo.len() < vars {
            let seen: std::collections::BTreeSet<usize> = topo.iter().copied().collect();
            topo.extend((0..vars).filter(|v| !seen.contains(v)));
        }

        // Start packed to the left.
        let mut x = vec![0.0f64; vars];
        for &v in &topo {
            for &c in &ins[v] {
                x[v] = x[v].max(x[cons[c].0] + cons[c].2);
            }
        }

        // What pulls on what: lines, more strongly between two placeholders
        // so long lines run straight. An anchor is drawn to the middle of
        // its group without pulling back.
        let mut pulls: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        let soft = |kind: Kind| matches!(kind, Kind::Dummy | Kind::Label);
        for edge in &self.edges {
            if edge.input.is_some() {
                for pair in edge.chain.windows(2) {
                    let w = match (soft(self.nodes[pair[0]].kind), soft(self.nodes[pair[1]].kind)) {
                        (true, true) => 24.0,
                        (false, false) => 1.0,
                        _ => 2.0,
                    };
                    pulls[pair[0]].push((pair[1], w));
                    pulls[pair[1]].push((pair[0], w));
                }
            } else {
                let (anchor, member) = if matches!(self.nodes[edge.from].kind, Kind::Anchor { .. }) {
                    (edge.from, edge.to)
                } else {
                    (edge.to, edge.from)
                };
                pulls[anchor].push((member, 0.25));
            }
        }
        let mut by_depth: Vec<usize> = (1..self.clusters.len()).collect();
        by_depth.sort_by_key(|&c| std::cmp::Reverse(self.clusters[c].depth));
        let mut members: Vec<Vec<usize>> = vec![Vec::new(); self.clusters.len()];
        for (i, node) in self.nodes.iter().enumerate() {
            members[node.cluster].push(i);
        }

        // Nodes row by row, for sweeping down and back up.
        let down_order: Vec<usize> = self.ranks.iter().flatten().copied().collect();
        let up_order: Vec<usize> = self.ranks.iter().rev().flatten().copied().collect();

        let mut want = vec![0.0f64; vars];
        let mut weight = vec![0.0f64; vars];
        for sweep in 0..32 {
            // Each node in turn moves to the middle of what pulls on it,
            // the next one seeing where the last one went: one sweep
            // carries a move the whole length of a long line.
            let mut moved = x.clone();
            for &v in if sweep % 2 == 0 { &down_order } else { &up_order } {
                let total: f64 = pulls[v].iter().map(|p| p.1).sum();
                if total > 0.0 {
                    moved[v] = pulls[v].iter().map(|&(other, w)| moved[other] * w).sum::<f64>() / total;
                    weight[v] = total;
                } else {
                    weight[v] = 0.05;
                }
                want[v] = moved[v];
            }
            // A border wants to be tight against what it holds.
            for &c in &by_depth {
                let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                for &m in &members[c] {
                    lo = lo.min(x[m] - self.nodes[m].wl);
                    hi = hi.max(x[m] + self.nodes[m].wr);
                }
                for &child in &self.clusters[c].children {
                    lo = lo.min(want[left_of(child)]);
                    hi = hi.max(want[right_of(child)]);
                }
                want[left_of(c)] = lo - self.clusters[c].pad[0];
                want[right_of(c)] = hi + self.clusters[c].pad[1];
                weight[left_of(c)] = 0.01;
                weight[right_of(c)] = 0.01;
            }
            want[left_of(ROOT)] = x[left_of(ROOT)];
            want[right_of(ROOT)] = x[right_of(ROOT)];
            weight[left_of(ROOT)] = 0.01;
            weight[right_of(ROOT)] = 0.01;
            x = project(&cons, &ins, &topo, &want, &weight);
        }

        for (i, node) in self.nodes.iter_mut().enumerate() {
            node.x = x[i];
        }
        // Borders: tight around what is inside, never past where the
        // solution had them (that room belongs to something else).
        for &c in &by_depth {
            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for &m in &members[c] {
                lo = lo.min(self.nodes[m].x - self.nodes[m].wl);
                hi = hi.max(self.nodes[m].x + self.nodes[m].wr);
            }
            for &child in &self.clusters[c].children {
                lo = lo.min(self.clusters[child].left);
                hi = hi.max(self.clusters[child].right);
            }
            let cluster = &mut self.clusters[c];
            let (solved_l, solved_r) = (x[left_of(c)], x[right_of(c)]);
            let (mut left, mut right) =
                ((lo - cluster.pad[0]).max(solved_l), (hi + cluster.pad[1]).min(solved_r));
            if right - left < cluster.min_w {
                let middle = (left + right) / 2.0;
                left =
                    (middle - cluster.min_w / 2.0).clamp(solved_l, (solved_r - cluster.min_w).max(solved_l));
                right = left + cluster.min_w;
            }
            cluster.left = left;
            cluster.right = right;
        }
    }

    /// Top-to-bottom positions; returns each row's top and bottom.
    fn place_y(&mut self) -> Vec<(f64, f64)> {
        let rows = self.ranks.len();
        let mut height = vec![0.0f64; rows];
        for node in &self.nodes {
            height[node.rank as usize] = height[node.rank as usize].max(node.h);
        }
        // Room above a row for the borders (and titles) of the groups that
        // start in it, and below for those that end in it.
        let mut by_depth: Vec<usize> = (1..self.clusters.len()).collect();
        by_depth.sort_by_key(|&c| std::cmp::Reverse(self.clusters[c].depth));
        let mut above = vec![0.0f64; self.clusters.len()];
        let mut below = vec![0.0f64; self.clusters.len()];
        let mut extra_above = vec![0.0f64; rows];
        let mut extra_below = vec![0.0f64; rows];
        for &c in &by_depth {
            let cluster = &self.clusters[c];
            let (mut a, mut b) = (0.0f64, 0.0f64);
            for &child in &cluster.children {
                if self.clusters[child].r1 == cluster.r1 {
                    a = a.max(above[child]);
                }
                if self.clusters[child].r2 == cluster.r2 {
                    b = b.max(below[child]);
                }
            }
            above[c] = a + cluster.pad[2];
            below[c] = b + cluster.pad[3];
            extra_above[cluster.r1 as usize] = extra_above[cluster.r1 as usize].max(above[c]);
            extra_below[cluster.r2 as usize] = extra_below[cluster.r2 as usize].max(below[c]);
        }
        // A line that moves far sideways between two rows needs the rows
        // further apart, or it runs almost flat along the boxes.
        let rank_sep = self.rank_sep;
        let mut gap = vec![rank_sep / 2.0; rows];
        for edge in &self.edges {
            if edge.input.is_some() {
                for (a, b) in self.hops(edge) {
                    let (a, b) = (&self.nodes[a], &self.nodes[b]);
                    let sideways = (a.x - b.x).abs() * 0.8;
                    let r = a.rank as usize;
                    gap[r] = gap[r].max((sideways * 0.3).min(rank_sep * 1.5));
                }
            }
        }
        let mut bands = Vec::with_capacity(rows);
        let mut y = 0.0;
        for r in 0..rows {
            y += extra_above[r];
            bands.push((y, y + height[r]));
            y += height[r] + extra_below[r] + gap[r];
        }
        for node in &mut self.nodes {
            let (top, bottom) = bands[node.rank as usize];
            node.y = (top + bottom) / 2.0;
        }
        for c in 1..self.clusters.len() {
            let cluster = &mut self.clusters[c];
            cluster.top = bands[cluster.r1 as usize].0 - above[c];
            cluster.bottom = bands[cluster.r2 as usize].1 + below[c];
        }
        bands
    }
}

/// The positions closest to `want` (by `weight`) that satisfy every
/// `(left, right, gap)`: right at least `gap` past left.
///
/// Variables are taken left to right. One that would sit too close to
/// something on its left is joined to it in a block that moves as one, to
/// the weighted mean of where its members want to be; the block may then be
/// too close to something further left, and joins that in turn.
fn project(
    cons: &[(usize, usize, f64)],
    ins: &[Vec<usize>],
    topo: &[usize],
    want: &[f64],
    weight: &[f64],
) -> Vec<f64> {
    struct Block {
        vars: Vec<usize>,
        /// Sum of weights, and of weight * (want - offset).
        w: f64,
        wd: f64,
        pos: f64,
    }
    let n = want.len();
    let mut block_of: Vec<usize> = (0..n).collect();
    let mut offset = vec![0.0f64; n];
    let mut blocks: Vec<Block> = (0..n)
        .map(|v| {
            let w = weight[v].max(1e-4);
            Block { vars: vec![v], w, wd: w * want[v], pos: want[v] }
        })
        .collect();

    for &v in topo {
        loop {
            let b = block_of[v];
            // The constraint into this block that is broken the most.
            let mut worst: Option<(f64, usize)> = None;
            for &var in &blocks[b].vars {
                for &c in &ins[var] {
                    let (l, r, gap) = cons[c];
                    let lb = block_of[l];
                    if lb == b {
                        continue;
                    }
                    let broken = blocks[lb].pos + offset[l] + gap - (blocks[b].pos + offset[r]);
                    if broken > 1e-7 && worst.is_none_or(|(most, _)| broken > most) {
                        worst = Some((broken, c));
                    }
                }
            }
            let Some((_, c)) = worst else { break };
            let (l, r, gap) = cons[c];
            let lb = block_of[l];
            // Move this block's members into the left one, placed so the
            // broken constraint just holds.
            let shift = offset[l] + gap - offset[r];
            let moved = std::mem::take(&mut blocks[b].vars);
            let (w, wd) = (blocks[b].w, blocks[b].wd);
            for &var in &moved {
                offset[var] += shift;
                block_of[var] = lb;
            }
            let left = &mut blocks[lb];
            left.vars.extend(moved);
            left.w += w;
            left.wd += wd - shift * w;
            left.pos = left.wd / left.w;
        }
    }
    (0..n).map(|v| blocks[block_of[v]].pos + offset[v]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxes(count: usize) -> Vec<InNode> {
        (0..count).map(|_| InNode { parent: None, group: false, w: 100.0, h: 60.0 }).collect()
    }

    fn line(source: usize, target: usize) -> InEdge {
        InEdge { source, target, label_w: 0.0, label_h: 0.0 }
    }

    fn overlap(a: &Rect, b: &Rect) -> bool {
        a.x < b.x + b.w - 0.01 && b.x < a.x + a.w - 0.01 && a.y < b.y + b.h - 0.01 && b.y < a.y + a.h - 0.01
    }

    fn inside(inner: &Rect, outer: &Rect) -> bool {
        inner.x >= outer.x - 0.01
            && inner.y >= outer.y - 0.01
            && inner.x + inner.w <= outer.x + outer.w + 0.01
            && inner.y + inner.h <= outer.y + outer.h + 0.01
    }

    #[test]
    fn a_chain_runs_straight_down() {
        let out = layout(&boxes(3), &[line(0, 1), line(1, 2)], Direction::TopBottom);
        let n = &out.nodes;
        assert!(n[0].y < n[1].y && n[1].y < n[2].y);
        assert!((n[0].x - n[1].x).abs() < 0.5 && (n[1].x - n[2].x).abs() < 0.5, "{n:?}");
        assert_eq!(n[1].y - (n[0].y + n[0].h), RANK_SEP);
        assert_eq!((n[0].x, n[0].y), (MARGIN, MARGIN));
        assert!((out.width - (100.0 + 2.0 * MARGIN)).abs() < 0.5);
        // Each line leaves the bottom of one box and arrives at the top of the next.
        let pts = &out.edges[0].points;
        assert_eq!(pts[0].1, n[0].y + n[0].h);
        assert_eq!(pts[pts.len() - 1].1, n[1].y);
    }

    #[test]
    fn a_parent_sits_over_its_children_and_directions_turn() {
        let edges = [line(0, 1), line(0, 2)];
        let out = layout(&boxes(3), &edges, Direction::TopBottom);
        let n = &out.nodes;
        assert_eq!(n[1].y, n[2].y);
        assert!(!overlap(&n[1], &n[2]));
        let middle = (n[1].x + n[2].x + n[2].w) / 2.0;
        assert!((n[0].x + n[0].w / 2.0 - middle).abs() < 1.0, "{n:?}");

        let out = layout(&boxes(3), &edges, Direction::LeftRight);
        let n = &out.nodes;
        assert!(n[0].x + n[0].w < n[1].x && n[1].x == n[2].x);
        assert_eq!((n[0].w, n[0].h), (100.0, 60.0));

        let out = layout(&boxes(3), &edges, Direction::BottomTop);
        assert!(out.nodes[0].y > out.nodes[1].y + out.nodes[1].h);
    }

    fn ranked(nodes: &[InNode], edges: &[InEdge], rank: Rank, named: &[usize]) -> Output {
        let ranks = [InRank { rank, nodes: named.to_vec() }];
        layout_spaced(nodes, edges, Direction::TopBottom, None, None, &ranks)
    }

    #[test]
    fn rank_same_puts_boxes_on_one_row() {
        // A chain, with its ends asked onto one row: the line that would
        // have put the last below is the one that goes back up.
        let edges = [line(0, 1), line(1, 2)];
        let out = ranked(&boxes(3), &edges, Rank::Same, &[0, 2]);
        let n = &out.nodes;
        assert_eq!(n[0].y, n[2].y);
        assert!(n[1].y > n[0].y + n[0].h && !overlap(&n[0], &n[2]), "{n:?}");
        let pts = &out.edges[1].points;
        assert!(pts[0].1 > pts[pts.len() - 1].1, "{pts:?}");
    }

    #[test]
    fn a_line_along_a_row_goes_round_under_it() {
        let mut edges = [line(0, 1)];
        edges[0].label_w = 40.0;
        edges[0].label_h = 14.0;
        let out = ranked(&boxes(2), &edges, Rank::Same, &[0, 1]);
        let n = &out.nodes;
        assert_eq!(n[0].y, n[1].y);
        assert!(!overlap(&n[0], &n[1]));
        // Out of the bottom of one and into the bottom of the other.
        let pts = &out.edges[0].points;
        let bottom = n[0].y + n[0].h;
        assert_eq!((pts[0].1, pts[pts.len() - 1].1), (bottom, bottom));
        assert!(n[0].x <= pts[0].0 && pts[0].0 <= n[0].x + n[0].w);
        assert!(n[1].x <= pts[pts.len() - 1].0 && pts[pts.len() - 1].0 <= n[1].x + n[1].w);
        assert!(pts.iter().all(|p| p.1 >= bottom));
        let label = out.edges[0].label.expect("its words have a place");
        assert!(label.y >= bottom && label.y + label.h <= out.height);
    }

    #[test]
    fn rank_min_and_max_are_the_first_row_and_the_last() {
        // 0 -> 1 -> 2, and 3 on its own.
        let edges = [line(0, 1), line(1, 2)];
        let out = ranked(&boxes(4), &edges, Rank::Min, &[2]);
        let n = &out.nodes;
        assert!(n[2].y < n[1].y && n[2].y <= n[0].y && n[2].y == n[3].y, "{n:?}");
        // Its line still ends at it.
        let pts = &out.edges[1].points;
        assert_eq!(pts[pts.len() - 1].1, n[2].y + n[2].h);

        let out = ranked(&boxes(4), &edges, Rank::Max, &[0, 3]);
        let n = &out.nodes;
        assert!(n[0].y == n[3].y && n[0].y > n[1].y && n[0].y >= n[2].y, "{n:?}");
    }

    #[test]
    fn rank_source_and_sink_have_their_row_to_themselves() {
        let edges = [line(0, 1), line(1, 2)];
        let out = ranked(&boxes(4), &edges, Rank::Source, &[3]);
        let n = &out.nodes;
        assert!((0..3).all(|i| n[3].y + n[3].h < n[i].y), "{n:?}");
        let out = ranked(&boxes(4), &edges, Rank::Sink, &[3]);
        let n = &out.nodes;
        assert!((0..3).all(|i| n[3].y > n[i].y + n[i].h), "{n:?}");
    }

    #[test]
    fn rank_keeps_groups_whole_and_ignores_what_it_cannot_use() {
        // A box beside the groups and one two rows down, asked onto one
        // row; the group and the repeat among the names count for nothing.
        let mut nodes = grouped();
        let count = nodes.len();
        nodes.extend(boxes(2));
        let members: Vec<usize> = (0..count).filter(|&i| !nodes[i].group).collect();
        let edges = [line(members[0], count), line(count, count + 1)];
        let out = ranked(&nodes, &edges, Rank::Same, &[members[0], count + 1, 1, 1]);
        let n = &out.nodes;
        assert_eq!(n[members[0]].y, n[count + 1].y);
        for (i, node) in nodes.iter().enumerate() {
            if let Some(parent) = node.parent {
                assert!(inside(&n[i], &n[parent]), "{i} in {parent}: {n:?}");
            }
        }
        // A rank naming only a group, or nothing, changes nothing.
        let plain = layout(&nodes, &edges, Direction::TopBottom);
        let group = (0..count).find(|&i| nodes[i].group).unwrap();
        for named in [vec![group], vec![]] {
            let out = ranked(&nodes, &edges, Rank::Same, &named);
            assert_eq!(out.nodes, plain.nodes);
        }
    }

    #[test]
    fn a_loop_is_drawn_with_one_line_going_back() {
        let out = layout(&boxes(3), &[line(0, 1), line(1, 2), line(2, 0)], Direction::TopBottom);
        let n = &out.nodes;
        assert!(n[0].y < n[1].y && n[1].y < n[2].y);
        // The line back still starts at its source and ends at its target.
        let pts = &out.edges[2].points;
        assert!(pts[0].1 > pts[pts.len() - 1].1);
        assert!(n.iter().all(|r| r.x >= MARGIN - 0.01));
    }

    #[test]
    fn a_label_gets_room_beside_its_line() {
        let edges = [InEdge { source: 0, target: 1, label_w: 80.0, label_h: 16.0 }, line(0, 2)];
        let out = layout(&boxes(3), &edges, Direction::TopBottom);
        let label = out.edges[0].label.expect("a label");
        assert_eq!((label.w, label.h), (80.0, 16.0));
        assert!(out.nodes.iter().all(|n| !overlap(n, &label)));
        assert!(label.y > out.nodes[0].y + out.nodes[0].h && label.y + label.h < out.nodes[1].y);
    }

    /// 0 outside; group 1 holding 2 and 3; group 4 holding 5; 6 outside.
    fn grouped() -> Vec<InNode> {
        let leaf = |parent| InNode { parent, group: false, w: 100.0, h: 60.0 };
        let group = |parent| InNode { parent, group: true, w: 120.0, h: 0.0 };
        vec![leaf(None), group(None), leaf(Some(1)), leaf(Some(1)), group(None), leaf(Some(4)), leaf(None)]
    }

    #[test]
    fn groups_hold_their_members_and_nothing_else() {
        let edges = [line(0, 2), line(2, 3), line(3, 5), line(0, 6), line(6, 5), line(2, 5)];
        for direction in
            [Direction::TopBottom, Direction::LeftRight, Direction::BottomTop, Direction::RightLeft]
        {
            let out = layout(&grouped(), &edges, direction);
            let n = &out.nodes;
            assert!(
                inside(&n[2], &n[1]) && inside(&n[3], &n[1]) && inside(&n[5], &n[4]),
                "{direction:?} {n:?}"
            );
            for outsider in [0, 6] {
                assert!(
                    !overlap(&n[outsider], &n[1]) && !overlap(&n[outsider], &n[4]),
                    "{direction:?} {n:?}"
                );
            }
            assert!(!overlap(&n[1], &n[4]), "{direction:?} {n:?}");
            for a in [0, 2, 3, 5, 6] {
                for b in [0, 2, 3, 5, 6] {
                    assert!(a == b || !overlap(&n[a], &n[b]), "{direction:?} {a} {b} {n:?}");
                }
            }
            let all = Rect { x: 0.0, y: 0.0, w: out.width, h: out.height };
            assert!(n.iter().all(|r| inside(r, &all)));
        }
    }

    #[test]
    fn a_line_to_a_group_stops_at_its_border() {
        let out = layout(&grouped(), &[line(0, 1), line(2, 3), line(1, 6)], Direction::TopBottom);
        let n = &out.nodes;
        let arriving = out.edges[0].points.last().copied().unwrap();
        assert!((arriving.1 - n[1].y).abs() < 0.01, "{arriving:?} {:?}", n[1]);
        assert!(arriving.0 >= n[1].x && arriving.0 <= n[1].x + n[1].w);
        let leaving = out.edges[2].points[0];
        assert!((leaving.1 - (n[1].y + n[1].h)).abs() < 0.01);
        assert!(n[0].y + n[0].h < n[1].y && n[1].y + n[1].h < n[6].y);
    }

    #[test]
    fn a_group_is_as_wide_as_its_title_needs() {
        let mut nodes = grouped();
        nodes[4].w = 400.0;
        let out = layout(&nodes, &[line(0, 5)], Direction::TopBottom);
        assert!(out.nodes[4].w >= 400.0);
        assert!(inside(&out.nodes[5], &out.nodes[4]));
    }

    #[test]
    fn nothing_at_all_and_nothing_connected() {
        let out = layout(&[], &[], Direction::TopBottom);
        assert_eq!((out.width, out.height), (2.0 * MARGIN, 2.0 * MARGIN));
        let out = layout(&boxes(3), &[], Direction::TopBottom);
        let n = &out.nodes;
        assert!(n[0].y == n[1].y && n[0].x + n[0].w + NODE_SEP == n[1].x, "{n:?}");
    }

    #[test]
    fn many_boxes_do_not_overlap() {
        // A fixed tangle: 24 boxes in 4 groups, lines by arithmetic.
        let mut nodes: Vec<InNode> =
            (0..4).map(|_| InNode { parent: None, group: true, w: 80.0, h: 0.0 }).collect();
        for i in 0..24 {
            let parent = if i % 5 == 0 { None } else { Some(i % 4) };
            nodes.push(InNode {
                parent,
                group: false,
                w: 90.0 + (i * 7 % 40) as f64,
                h: 50.0 + (i * 3 % 30) as f64,
            });
        }
        let mut edges = Vec::new();
        for i in 0..24usize {
            for step in [3usize, 7, 11] {
                let j = (i * step + 5) % 24;
                if i != j && (i + step) % 3 != 0 {
                    edges.push(InEdge { source: 4 + i, target: 4 + j, label_w: 40.0, label_h: 14.0 });
                }
            }
        }
        edges.push(line(0, 1));
        edges.push(line(4, 2));
        for direction in [Direction::TopBottom, Direction::LeftRight] {
            let out = layout(&nodes, &edges, direction);
            let n = &out.nodes;
            for a in 4..n.len() {
                for b in a + 1..n.len() {
                    assert!(!overlap(&n[a], &n[b]), "{a} {b}");
                }
                for group in 0..4 {
                    let member = nodes[a].parent == Some(group);
                    assert_eq!(inside(&n[a], &n[group]), member, "{a} in {group}");
                    assert!(member || !overlap(&n[a], &n[group]), "{a} over {group}");
                }
            }
            for a in 0..4 {
                for b in a + 1..4 {
                    assert!(!overlap(&n[a], &n[b]), "groups {a} {b}");
                }
            }
            assert!(out.edges.iter().all(|e| e.points.len() >= 2));
            // No line goes through a box it is not a line of, and no
            // line's words lie over a box or over another line's.
            let within = |r: &Rect| [r.x + 1.0, r.y + 1.0, r.x + r.w - 1.0, r.y + r.h - 1.0];
            for (e, route) in out.edges.iter().enumerate() {
                for other in (4..n.len()).filter(|&i| i != edges[e].source && i != edges[e].target) {
                    for pair in route.points.windows(2) {
                        assert!(!crosses(pair[0], pair[1], within(&n[other])), "line {e} through {other}");
                    }
                }
                let Some(label) = &route.label else { continue };
                assert!((4..n.len()).all(|i| !overlap(label, &n[i])), "words of {e} over a box");
                for later in &out.edges[e + 1..] {
                    assert!(later.label.is_none_or(|l| !overlap(label, &l)), "words of {e} over words");
                }
            }
        }
    }

    #[test]
    fn a_long_line_is_straight_where_nothing_is_in_its_way() {
        // Down a chain of four, and from the first straight to the last,
        // past the two between.
        let edges = [line(0, 1), line(1, 2), line(2, 3), line(0, 3)];
        let out = layout(&boxes(4), &edges, Direction::TopBottom);
        let n = &out.nodes;
        let pts = &out.edges[3].points;
        let (first, last) = (pts[0], pts[pts.len() - 1]);
        assert_eq!((first.1, last.1), (n[0].y + n[0].h, n[3].y));
        // Not through the boxes it passes, nor nearer them than it may be.
        for passed in [&n[1], &n[2]] {
            let near = [
                passed.x - CLEAR_BOX + 1.0,
                passed.y - CLEAR_BOX + 1.0,
                passed.x + passed.w + CLEAR_BOX - 1.0,
                passed.y + passed.h + CLEAR_BOX - 1.0,
            ];
            assert!(pts.windows(2).all(|w| !crosses(w[0], w[1], near)), "{pts:?}");
        }
        // And little longer than the straight line between its ends.
        let length: f64 =
            pts.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum();
        let direct = ((last.0 - first.0).powi(2) + (last.1 - first.1).powi(2)).sqrt();
        assert!(length < direct * 1.08, "{length} for {direct}: {pts:?}");
        // It turns only to leave one box and to arrive at the other: no
        // corner between is a turn back the other way.
        let turns = pts
            .windows(3)
            .filter(|w| {
                let cross = (w[1].0 - w[0].0) * (w[2].1 - w[1].1) - (w[1].1 - w[0].1) * (w[2].0 - w[1].0);
                cross.abs() > 1e-6
            })
            .count();
        assert!(turns <= 4, "{turns} turns: {pts:?}");
    }

    #[test]
    fn crossing_a_rectangle_and_rounding_a_corner() {
        let r = [10.0, 10.0, 20.0, 20.0];
        assert!(crosses((0.0, 0.0), (30.0, 30.0), r));
        assert!(crosses((15.0, 0.0), (15.0, 12.0), r));
        assert!(!crosses((0.0, 0.0), (9.0, 30.0), r));
        assert!(!crosses((0.0, 25.0), (30.0, 25.0), r));
        assert!(!crosses((0.0, 0.0), (5.0, 5.0), r));
        // A long straight gets a corner near each end; a short one is left.
        let path = cornered(&[(0.0, 0.0), (0.0, 100.0), (30.0, 100.0)]);
        assert_eq!(path, [(0.0, 0.0), (0.0, CORNER), (0.0, 100.0 - CORNER), (0.0, 100.0), (30.0, 100.0)]);
    }
}
