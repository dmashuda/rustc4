use super::*;
use c4_model::{Source, load};

fn render(source: &str, options: &SvgOptions) -> String {
    let ws = load(&[Source { name: "t.c4".into(), text: source.into() }]);
    assert!(ws.diagnostics.is_empty(), "{:?}", ws.diagnostics);
    let view = ws.model.compute_view("index").unwrap();
    svg(&c4_layout::layout(&view, &ws.model.colors), options)
}

const SOURCE: &str = "
        specification {
            element person { style { shape person } }
            element box
            element store { style { shape storage } }
            element pipe { style { shape queue; border dashed } }
            element web { style { shape browser } }
            element phone { style { shape mobile } }
        }
        model {
            user = person 'A <User> & \"friend\"'
            app = box 'App' { inner = box 'Inner' }
            db = store 'DB'
            q = pipe 'Queue'
            w = web 'Web'
            p = phone 'Phone'
            user -> app.inner 'uses' { tail odiamond; line dotted }
            app.inner -> db
            app.inner -> q { head dot }
            w -> app 'calls'
            p -> w { head open }
        }
        views {
            view index { title 'Everything' include *, app.* }
            view inner of app.inner { include * }
        }";

#[test]
fn well_formed_and_addressable() {
    let out = render(SOURCE, &SvgOptions::default());
    assert!(out.starts_with(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 "#));
    assert!(out.ends_with("</svg>"));
    assert!(out.contains("<title>Everything</title>"));
    assert!(out.contains("A &lt;User&gt; &amp; &quot;friend&quot;"));
    assert!(out.contains(r#"<g class="node group" data-id="app" data-kind="box" data-view="app">"#));
    assert!(out.contains(r#"<g class="node" data-id="app.inner" data-kind="box" data-view="inner">"#));
    assert!(out.contains(
        r#"<g class="edge" data-id="user-&gt;app.inner" data-source="user" data-target="app.inner">"#
    ));
    assert!(out.contains(r#"<g class="edge-label" data-id="w-&gt;app">"#));
    // Every tag opened is closed, in order.
    let mut open: Vec<&str> = Vec::new();
    for tag in out.split('<').skip(1) {
        let tag = &tag[..tag.find('>').expect("a closed tag")];
        if let Some(name) = tag.strip_prefix('/') {
            assert_eq!(open.pop(), Some(name));
        } else if !tag.ends_with('/') {
            open.push(tag.split(' ').next().unwrap());
        }
    }
    assert!(open.is_empty(), "{open:?}");
    assert!(!out.contains("NaN") && !out.contains("inf"));
}

#[test]
fn styles_show() {
    let out = render(SOURCE, &SvgOptions { background: Some("#fff".into()) });
    assert!(out.contains(r##"<rect width="##) && out.contains(r##"fill="#fff"/>"##));
    assert!(out.contains(r#"stroke-dasharray="1.5 4""#), "a dotted line");
    assert!(out.contains(r#"stroke-dasharray="6 4""#), "a dashed border");
    assert!(out.contains("<polyline"), "an open arrowhead");
    assert_eq!(out.matches("<polygon").count(), 4, "three arrowheads and a diamond");
    assert!(out.contains(r#"<g class="open""#), "somewhere to go");
    assert!(
        out.contains(r#"stroke-linecap="round"><rect class="hit""#)
            && out.contains(r#"width="24" height="24""#),
        "and more than two thin strokes to click"
    );
}

#[test]
fn a_stack_is_its_shape_twice() {
    let out = render(
        "specification { element store { style { shape storage; multiple true } } }
            model { one = store 'One' }
            views { view index { include * } }",
        &SvgOptions::default(),
    );
    // A cylinder is two paths: the one behind is drawn first, 8 along
    // and 8 down.
    let node = &out[out.find(r#"<g class="node""#).unwrap()..];
    let starts: Vec<&str> =
        node.match_indices(r#"<path d="M"#).map(|(i, m)| &node[i + m.len()..i + m.len() + 5]).collect();
    assert_eq!(starts, ["32 42", "32 42", "24 34", "24 34"]);
}

#[test]
fn an_icon_is_an_image_under_the_words() {
    let out = render(
        "specification { element box }
            model { one = box 'One' { icon https://example.com/a.svg?x=1&y=2 } }
            views { view index { include * } }",
        &SvgOptions::default(),
    );
    let image = r#"<image href="https://example.com/a.svg?x=1&amp;y=2" x="59.41" y="44" width="36" height="36" preserveAspectRatio="xMidYMid meet"/>"#;
    let (shape, at, words) = (out.find("<rect").unwrap(), out.find(image), out.find("<text").unwrap());
    assert!(at.is_some_and(|at| shape < at && at < words), "{out}");
}

#[test]
fn a_legend_is_drawn_last_and_is_nothing_to_tap() {
    let ws = load(&[Source {
        name: "t.c4".into(),
        text: "specification {
                element store { notation 'Data & more' style { shape storage } }
                relationship async { notation 'Does not wait' line dotted }
            }
            model { a = store; b = store; a -[async]-> b }"
            .into(),
    }]);
    let view = ws.model.compute_view("index").unwrap();
    let draw = |legend| {
        svg(
            &c4_layout::layout_with(&view, &ws.model.colors, c4_layout::Options { legend }),
            &SvgOptions::default(),
        )
    };
    assert!(!draw(false).contains("legend"));
    let out = draw(true);
    let at = out.find(r#"<g class="legend">"#).unwrap();
    let legend = &out[at..];
    assert!(legend.ends_with("</g></svg>"));
    assert!(!legend.contains("data-id") && !legend.contains(r#"class="hit""#));
    // A frame, a cylinder (two paths), a dotted line with its head, and the words.
    assert!(legend.starts_with(r#"<g class="legend"><rect "#));
    assert_eq!(legend.matches("<path").count(), 3);
    assert!(legend.contains(r#"stroke-dasharray="1.5 4""#) && legend.contains("<polygon"));
    assert!(legend.contains(r#"<text class="Legend""#) && legend.contains(">Data &amp; more</text>"));
}

#[test]
fn a_box_without_a_border_has_nothing_drawn_around_it() {
    let out = render(
        "specification { element box }
            model { one = box 'One' { style { border none } } }
            views { view index { include * } }",
        &SvgOptions::default(),
    );
    assert!(out.contains(r#"stroke-width="1.5" stroke-dasharray="0 1"/>"#), "{out}");
}

#[test]
fn numbers_are_short() {
    assert_eq!(
        (n(12.0), n(1.5), n(0.126), n(-0.001)),
        ("12".into(), "1.5".into(), "0.13".into(), "0".into())
    );
}
