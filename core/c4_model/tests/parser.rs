use super::*;

fn ok(text: &str) -> File {
    let (file, errors) = parse(0, text);
    let messages: Vec<_> =
        errors.iter().map(|e| format!("{}:{} {}", e.span.line, e.span.col, e.message)).collect();
    assert!(messages.is_empty(), "{messages:?}");
    file
}

#[test]
fn elements_in_both_spellings() {
    let file = ok("model {
            customer = actor 'Customer' 'Buys things'
            system cloud 'Cloud' {
                description 'Where it runs'
                #core, #next
                link https://example.com 'Docs'
                style { shape browser; color: green }
                api = service 'API' 'Serves' 'Rust'
                -> customer 'emails'
                api -[async]-> customer 'notifies' 'By email' 'SMTP' { line dotted }
            }
        }");
    assert_eq!(file.model.len(), 2);
    let ModelItem::Element(cloud) = &file.model[1] else { panic!() };
    assert_eq!((cloud.kind.as_str(), cloud.name.as_str()), ("system", "cloud"));
    assert_eq!(cloud.description.as_deref(), Some("Where it runs"));
    assert_eq!(cloud.tags, ["core", "next"]);
    assert_eq!(cloud.links[0].0, "https://example.com");
    assert_eq!(cloud.style.0.len(), 2);
    assert_eq!(cloud.items.len(), 3);
    let ModelItem::Element(api) = &cloud.items[0] else { panic!() };
    assert_eq!(api.technology.as_deref(), Some("Rust"));
    let ModelItem::Relation(implicit) = &cloud.items[1] else { panic!() };
    assert!(implicit.source.is_none());
    let ModelItem::Relation(rel) = &cloud.items[2] else { panic!() };
    assert_eq!(rel.kind.as_deref(), Some("async"));
    assert_eq!(rel.technology.as_deref(), Some("SMTP"));
    assert_eq!(rel.style.0[0].0, "line");
}

#[test]
fn views_and_predicates() {
    let file = ok("views {
            view index {
                title 'Landscape'
                include *
            }
            view of cloud.api {
                include *, cloud.*, -> cloud.api ->, a -> b,
                exclude element.kind = actor, cloud.db.**
                include customer with { color red }
                autoLayout LeftRight
                style *, cloud._ { color muted; opacity 10% }
            }
        }");
    assert_eq!(file.views.len(), 2);
    let view = &file.views[1];
    assert!(view.id.is_none());
    assert_eq!(view.of.as_ref().unwrap().text(), "cloud.api");
    let ViewRule::Include(first) = &view.rules[0] else { panic!() };
    assert_eq!(first.len(), 4);
    assert!(matches!(first[2], Predicate::Relation(RelExpr::InOut(_), ..)));
    assert!(matches!(first[3], Predicate::Relation(RelExpr::Direct(..), ..)));
    let ViewRule::Exclude(second) = &view.rules[1] else { panic!() };
    assert!(matches!(&second[0], Predicate::Element { expr: ElementExpr::Kind { eq: true, .. }, .. }));
    assert!(matches!(&second[1], Predicate::Element { expr: ElementExpr::Descendants(_), .. }));
    assert!(matches!(view.rules[3], ViewRule::AutoLayout { direction: Direction::LeftRight, .. }));
    let ViewRule::Style { targets, style } = &view.rules[4] else { panic!() };
    assert_eq!((targets.len(), style.0.len()), (2, 2));
}

#[test]
fn an_error_does_not_hide_the_rest() {
    let (file, errors) = parse(
        0,
        "model {
                a = system 'A'
                b = = system
                c = system 'C'
            }
            views { view v { include * } }",
    );
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].span.line, 3);
    assert_eq!(file.model.len(), 2);
    assert_eq!(file.views.len(), 1);
}
