use super::*;
use c4_model::{Source, load};

const SHOP: &str = "
        specification {
            element actor { style { shape person; color green } }
            element system
            element component
            color brand #ffee00
        }
        model {
            customer = actor 'Customer' 'Someone who buys things from the shop, now and then'
            shop = system 'Shop' {
                ui = component 'Web app' 'What the customer sees' 'React' { style { shape browser; color brand } }
                api = component 'API'
                db = component 'Database' { style { shape storage } }
                ui -> api 'calls' { technology 'HTTPS' }
                api -> db 'reads and writes'
            }
            customer -> shop.ui 'browses'
            customer -> shop 'buys from' { head diamond; tail odot; color red }
        }
        views {
            view index { include * }
            view shop of shop { include * }
            view across of shop { include *; autoLayout LeftRight }
        }";

fn diagram(view: &str) -> Diagram {
    let ws = load(&[Source { name: "shop.c4".into(), text: SHOP.into() }]);
    assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
    layout(&ws.model.compute_view(view).unwrap(), &ws.model.colors)
}

#[test]
fn boxes_fit_their_text() {
    let d = diagram("index");
    assert_eq!(d.nodes.len(), 2);
    let customer = &d.nodes[0];
    assert_eq!(customer.shape, Shape::Person);
    assert_eq!(customer.fill, "#428a4f");
    let texts: Vec<_> = customer.labels.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["Customer", "Someone who buys things from the", "shop, now and then"]);
    for label in &customer.labels {
        let half = text::width(&label.text, label.size, label.bold) / 2.0;
        assert!(label.x - half >= customer.x && label.x + half <= customer.x + customer.width);
        assert!(label.y > customer.y + 34.0 && label.y < customer.y + customer.height);
    }
    assert!(customer.labels[0].bold && customer.labels[0].role == TextRole::Title);
    assert_eq!(d.nodes[1].navigate_to.as_deref(), Some("shop"));
    assert!(d.width > customer.width && d.height > customer.height * 2.0);
}

#[test]
fn a_group_is_a_tinted_titled_border() {
    let d = diagram("shop");
    let shop = d.nodes.iter().find(|n| n.id == "shop").unwrap();
    assert!(shop.group && shop.fill_opacity < 0.2 && shop.depth == 0);
    assert_eq!(shop.labels[0].role, TextRole::GroupTitle);
    let ui = d.nodes.iter().find(|n| n.id == "shop.ui").unwrap();
    assert_eq!((ui.depth, ui.fill.as_str(), ui.shape), (1, "#ffee00", Shape::Browser));
    assert!(ui.x > shop.x && ui.y > shop.labels[0].y && ui.x + ui.width < shop.x + shop.width);
    let texts: Vec<_> = ui.labels.iter().map(|l| (l.text.as_str(), l.role)).collect();
    assert_eq!(
        texts,
        [
            ("Web app", TextRole::Title),
            ("React", TextRole::Technology),
            ("What the customer sees", TextRole::Description)
        ]
    );
}

#[test]
fn lines_have_curves_marks_and_labels() {
    let d = diagram("shop");
    let calls = d.edges.iter().find(|e| e.id == "shop.ui->shop.api").unwrap();
    assert_eq!(calls.path.len() % 3, 1);
    let texts: Vec<_> = calls.labels.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["calls", "[HTTPS]"]);
    assert_eq!(calls.labels[0].anchor, Anchor::Start);
    assert_eq!(calls.color, theme::LINE_GRAY);
    // One arrowhead, its tip on the top of the box it points at.
    assert_eq!(calls.markers.len(), 1);
    let api = d.nodes.iter().find(|n| n.id == "shop.api").unwrap();
    let tip = calls.markers[0].points[0];
    assert_eq!(tip.y, api.y);
    assert!(tip.x > api.x && tip.x < api.x + api.width);
    assert_eq!(calls.path.last().unwrap().y, api.y - 9.0);

    let buys = d.edges.iter().find(|e| e.id == "customer->shop").unwrap();
    assert_eq!(buys.color, "#ac4d39");
    assert_eq!(buys.markers.len(), 2);
    assert!(buys.markers[0].radius > 0.0 && !buys.markers[0].filled, "the tail: an open dot");
    assert_eq!(buys.markers[1].points.len(), 4);

    let across = diagram("across");
    let calls = across.edges.iter().find(|e| e.id == "shop.ui->shop.api").unwrap();
    assert_eq!(calls.labels[0].anchor, Anchor::Middle);
    let (ui, api) = (&across.nodes[2], &across.nodes[3]);
    assert!(ui.x + ui.width < api.x);
}

#[test]
fn the_same_source_gives_the_same_diagram() {
    assert_eq!(diagram("shop"), diagram("shop"));
    let json = |d: &Diagram| format!("{:?}", d);
    assert_eq!(json(&diagram("across")), json(&diagram("across")));
}

#[test]
fn a_smooth_line_starts_and_ends_where_it_should() {
    let path = smooth(&[(0.0, 0.0), (0.0, 30.0), (50.0, 60.0), (50.0, 90.0)]);
    assert_eq!(path.len(), 1 + 3 * 5);
    assert_eq!((path[0].x, path[0].y), (0.0, 0.0));
    assert_eq!((path[15].x, path[15].y), (50.0, 90.0));
    // Leaves straight down.
    assert!(path[1].x == 0.0 && path[2].x == 0.0);
    assert!(smooth(&[]).is_empty());
}

#[test]
fn a_sequence_puts_each_step_under_the_last() {
    let source = SHOP.replace(
        "view index { include * }",
        "view index { include * }
            dynamic view buying {
                variant sequence
                include shop
                customer -> shop.ui 'opens the shop'
                shop.ui -> shop.api
                shop.api -> shop.db 'looks up what is for sale, which takes a while'
                customer <- shop.ui 'shows it'
            }",
    );
    let ws = load(&[Source { name: "shop.c4".into(), text: source }]);
    assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
    let d = layout(&ws.model.compute_view("buying").unwrap(), &ws.model.colors);
    // Who takes part, in the order they come into it; no group.
    let ids: Vec<&str> = d.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, ["customer", "shop.ui", "shop.api", "shop.db"]);
    assert!(d.nodes.iter().all(|n| !n.group && n.parent.is_none()));
    // Side by side, standing on one line, a line down from the middle of each.
    assert_eq!(d.lifelines.len(), 4);
    for (pair, line) in d.nodes.windows(2).zip(&d.lifelines) {
        assert!(pair[0].x + pair[0].width < pair[1].x);
        assert_eq!(pair[0].y + pair[0].height, pair[1].y + pair[1].height);
        assert_eq!(line.from.x, pair[0].x + pair[0].width / 2.0);
        assert_eq!(line.from.y, pair[0].y + pair[0].height);
        assert!(line.to.y < d.height);
    }
    // Each step level, from one line down to another, under the one before.
    let mut last = d.lifelines[0].from.y;
    for edge in &d.edges {
        let (start, end) = (edge.path[0], *edge.path.last().unwrap());
        assert_eq!(start.y, end.y);
        assert!(start.y > last && start.y < d.lifelines[0].to.y);
        last = start.y;
        let x_of = |id: &str| d.lifelines.iter().find(|l| l.node == id).unwrap().from.x;
        assert_eq!(start.x, x_of(&edge.source));
        // Its words fit between the two, above the line.
        let (lo, hi) =
            (x_of(&edge.source).min(x_of(&edge.target)), x_of(&edge.source).max(x_of(&edge.target)));
        for label in &edge.labels {
            let half = text::width(&label.text, label.size, label.bold) / 2.0;
            assert!(label.x - half > lo && label.x + half < hi && label.y < start.y, "{label:?}");
        }
    }
    assert_eq!(d.edges[3].title, "4. shows it");
    assert!(d.edges[3].path[0].x > d.edges[3].path.last().unwrap().x, "an answer goes back");
    // Any other view has no lines down.
    assert!(diagram("index").lifelines.is_empty());
}

#[test]
fn a_step_it_takes_alone_is_a_loop_beside_it() {
    for variant in ["variant sequence", "autoLayout TopBottom", "autoLayout LeftRight"] {
        let source = SHOP.replace(
            "view index { include * }",
            &format!(
                "view index {{ include * }}
                    dynamic view buying {{
                        {variant}
                        customer -> shop.ui 'opens the shop'
                        shop.ui -> shop.api
                        shop.api -> shop.api 'checks the stock'
                        shop.api -> shop.db
                        shop.db -> shop.db 'locks a row'
                    }}"
            ),
        );
        let ws = load(&[Source { name: "shop.c4".into(), text: source }]);
        assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
        let d = layout(&ws.model.compute_view("buying").unwrap(), &ws.model.colors);
        for (step, on) in [("shop.api->shop.api#3", "shop.api"), ("shop.db->shop.db#5", "shop.db")] {
            let edge = d.edges.iter().find(|e| e.id == step).expect(step);
            let start = edge.path[0];
            // Out of one side of it and back to the same side.
            let at = |p: Point| match d.lifelines.iter().find(|l| l.node == on) {
                Some(line) => p.x == line.from.x,
                None => {
                    let n = d.nodes.iter().find(|n| n.id == on).unwrap();
                    let side = if variant.ends_with("TopBottom") {
                        p.x == n.x + n.width
                    } else {
                        p.y == n.y + n.height
                    };
                    side && n.x <= p.x && p.x <= n.x + n.width && n.y <= p.y && p.y <= n.y + n.height
                }
            };
            assert!(at(start) && at(edge.markers[0].points[0]), "{variant}: {step} {edge:?}");
            // Its words beside it (below, when lines go across), clear of every
            // box and inside the drawing.
            let label = &edge.labels[0];
            let half = text::width(&label.text, label.size, label.bold) / 2.0;
            let (left, right) = match label.anchor {
                Anchor::Start => (label.x, label.x + 2.0 * half),
                _ => (label.x - half, label.x + half),
            };
            let beside = match variant.ends_with("LeftRight") {
                true => label.y - label.size > start.y,
                false => left > start.x,
            };
            assert!(beside && right < d.width && label.y < d.height, "{variant}: {label:?}");
            for n in &d.nodes {
                let overlaps = left < n.x + n.width
                    && right > n.x
                    && label.y > n.y
                    && label.y - label.size < n.y + n.height;
                assert!(!overlaps, "{variant}: {label:?} over {}", n.id);
            }
        }
    }
}

fn drawn(source: &str) -> Diagram {
    let ws = load(&[Source { name: "t.c4".into(), text: source.into() }]);
    assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
    layout(&ws.model.compute_view("index").unwrap(), &ws.model.colors)
}

#[test]
fn size_padding_and_text_size_change_the_box() {
    let d = drawn(
        "specification { element box }
            model {
                plain = box 'Orders' 'Takes an order from basket to doorstep' 'Rust'
                big = box 'Orders' 'Takes an order from basket to doorstep' 'Rust' { style { size xl } }
                tiny = box 'Orders' 'Takes an order from basket to doorstep' 'Rust' { style { size xs } }
                roomy = box 'Orders' 'Takes an order from basket to doorstep' 'Rust' { style { padding xl } }
                loud = box 'Orders' 'Takes an order from basket to doorstep' 'Rust' { style { textSize xl } }
            }
            views { view index { include * } }",
    );
    let node = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
    let (plain, big, tiny, roomy, loud) =
        (node("plain"), node("big"), node("tiny"), node("roomy"), node("loud"));
    assert!(big.width > plain.width && big.height > plain.height);
    assert!(tiny.width < plain.width && tiny.height < plain.height);
    // The smallest says only its title.
    assert_eq!(tiny.labels.len(), 1);
    assert_eq!(tiny.labels[0].role, TextRole::Title);
    // The same words, the same lines, more room around them.
    assert_eq!(roomy.labels.len(), plain.labels.len());
    assert!(roomy.width > plain.width && roomy.height > plain.height);
    assert_eq!(plain.labels[0].size, TITLE_SIZE);
    assert_eq!(loud.labels[0].size, 21.75);
    assert!(loud.height > plain.height);
    // Text stays inside its box, whatever its size.
    for n in &d.nodes {
        for label in &n.labels {
            let half = text::width(&label.text, label.size, label.bold) / 2.0;
            assert!(label.x - half >= n.x && label.x + half <= n.x + n.width, "{} in {}", label.text, n.id);
            assert!(label.y > n.y && label.y < n.y + n.height);
        }
    }
}

#[test]
fn a_multiple_is_a_stack_with_room_behind_it() {
    let d = drawn(
        "specification { element box }
            model {
                one = box 'One'
                many = box 'Many' { style { multiple true } }
                next = box 'Next'
                one -> many
            }
            views { view index { include *; autoLayout LeftRight } }",
    );
    let node = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
    let (one, many, next) = (node("one"), node("many"), node("next"));
    assert!(one.behind.is_empty());
    // The same size in front; the one behind shows past two edges.
    assert_eq!((many.width, many.height), (one.width, one.height));
    assert_eq!(many.behind, [Point { x: many.x + STACK, y: many.y + STACK }]);
    // Nothing is laid over the one behind.
    for other in [one, next] {
        let apart = other.x >= many.x + many.width + STACK
            || other.x + other.width <= many.x
            || other.y >= many.y + many.height + STACK
            || other.y + other.height <= many.y;
        assert!(apart, "{} over the stack", other.id);
    }
}

#[test]
fn an_icon_has_its_place_beside_the_words() {
    let d = drawn(
            "specification { element box; element group }
            model {
                none = box 'Orders' 'Takes them'
                left = box 'Orders' 'Takes them' { icon https://example.com/i.svg }
                right = box 'Orders' 'Takes them' { icon https://example.com/i.svg; style { iconPosition right } }
                top = box 'Orders' 'Takes them' { icon https://example.com/i.svg; style { iconPosition top } }
                bottom = box 'Orders' 'Takes them' { icon https://example.com/i.svg; style { iconPosition bottom; iconSize xl } }
                outer = group 'Outer' { icon https://example.com/i.svg; inner = box 'Inner' }
            }
            views { view index { include *, outer.* } }",
        );
    let node = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
    assert!(node("none").icon.is_none());
    for id in ["left", "right", "top", "bottom"] {
        let n = node(id);
        let icon = n.icon.as_ref().unwrap();
        assert_eq!(icon.href, "https://example.com/i.svg");
        assert_eq!(icon.width, if id == "bottom" { 64.0 } else { 36.0 });
        // Inside the box, and clear of every line of its words.
        assert!(icon.x >= n.x && icon.x + icon.width <= n.x + n.width, "{id}");
        assert!(icon.y >= n.y && icon.y + icon.height <= n.y + n.height, "{id}");
        for label in &n.labels {
            let half = text::width(&label.text, label.size, label.bold) / 2.0;
            let (l, r) = (label.x - half, label.x + half);
            let (t, b) = (label.y - label.size, label.y);
            let clear = r <= icon.x || l >= icon.x + icon.width || b <= icon.y || t >= icon.y + icon.height;
            assert!(clear, "{} under the icon of {id}", label.text);
            assert!(l >= n.x && r <= n.x + n.width, "{} out of {id}", label.text);
        }
    }
    let words = |id: &str| node(id).labels[0].clone();
    let icon = |id: &str| node(id).icon.clone().unwrap();
    assert!(icon("left").x < words("left").x && icon("right").x > words("right").x);
    assert!(icon("top").y < words("top").y && icon("bottom").y > words("bottom").y);
    // A group's is before its title.
    let (outer, mark) = (node("outer"), icon("outer"));
    assert_eq!((mark.x, mark.width), (outer.x + 16.0, 16.0));
    assert_eq!(outer.labels[0].x, mark.x + 22.0);
}

#[test]
fn auto_layout_can_say_how_far_apart() {
    let source = |layout: &str| {
        format!(
            "specification {{ element box }}
                model {{ a = box; b = box; c = box; a -> b; a -> c }}
                views {{ view index {{ include *; {layout} }} }}"
        )
    };
    let d = drawn(&source("autoLayout TopBottom 200 10"));
    let node = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
    let (a, b, c) = (node("a"), node("b"), node("c"));
    assert_eq!(b.y - (a.y + a.height), 200.0);
    assert_eq!(c.x - (b.x + b.width), 10.0);
    // Turned: rows are still what the first number is between.
    let d = drawn(&source("autoLayout LeftRight 200 10"));
    let node = |id: &str| d.nodes.iter().find(|n| n.id == id).unwrap();
    let (a, b, c) = (node("a"), node("b"), node("c"));
    assert_eq!(b.x - (a.x + a.width), 200.0);
    assert_eq!(c.y - (b.y + b.height), 10.0);
}

const NOTED: &str = "specification {
                element person { notation 'Someone' style { shape person; color green } }
                element service { notation 'A service we run, day and night, whatever happens' }
                element store { notation 'Data' style { shape storage } }
                element plain
                relationship async { notation 'Does not wait' line dotted; head open }
                relationship sync
            }
            model {
                user = person 'User'
                api = service 'API'
                jobs = service 'Jobs'
                odd = service 'Odd' { notation 'An odd one' }
                db = store 'DB'
                other = plain 'Other'
                user -> api
                api -[async]-> jobs
                api -[async]-> odd
                api -[sync]-> db
                jobs -> other
            }
            views {
                view index { include * }
                view quiet { include other, db with { notation '' } }
                view marked { include api, jobs with { notation 'Marked'; color red } }
            }";

fn noted(view: &str, legend: bool) -> Diagram {
    let ws = load(&[Source { name: "t.c4".into(), text: NOTED.into() }]);
    assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
    let view = ws.model.compute_view(view).unwrap();
    assert_eq!(has_legend(&view), view.id != "quiet");
    layout_with(&view, &ws.model.colors, Options { legend })
}

#[test]
fn a_legend_has_one_entry_for_each_look_with_a_notation() {
    let without = noted("index", false);
    assert!(without.legend.is_none());
    let with = noted("index", true);
    // There unless it is turned off.
    let ws = load(&[Source { name: "t.c4".into(), text: NOTED.into() }]);
    assert_eq!(layout(&ws.model.compute_view("index").unwrap(), &ws.model.colors), with);
    let legend = with.legend.as_ref().unwrap();
    // The legend is first, the drawing itself as it was, under it.
    assert_eq!(legend.y, LEGEND_MARGIN);
    let down = legend.y + legend.height;
    assert_eq!(with.height, without.height + down);
    assert!(with.width >= without.width);
    for (moved, was) in with.nodes.iter().zip(&without.nodes) {
        assert_eq!((moved.x, moved.y), (was.x, was.y + down));
        assert_eq!(moved.labels[0].y, was.labels[0].y + down);
    }
    for (moved, was) in with.edges.iter().zip(&without.edges) {
        assert_eq!(moved.path[0], Point { x: was.path[0].x, y: was.path[0].y + down });
        assert_eq!(moved.markers[0].points[0].y, was.markers[0].points[0].y + down);
    }
    assert!(legend.x >= 0.0 && legend.x + legend.width <= with.width);
    assert!(with.nodes.iter().all(|n| n.y >= down));

    // Two services look the same and say the same: one entry. `plain`
    // and `sync` say nothing: none.
    let said: Vec<&str> = legend.nodes.iter().map(|n| n.title.as_str()).collect();
    assert_eq!(said, ["Someone", "A service we run, day and night, whatever happens", "An odd one", "Data"]);
    assert_eq!(legend.nodes[0].shape, Shape::Person);
    assert_eq!(legend.nodes[3].shape, Shape::Storage);
    let lines: Vec<&str> = legend.edges.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(lines, ["Does not wait"]);
    assert_eq!((legend.edges[0].line, legend.edges[0].markers.len()), (LineStyle::Dotted, 1));

    // A sample is nothing to tap, and says nothing itself.
    assert!(legend.nodes.iter().all(|n| n.id.is_empty() && n.labels.is_empty()));
    assert_eq!(legend.labels[0].text, "Legend");
    // A long notation breaks; every line stays inside the frame.
    let texts: Vec<&str> = legend.labels.iter().map(|l| l.text.as_str()).collect();
    assert!(texts.contains(&"A service we run, day and"), "{texts:?}");
    for label in &legend.labels {
        assert_eq!(label.role, TextRole::Legend);
        let right = label.x + text::width(&label.text, label.size, label.bold);
        assert!(label.x >= legend.x && right <= legend.x + legend.width, "{}", label.text);
        assert!(label.y > legend.y && label.y < legend.y + legend.height, "{}", label.text);
    }
    // No two samples share a place.
    let places: Vec<(f64, f64)> = legend
        .nodes
        .iter()
        .map(|n| (n.x, n.y))
        .chain(legend.edges.iter().map(|e| (e.path.last().unwrap().x.floor(), e.path[0].y)))
        .collect();
    for (i, a) in places.iter().enumerate() {
        assert!(places[i + 1..].iter().all(|b| (a.0 - b.0).abs() > 50.0 || (a.1 - b.1).abs() > 20.0));
    }
}

#[test]
fn a_legend_is_as_wide_as_the_drawing_in_even_rows() {
    // So many kinds, one box of each, side by side; how many are on
    // each row of the legend.
    let rows = |kinds: usize| -> (Diagram, Vec<usize>) {
        let mut spec = String::new();
        let mut model = String::new();
        for k in 0..kinds {
            spec += &format!("element k{k} {{ notation 'Kind {k}' }}\n");
            model += &format!("e{k} = k{k}\n");
        }
        let source =
            format!("specification {{ {spec} }} model {{ {model} }} views {{ view index {{ include * }} }}");
        let ws = load(&[Source { name: "t.c4".into(), text: source }]);
        let view = ws.model.compute_view("index").unwrap();
        let d = layout(&view, &ws.model.colors);
        let legend = d.legend.clone().unwrap();
        assert_eq!(legend.nodes.len(), kinds);
        let mut tops: Vec<f64> = legend.nodes.iter().map(|n| n.y).collect();
        tops.dedup();
        let counts = tops.iter().map(|&y| legend.nodes.iter().filter(|n| n.y == y).count()).collect();
        (d, counts)
    };
    // Nine would fit in one row under nine boxes: five and four.
    let (d, counts) = rows(9);
    assert_eq!(counts, [5, 4]);
    let legend = d.legend.unwrap();
    assert_eq!((legend.x, legend.x + legend.width), (LEGEND_MARGIN, d.width - LEGEND_MARGIN));
    // The second row stands under the first, the columns evenly apart.
    let xs: Vec<f64> = legend.nodes.iter().map(|n| n.x).collect();
    assert_eq!(xs[5..], xs[..4]);
    let apart = xs[1] - xs[0];
    assert!(xs[..5].windows(2).all(|w| (w[1] - w[0] - apart).abs() < 0.02), "{xs:?}");
    assert!(apart > SAMPLE_W + SAMPLE_GAP + ENTRY_GAP_X);
    assert_eq!(rows(5).1, [5]);
    assert_eq!(rows(6).1, [3, 3]);
    assert_eq!(rows(11).1, [4, 4, 3]);
}

#[test]
fn a_legend_follows_what_the_view_shows() {
    // Nothing with a notation: no legend, though one was asked for.
    let quiet = noted("quiet", true);
    assert!(quiet.legend.is_none());
    assert_eq!(quiet.height, noted("quiet", false).height);
    // A look the view gives is a look of its own, with its own words.
    let marked = noted("marked", true);
    let legend = marked.legend.unwrap();
    let looks: Vec<(&str, &str)> = legend.nodes.iter().map(|n| (n.title.as_str(), n.fill.as_str())).collect();
    assert_eq!(
        looks,
        [("A service we run, day and night, whatever happens", "#3b82f6"), ("Marked", "#ac4d39")]
    );
    // Narrow drawing, so one under the other; and the drawing is made
    // wide enough for the legend.
    assert_eq!(legend.nodes[0].x, legend.nodes[1].x);
    assert!(legend.nodes[1].y > legend.nodes[0].y);
    assert!(marked.width >= legend.x + legend.width);
}

#[test]
fn crow_and_vee_ends_and_a_box_without_a_border() {
    let d = drawn(
        "specification { element box }
            model {
                a = box { style { border none } }
                b = box
                a -> b { head crow; tail vee }
            }
            views { view index { include * } }",
    );
    assert_eq!(d.nodes[0].border, LineStyle::Hidden);
    assert_eq!(d.nodes[1].border, LineStyle::Solid);
    let edge = &d.edges[0];
    // The tail's mark first: a filled head with a notch, at `a`.
    let (vee, crow) = (&edge.markers[0], &edge.markers[1]);
    assert!(vee.filled && vee.closed && vee.points.len() == 4);
    assert_eq!(vee.points[0].y, d.nodes[0].y + d.nodes[0].height);
    // A crow's foot is open, and spreads to where the line ends: its
    // two outer points are on the box, the middle one back up the line.
    assert!(!crow.filled && !crow.closed && crow.points.len() == 3);
    let top = d.nodes[1].y;
    assert_eq!((crow.points[0].y, crow.points[2].y), (top, top));
    assert!(crow.points[1].y < top);
    // The line runs on to the box: it is the foot's middle toe.
    assert_eq!(edge.path.last().unwrap().y, top);
}
