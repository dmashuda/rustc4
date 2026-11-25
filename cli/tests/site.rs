use super::*;

#[test]
fn names_lose_what_they_share() {
    let source = |name: &str| Source { name: name.into(), text: String::new() };
    assert_eq!(
        relative_names(&[source("/home/me/arch/a.c4"), source("/home/me/arch/sub/b.c4")]),
        ["a.c4", "sub/b.c4"]
    );
    assert_eq!(relative_names(&[source("examples/shop.c4")]), ["shop.c4"]);
    assert_eq!(relative_names(&[source("a.c4"), source("b.c4")]), ["a.c4", "b.c4"]);
}
