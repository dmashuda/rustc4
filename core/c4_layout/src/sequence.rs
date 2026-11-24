//! A dynamic view as a sequence: who takes part across the top, a line
//! down from each, and the steps one under another in the order they
//! happen.

use crate::graph::{InEdge, InNode, Output, Rect, Route};

const MARGIN: f64 = 24.0;
/// Between two neighbours' boxes, at least.
const GAP: f64 = 48.0;
/// From the boxes down to the first step, and after the last.
const LEAD: f64 = 30.0;
/// Between one step's line and the next step's words.
const STEP_GAP: f64 = 22.0;
/// Words sit this far above their line, and stay this clear of the lines
/// down at either end.
const LABEL_GAP: f64 = 6.0;
const LABEL_PAD: f64 = 18.0;
/// A step one takes alone: how far its loop reaches out, and how tall it
/// is at the least.
const LOOP_W: f64 = 30.0;
const LOOP_H: f64 = 22.0;

/// Where each participant's line runs: its x, and from which y to which.
pub(crate) struct Lifeline {
    pub x: f64,
    pub top: f64,
    pub bottom: f64,
}

/// `nodes` are the participants, left to right; `edges` the steps, in order.
pub(crate) fn layout(nodes: &[InNode], edges: &[InEdge]) -> (Output, Vec<Lifeline>) {
    // Each one's middle: far enough from its neighbour for both boxes...
    let mut middle: Vec<f64> = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        middle.push(match i {
            0 => MARGIN + node.w / 2.0,
            _ => middle[i - 1] + (nodes[i - 1].w + node.w) / 2.0 + GAP,
        });
    }
    // ...and for the words of every step between the two, nearest first.
    // A loop's words are beside it, before the next one's line down (or
    // the edge of the drawing, after the last).
    let looped = |e: &InEdge| LOOP_W + LABEL_GAP + e.label_w + LABEL_PAD;
    let mut by_span: Vec<(usize, usize, f64)> = edges
        .iter()
        .filter(|e| e.source != e.target || e.source + 1 < nodes.len())
        .map(|e| match e.source == e.target {
            true => (e.source, e.source + 1, looped(e)),
            false => (e.source.min(e.target), e.source.max(e.target), e.label_w + 2.0 * LABEL_PAD),
        })
        .collect();
    by_span.sort_by_key(|&(lo, hi, _)| hi - lo);
    for (lo, hi, room) in by_span {
        let short = room - (middle[hi] - middle[lo]);
        if short > 0.0 {
            middle[hi..].iter_mut().for_each(|x| *x += short);
        }
    }
    let beyond = edges
        .iter()
        .filter(|e| e.source == e.target && e.source + 1 == nodes.len())
        .map(|e| middle[e.source] + looped(e) - LABEL_PAD)
        .fold(0.0, f64::max);

    // Boxes stand on one line, so every line down starts level.
    let tallest = nodes.iter().map(|n| n.h).fold(0.0, f64::max);
    let floor = MARGIN + tallest;
    let rects: Vec<Rect> = nodes
        .iter()
        .zip(&middle)
        .map(|(n, &x)| Rect { x: x - n.w / 2.0, y: floor - n.h, w: n.w, h: n.h })
        .collect();

    let mut y = floor + LEAD - STEP_GAP;
    let mut routes = Vec::with_capacity(edges.len());
    for edge in edges {
        if edge.source == edge.target {
            // Out to the side, down, and back: its words beside it.
            let x = middle[edge.source];
            let top = y + STEP_GAP + LABEL_GAP;
            let h = edge.label_h.max(LOOP_H);
            let label = (edge.label_w > 0.0).then(|| Rect {
                x: x + LOOP_W + LABEL_GAP,
                y: top + (h - edge.label_h) / 2.0,
                w: edge.label_w,
                h: edge.label_h,
            });
            let points = vec![(x, top), (x + LOOP_W, top), (x + LOOP_W, top + h), (x, top + h)];
            routes.push(Route { points, label });
            y = top + h;
            continue;
        }
        y += STEP_GAP + edge.label_h + LABEL_GAP;
        let (from, to) = (middle[edge.source], middle[edge.target]);
        let label = (edge.label_w > 0.0).then(|| Rect {
            x: (from + to) / 2.0 - edge.label_w / 2.0,
            y: y - LABEL_GAP - edge.label_h,
            w: edge.label_w,
            h: edge.label_h,
        });
        routes.push(Route { points: vec![(from, y), (to, y)], label });
    }
    let bottom = y + LEAD;

    let lifelines = middle.iter().map(|&x| Lifeline { x, top: floor, bottom }).collect();
    let width = rects.last().map_or(0.0, |r| r.x + r.w).max(beyond) + MARGIN;
    (Output { nodes: rects, edges: routes, width, height: bottom + MARGIN }, lifelines)
}
