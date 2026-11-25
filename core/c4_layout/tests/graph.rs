use super::*;

fn layout(nodes: &[InNode], edges: &[InEdge], direction: Direction) -> Output {
    layout_spaced(nodes, edges, direction, None, None, &[])
}

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
    for direction in [Direction::TopBottom, Direction::LeftRight, Direction::BottomTop, Direction::RightLeft]
    {
        let out = layout(&grouped(), &edges, direction);
        let n = &out.nodes;
        assert!(inside(&n[2], &n[1]) && inside(&n[3], &n[1]) && inside(&n[5], &n[4]), "{direction:?} {n:?}");
        for outsider in [0, 6] {
            assert!(!overlap(&n[outsider], &n[1]) && !overlap(&n[outsider], &n[4]), "{direction:?} {n:?}");
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
