use crate::{Arrow, Direction, IconPosition, Rank, Severity, Size, Source, load};

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
    assert_eq!(edges, ["customer->shop | [...]", "shop->mail | sends receipts", "mail->customer | delivers"]);
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
    let spec = "specification { element box { style { color green } } relationship async { line dotted } }";
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
  a = box { icon aws:lambda; style { iconColor red } }
  b = box { style { sizing fixed; icon @/b.svg } }
  c = box { icon ./c.bmp }
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
            "1:Bundled icons ('tech:rust') are not supported yet; use a URL, or the path of an image file like './rust.svg'.",
            "2:'deployment' blocks are not supported yet; this one is ignored.",
            "4:Bundled icons ('aws:lambda') are not supported yet; use a URL, or the path of an image file like './lambda.svg'.",
            "4:'iconColor' is not supported yet; an icon is drawn in its own colors.",
            "5:'sizing fixed' is not supported yet; a box grows to fit its words.",
            "5:Image aliases ('@/...') are not supported yet; write the path from this file, like './images/logo.svg'.",
            "6:An icon from a file like './c.bmp' is not supported yet; use an SVG, PNG, JPEG, GIF or WebP file, or a URL.",
            "10:Manual layout is not supported yet; this view is laid out automatically.",
            "12:'ortho' routing is not supported yet; lines are drawn as curves.",
            "13:deployment views are not supported yet; this one is ignored.",
        ]
    );
    assert_eq!((ws.model.elements.len(), ws.model.relations.len(), ws.model.views.len()), (3, 1, 1));
}

#[test]
fn rank_names_what_the_view_shows() {
    let ws = load(&[Source {
        name: "m.c4".into(),
        text: "specification { element box }
model {
  a = box { inner = box }
  b = box
  c = box
}
views {
  view v {
    include *
    rank { a, b, a, nowhere }
    rank sink { c, a.inner }
    rank source { a.inner }
    rank sideways { a }
  }
  view w extends v { exclude b }
}"
        .into(),
    }]);
    let messages: Vec<_> = ws.diagnostics.iter().map(|d| format!("{}:{}", d.line, d.message)).collect();
    assert_eq!(messages.len(), 2, "{messages:#?}");
    assert!(messages[0].starts_with("10:") && messages[0].contains("nowhere"), "{messages:#?}");
    assert_eq!(messages[1], "13:Unknown rank 'sideways': use same, min, max, source or sink.");
    // Named twice counts once; what is not in the view is not named;
    // a block left with nothing is not there.
    let ranks = |id: &str| -> Vec<(Rank, Vec<String>)> {
        let view = ws.model.compute_view(id).unwrap();
        view.ranks.iter().map(|r| (r.rank, r.nodes.clone())).collect()
    };
    assert_eq!(
        ranks("v"),
        [(Rank::Same, vec!["a".to_string(), "b".to_string()]), (Rank::Sink, vec!["c".to_string()])]
    );
    assert_eq!(ranks("w"), [(Rank::Same, vec!["a".to_string()]), (Rank::Sink, vec!["c".to_string()])]);
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
    let ws =
        load(&[model, image("shop/arch/icons/box.svg", "<svg/>"), image("shop/shared.svg", "<svg>ab</svg>")]);
    assert!(ws.diagnostics.is_empty() && ws.wanted.is_empty(), "{:?}", ws.diagnostics);
    let view = ws.model.compute_view("v").unwrap();
    let icons: Vec<_> =
        view.nodes.iter().map(|n| (n.icon.as_deref().unwrap_or("-"), n.icon_size, n.icon_position)).collect();
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
fn an_icon_is_an_image_that_is_not_text() {
    let model = Source {
        name: "shop/m.c4".into(),
        text: "specification { element box }
model {
  a = box { icon ./logo.png }
  b = box { icon ./photo.JPG }
}"
        .into(),
    };
    let ws = load(std::slice::from_ref(&model));
    assert_eq!(ws.wanted, ["shop/logo.png", "shop/photo.JPG"]);
    assert_eq!(ws.diagnostics.len(), 2);

    // Whoever reads files gives an image's bytes in base64.
    let png = Source::from_bytes("shop/logo.png", &[0x89, b'P', b'N', b'G']).unwrap();
    assert_eq!(png.text, "iVBORw==");
    assert_eq!(Source::from_bytes("a.c4", b"model {}").unwrap().text, "model {}");
    assert!(Source::from_bytes("a.c4", &[0xff]).is_none());
    let ws = load(&[model, png, Source::from_bytes("shop/photo.JPG", b"jpg").unwrap()]);
    assert!(ws.wanted.is_empty() && ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
    let icon = |name: &str| ws.model.element(name).unwrap().style.icon.clone();
    assert_eq!(icon("a").as_deref(), Some("data:image/png;base64,iVBORw=="));
    assert_eq!(icon("b").as_deref(), Some("data:image/jpeg;base64,anBn"));
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
  view styled {
    include *, b with { notation 'Said last' }
    style a, b { notation 'Deprecated' }
    style c { color red }
  }
  view shared { include *; global style old }
}
global { style old element.kind = box { notation 'Old' } }"
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
    // A view's style says it over the kind; `with` over the style.
    let said = |id: &str| {
        let view = ws.model.compute_view(id).unwrap();
        view.nodes.iter().map(|n| n.notation.clone()).collect::<Vec<_>>()
    };
    assert_eq!(said("styled"), ["Deprecated", "Said last", ""]);
    assert_eq!(said("shared"), ["Old", "Old", ""]);
}

#[test]
fn the_rest_of_how_likec4_writes_a_model() {
    let ws = load(&[Source {
        name: "m.c4".into(),
        text: "specification {
  tag slow
  tag infra
  tag extra
  color brand rgb(255, 128, 0)
  color brand2 rgb(0 128 255)
  element box { #infra }
  relationship async {
    #slow
    title 'Asynchronous'
    description 'Does not wait'
    technology 'Kafka'
  }
}
model {
  a = box 'A' {
    #extra
    description '''
      Says \"hello\", and it's **bold**.
    '''
    metadata {
      owner 'core'
      regions ['us', 'eu']
    }
  }
  b = box 'B' { style { color brand } }
  c = box 'C'
  a <-> b 'talk'
  a -[async]<-> c
  b .async c 'by hand'
  b -> c 'opens' 'In a browser' 'HTTPS'
  b -> a 'two' 'Only a description'
  extend a.nothing
  extend a {
    #slow #extra
    metadata { regions 'ap' }
    link https://example.com 'More'
    inner = box
  }
  extend b -> c 'opens' {
    #slow
    metadata { latency '150ms' }
  }
}
views {
  view regions { include * where metadata.regions is 'eu' }
}"
        .into(),
    }]);
    let messages: Vec<_> = ws.diagnostics.iter().map(|d| d.to_string()).collect();
    assert_eq!(messages, ["m.c4:33:10: error: There is no element 'a.nothing' to extend."]);
    let m = &ws.model;
    let a = m.element("a").unwrap();
    assert_eq!(a.description, "Says \"hello\", and it's **bold**.");
    // Its kind's tags, its own, and what `extend` adds, once each.
    assert_eq!(a.tags, ["infra", "extra", "slow"]);
    let pairs = |list: &[(String, String)]| {
        list.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ")
    };
    assert_eq!(pairs(&a.metadata), "owner=core regions=us regions=eu regions=ap");
    assert_eq!(a.links, [("https://example.com".to_string(), "More".to_string())]);
    assert!(m.element("a.inner").is_some());
    assert_eq!(m.colors["brand"], "#ff8000");
    assert_eq!(m.colors["brand2"], "#0080ff");

    let rel = |i: usize| &m.relations[i];
    // Both ways: a mark at each end.
    assert_eq!((rel(0).style.head, rel(0).style.tail), (Arrow::Normal, Arrow::Normal));
    assert_eq!((rel(1).kind.as_deref(), rel(1).style.tail), (Some("async"), Arrow::Normal));
    // What the kind says, unless the relationship does.
    assert_eq!(
        (rel(1).title.as_str(), rel(1).description.as_str(), rel(1).technology.as_str()),
        ("Asynchronous", "Does not wait", "Kafka")
    );
    assert_eq!((rel(2).kind.as_deref(), rel(2).title.as_str()), (Some("async"), "by hand"));
    assert_eq!(rel(2).tags, ["slow"]);
    // Title, description, technology, in that order.
    assert_eq!(
        (rel(3).title.as_str(), rel(3).description.as_str(), rel(3).technology.as_str()),
        ("opens", "In a browser", "HTTPS")
    );
    assert_eq!((rel(4).description.as_str(), rel(4).technology.as_str()), ("Only a description", ""));
    assert_eq!(rel(3).tags, ["slow"]);
    assert_eq!(pairs(&rel(3).metadata), "latency=150ms");
    // One of a list is enough.
    assert_eq!(shows(&ws, "regions"), "a");
}

#[test]
fn relationship_predicates_name_a_kind_and_groups_still_include() {
    let ws = load(&[Source {
        name: "fleet.c4".into(),
        text: format!(
            "{FLEET}
views {{
  view kinded {{ include api -[async]-> * }}
  view dotted {{ include * .async jobs }}
  view into {{ include -[async]-> jobs }}
  view both {{ include api -[async]-> * where tag is not #old }}
  view grouped {{
    group 'Front' {{
      color amber
      opacity 20%
      include web
      group {{ include api }}
    }}
    include db
  }}
}}"
        ),
    }]);
    assert_eq!(shows(&ws, "kinded"), "api jobs");
    assert_eq!(shows(&ws, "dotted"), "api jobs");
    assert_eq!(shows(&ws, "into"), "api jobs");
    assert_eq!(shows(&ws, "both"), "");
    assert_eq!(shows(&ws, "grouped"), "web api db");
    // Only the group is not drawn, and says so, once.
    let messages: Vec<_> = ws.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert_eq!(
        messages,
        ["'group' is not supported yet; what it includes is shown without a frame around it."]
    );
}

#[test]
fn more_that_is_read_and_not_drawn_is_a_warning() {
    let ws = load(&[Source {
        name: "m.c4".into(),
        text: "specification {
  color glass rgba(0, 0, 0, 50%)
  color glass2 #00000080
  tag t { color #f00 }
  deploymentNode vm { notation 'VM' }
  element box { link https://example.com }
  relationship r { multiple true }
}
model {
  a = box { descriptionFile ./a.docx; style { shape document } }
  b = box
  a -> b { multiple true; navigateTo v }
}
diff plan 'A plan' { remove { a } }
views {
  view v {
    link https://example.com
    order 2
    routing ortho
    rank same { a, b }
    group 'G' { include a }
    include a -> b with { navigateTo v }
  }
  dynamic view d {
    a -> b { metadata { k 'v' } }
    opt 'sometimes' { a -> b }
    alt { when 'x' { a -> b } else { b -> a } }
    try { a -> b } catch 'failed' { b -> a } finally { a -> a }
  }
}"
        .into(),
    }]);
    let messages: Vec<_> = ws.diagnostics.iter().map(|d| format!("{}:{}", d.line, d.message)).collect();
    assert!(ws.diagnostics.iter().all(|d| d.severity == Severity::Warning), "{messages:#?}");
    assert_eq!(
        messages,
        [
            "2:A see-through color ('rgba(0,0,0,50%)') is not supported yet; it is drawn solid.",
            "3:A see-through color ('#00000080') is not supported yet; it is drawn solid.",
            "4:What a tag is given in '{ }' (its color) is not supported yet; it is ignored.",
            "5:'deploymentNode' kinds are not supported yet; this one is ignored.",
            "6:A link on a kind is not supported yet; give it to the elements themselves.",
            "7:'multiple' on a relationship is not supported yet; relationships between the same two boxes are drawn as one line.",
            "10:A description from a file like './a.docx' is not supported yet; use a .md or .txt file.",
            "10:The shape 'document' is not supported yet; it is drawn as a rectangle.",
            "12:'multiple' on a relationship is not supported yet; relationships between the same two boxes are drawn as one line.",
            "12:'navigateTo' on a relationship is not supported yet; it goes nowhere.",
            "14:'diff' blocks are not supported yet; this one is ignored.",
            "17:A link on a view is not supported yet; it is not shown anywhere.",
            "18:'order' is not supported yet; views are listed in the order they are written.",
            "19:'ortho' routing is not supported yet; lines are drawn as curves.",
            "21:'group' is not supported yet; what it includes is shown without a frame around it.",
            "22:'navigateTo' on a relationship is not supported yet; it goes nowhere.",
            "25:'metadata' on a step is not supported yet; it is ignored.",
            "26:Blocks of steps ('opt', 'loop', 'alt', 'try') are not supported yet; their steps are shown as ordinary steps, in order.",
            "27:Blocks of steps ('opt', 'loop', 'alt', 'try') are not supported yet; their steps are shown as ordinary steps, in order.",
            "28:Blocks of steps ('opt', 'loop', 'alt', 'try') are not supported yet; their steps are shown as ordinary steps, in order.",
        ]
    );
    // And what could be drawn is: every step of every block, in order.
    assert_eq!(shows(&ws, "v"), "a b");
    let steps = ws.model.compute_view("d").unwrap();
    let said: Vec<&str> = steps.edges.iter().map(|e| e.step.as_str()).collect();
    assert_eq!(said, ["1", "2", "3", "4", "5", "6", "7"]);
}

#[test]
fn a_line_is_what_its_view_says_with() {
    let ws = load(&[Source {
        name: "fleet.c4".into(),
        text: format!(
            "{FLEET}
views {{
  view restyled {{
    include *
    include web -> api with {{ title 'asks'; technology 'gRPC'; color red; line solid; head crow; tail vee }}
    include api -> * where kind is async with {{ color green; notation 'Queued' }}
    include -> legacy with {{ line none; head sideways; color nothing }}
  }}
  view plain {{ include * }}
}}"
        ),
    }]);
    let messages: Vec<_> = ws.diagnostics.iter().map(|d| d.message.as_str()).collect();
    // What `with` says is checked like any relationship's looks.
    assert_eq!(
        messages,
        [
            "A relationship always has a line: use solid, dashed or dotted.",
            "Unknown arrow 'sideways': use none, normal, onormal, open, vee, crow, diamond, odiamond, dot or odot.",
            "Unknown color 'nothing'. Use one of primary, secondary, muted, slate, blue, indigo, sky, red, gray, green, amber, or define it: specification { color nothing #rrggbb }.",
        ]
    );
    let looks = |view: &str| {
        let view = ws.model.compute_view(view).unwrap();
        view.edges
            .iter()
            .map(|e| {
                format!(
                    "{} {:?} {} {} {:?} {:?}>{:?} {:?}",
                    e.id, e.label, e.technology, e.color, e.line, e.tail, e.head, e.notation
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        looks("restyled"),
        [
            "web->api \"asks\" gRPC red Solid Vee>Crow \"\"",
            "api->db \"reads\"  gray Dashed None>Normal \"\"",
            "api->jobs \"queues\"  green Dashed None>Normal \"Queued\"",
            // What could not be read of it changed nothing.
            "jobs->legacy \"feeds\"  gray Dashed None>Normal \"\"",
        ]
    );
    // Only in the view that says so.
    assert_eq!(looks("plain")[0], "web->api \"calls\"  gray Dashed None>Normal \"\"");
}

#[test]
fn a_relationship_both_ways_is_one_from_either_end() {
    let ws = load(&[Source {
        name: "m.c4".into(),
        text: "specification { element box }
model {
  a = box; b = box; c = box; d = box
  a <-> b 'talk' { link https://example.com/talk 'How they talk'; link ./docs/talk.md }
  c -> d
  extend a <-> b 'talk' { link https://example.com/more }
}
views {
  view into_a { include -> a }
  view out_of_b { include b -> }
  view b_to_a { include b -> a }
  view into_c { include -> c }
  view cut { include *; exclude b -> a }
}"
        .into(),
    }]);
    assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
    // `a <-> b` goes into a and out of b as much as the other way.
    assert_eq!(shows(&ws, "into_a"), "a b");
    assert_eq!(shows(&ws, "out_of_b"), "a b");
    assert_eq!(shows(&ws, "b_to_a"), "a b");
    // `c -> d` goes only one way.
    assert_eq!(shows(&ws, "into_c"), "");
    let cut = ws.model.compute_view("cut").unwrap();
    assert_eq!(cut.edges.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["c->d"]);
    let links: Vec<_> =
        ws.model.relations[0].links.iter().map(|(url, title)| format!("{url}|{title}")).collect();
    assert_eq!(
        links,
        ["https://example.com/talk|How they talk", "./docs/talk.md|", "https://example.com/more|"]
    );
}

#[test]
fn a_description_can_be_kept_in_a_file() {
    let model = Source {
        name: "arch/m.c4".into(),
        text: "specification { element box }
model {
  a = box { descriptionFile ./docs/a.md }
  b = box { descriptionFile ../shared.txt; description 'Said here' }
  c = box { descriptionFile ./c.docx }
  d = box { style { border none } }
}"
        .into(),
    };
    let ws = load(std::slice::from_ref(&model));
    assert_eq!(ws.wanted, ["arch/docs/a.md"]);
    let messages: Vec<_> = ws.diagnostics.iter().map(|d| format!("{}:{}", d.line, d.message)).collect();
    assert_eq!(
        messages,
        [
            "3:There is no file 'arch/docs/a.md' for this description: check the path, or write the text as its description.",
            "5:A description from a file like './c.docx' is not supported yet; use a .md or .txt file.",
        ]
    );
    // With the file, it is one more source, and not read as a model.
    let text = Source { name: "arch/docs/a.md".into(), text: "\n  # A\n\n  Does **things**.\n".into() };
    let ws = load(&[model, text]);
    assert_eq!(ws.diagnostics.len(), 1, "{:?}", ws.diagnostics);
    assert!(ws.wanted.is_empty());
    assert_eq!(ws.model.elements.len(), 4);
    assert_eq!(ws.model.element("a").unwrap().description, "# A\n\nDoes **things**.");
    // Written out, it is what is written.
    assert_eq!(ws.model.element("b").unwrap().description, "Said here");
    assert_eq!(ws.model.element("d").unwrap().style.border, crate::Line::None);
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
        Source { name: "b.c4".into(), text: "global { styleGroup quiet { style * { color red } } }".into() },
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
