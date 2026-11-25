use super::*;

const SHOP: &str = include_str!("../../../examples/shop.c4");

fn shop(editable: bool) -> C4Ui {
    C4Ui::new(vec![Source { name: "shop.c4".into(), text: SHOP.into() }], editable)
}

#[test]
fn the_legend_shows_and_hides_where_there_is_one() {
    // The shop's specification gives no kind a notation.
    let ui = shop(false);
    let screen = ui.screen();
    assert!(screen.legend.is_none() && screen.diagram.unwrap().legend.is_none());

    let source = "specification { element box { notation 'A box' }; element plain }
            model { a = box; b = plain }
            views { view index { include * }; view bare { include b } }";
    let mut ui = C4Ui::new(vec![Source { name: "m.c4".into(), text: source.into() }], false);
    let screen = ui.screen();
    let button = screen.legend.unwrap();
    // It starts showing.
    assert_eq!((button.id.as_str(), button.text.as_str(), button.selected), ("legend", "Legend", true));
    assert_eq!(button.hint, "Hide what the shapes and lines mean");
    assert_eq!(screen.diagram.unwrap().legend.unwrap().nodes.len(), 1);

    let screen = ui.tap("legend");
    let button = screen.legend.unwrap();
    assert!(!button.selected);
    assert_eq!(button.hint, "Show what the shapes and lines mean");
    assert!(screen.diagram.unwrap().legend.is_none());
    // What is exported is what shows.
    assert!(ui.diagram_of("index").unwrap().legend.is_none());

    // A view with nothing to explain has no legend and no button.
    ui.tap("legend");
    let screen = ui.tap("view.bare");
    assert!(screen.legend.is_none() && screen.diagram.unwrap().legend.is_none());
    assert!(ui.tap("back").diagram.unwrap().legend.is_some());
}

#[test]
fn a_line_shows_the_links_of_its_relationships() {
    let source = "specification { element box }
            model {
                a = box { inner = box }
                b = box
                a -> b 'one' { link https://example.com/one 'One' }
                a.inner -> b 'two' { link https://example.com/two; link https://example.com/one }
            }
            views { view index { include a, b } }";
    let mut ui = C4Ui::new(vec![Source { name: "m.c4".into(), text: source.into() }], false);
    let details = ui.tap("edge.a->b").details.unwrap();
    let links: Vec<_> = details.links.iter().map(|l| (l.text.as_str(), l.url.as_str())).collect();
    assert_eq!(
        links,
        [("One", "https://example.com/one"), ("https://example.com/two", "https://example.com/two")]
    );
}

fn texts(items: &[Item]) -> Vec<&str> {
    items.iter().map(|i| i.text.as_str()).collect()
}

#[test]
fn opens_on_the_landscape() {
    let ui = shop(false);
    let screen = ui.screen();
    assert_eq!((screen.title.as_str(), screen.view.as_str()), ("Landscape", "index"));
    assert_eq!(screen.description, "The shop and everything around it");
    assert_eq!(
        texts(&screen.views),
        [
            "Landscape",
            "Inside the shop",
            "Inside the API",
            "An order, end to end",
            "Placing an order",
            "Placing an order, as a sequence"
        ]
    );
    assert_eq!(screen.views.iter().filter(|v| v.selected).count(), 1);
    assert!(screen.views[0].selected && screen.views[0].id == "view.index");
    assert_eq!(screen.status, "14 elements · 15 relationships · 6 views");
    assert!(screen.back.is_none() && screen.details.is_none() && screen.editor.is_none());
    assert!(screen.problems.is_empty() && screen.problems_title.is_empty() && screen.empty.is_empty());
    assert_eq!(screen.diagram.as_ref().unwrap().nodes.len(), 5);
    assert_eq!(screen.exports[0].file_name, "index.svg");
    assert_eq!(screen.exports[1].kind, ExportKind::Png);
}

#[test]
fn moving_between_views_and_back() {
    let mut ui = shop(false);
    let screen = ui.tap("open.shop");
    assert_eq!(screen.view, "shop");
    let back = screen.back.expect("somewhere to go back to");
    assert_eq!((back.id.as_str(), back.hint.as_str()), ("back", "Back to Landscape"));
    let screen = ui.tap("view.orders");
    assert_eq!(screen.title, "An order, end to end");
    assert_eq!(ui.tap("back").view, "shop");
    let screen = ui.tap("back");
    assert_eq!(screen.view, "index");
    assert!(screen.back.is_none());
    // Nowhere to go, nothing happens; the same for ids nobody knows.
    assert_eq!(ui.tap("back").view, "index");
    assert_eq!(ui.tap("view.nope").view, "index");
    assert_eq!(ui.tap("open.customer").view, "index");
    assert_eq!(ui.tap("whatever").view, "index");
    // Showing the view already showing is not a step to go back over.
    assert!(ui.tap("view.index").back.is_none());
}

#[test]
fn selecting_an_element_says_what_it_is() {
    let mut ui = shop(false);
    ui.tap("view.shop");
    let screen = ui.tap("node.shop.api");
    assert_eq!(screen.selected, "node.shop.api");
    let details = screen.details.expect("details");
    assert_eq!(details.title, "API");
    assert_eq!(details.subtitle, "service · shop.api");
    assert_eq!(details.technology, "Rust");
    assert_eq!(details.description, "One front door for every client");
    assert_eq!(texts(&details.actions), ["Open “Inside the API”"]);
    assert_eq!(details.actions[0].id, "open.shop.api");
    let titles: Vec<_> = details.sections.iter().map(|s| s.title.as_str()).collect();
    assert_eq!(titles, ["Uses", "Used by", "Contains", "Also in"]);
    let uses = &details.sections[0].rows;
    assert_eq!(
        texts(uses),
        ["→ Database", "→ Database", "→ Database", "→ Order events", "→ Payment provider"]
    );
    assert_eq!(uses[0].detail, "reads · via Catalogue");
    assert_eq!(uses[4].detail, "charges cards through · HTTPS · via Orders");
    assert_eq!(uses[4].id, "node.payments");
    assert_eq!(texts(&details.sections[1].rows), ["← Storefront", "← Mobile app"]);
    assert_eq!(texts(&details.sections[2].rows), ["Catalogue", "Orders", "Accounts"]);
    assert_eq!(
        texts(&details.sections[3].rows),
        ["Inside the API", "An order, end to end", "Placing an order"]
    );

    // From a row to another element, to another view, and it stays selected there.
    assert_eq!(ui.tap("node.shop.fulfilment").details.unwrap().tags, ["next"]);
    let screen = ui.tap("view.orders");
    assert_eq!(screen.selected, "node.shop.fulfilment");
    // Not where it does not show.
    assert_eq!(ui.tap("view.api").selected, "");
    assert!(ui.tap("node.nope").details.is_none());
    ui.tap("node.shop.api.orders");
    assert!(ui.tap("canvas").details.is_none());
    ui.tap("node.shop.api.orders");
    assert!(ui.tap("details.close").details.is_none());
}

#[test]
fn selecting_a_line_lists_what_it_stands_for() {
    let mut ui = shop(false);
    ui.tap("view.shop");
    let details = ui.tap("edge.shop.api->shop.db").details.expect("details");
    assert_eq!(details.title, "API → Database");
    assert_eq!(details.subtitle, "3 relationships");
    let rows = &details.sections[0].rows;
    assert_eq!(texts(rows), ["reads", "reads and writes", "reads and writes"]);
    assert_eq!(rows[1].detail, "Orders → Database");
    assert!(rows.iter().all(|r| r.id.is_empty()));
    assert_eq!(details.sections[1].rows[1].id, "node.shop.db");
    assert!(ui.tap("edge.nope->nope").details.is_some(), "an unknown line changes nothing");
}

#[test]
fn editing_redraws_and_reports() {
    let mut ui = shop(true);
    let editor = ui.screen().editor.expect("an editor");
    assert_eq!((editor.title.as_str(), editor.file, texts(&editor.files)), ("Source", 0, vec!["shop.c4"]));
    assert_eq!(editor.text, SHOP);
    ui.tap("view.api");
    ui.tap("node.shop.api.orders");

    // A typo: everything else still draws, and the problem is listed.
    let broken =
        SHOP.replace("orders -> accounts 'checks the customer'", "orders -> acounts 'checks the customer'");
    let screen = ui.set_source(0, &broken);
    assert_eq!(screen.view, "api");
    assert_eq!(screen.selected, "node.shop.api.orders");
    assert_eq!(screen.problems_title, "1 problem");
    assert_eq!(screen.status, "1 problem");
    let problem = &screen.problems[0];
    assert_eq!(problem.text, "There is no element 'acounts'.");
    assert!(problem.error && problem.file == "shop.c4" && problem.line > 40);
    assert_eq!(screen.diagram.unwrap().edges.len(), ui.diagram_of("api").unwrap().edges.len());

    // The showing view is deleted: the landscape takes its place.
    let without = SHOP.replace("view api of shop.api", "view other of shop.api");
    let screen = ui.set_source(0, &without);
    assert_eq!(screen.view, "index");
    assert!(screen.problems.is_empty());
    assert_eq!(screen.selected, "node.shop.api.orders", "the element is still in the model");

    // Nothing left at all.
    let screen = ui.set_source(0, "");
    assert!(screen.diagram.is_none() && screen.views.is_empty() && screen.exports.is_empty());
    assert!(screen.empty.starts_with("Nothing to draw yet."));
    assert_eq!(screen.selected, "");
    let screen = ui.set_source(0, "model {");
    assert!(screen.empty.starts_with("Nothing to draw until"));
    // And back.
    assert_eq!(ui.set_source(0, SHOP).view, "index");
    assert_eq!(ui.set_source(7, "ignored").view, "index");
}

#[test]
fn selecting_picks_out_what_is_connected() {
    let mut ui = shop(false);
    ui.tap("view.shop");
    assert!(ui.screen().related.is_empty(), "nothing selected, nothing faded");
    let mut related = ui.tap("node.shop.events").related;
    related.sort();
    assert_eq!(
        related,
        [
            "edge.shop.api->shop.events",
            "edge.shop.events->shop.fulfilment",
            "node.shop",
            "node.shop.api",
            "node.shop.events",
            "node.shop.fulfilment",
        ]
    );
    // A group: everything in it, and what its members reach outside.
    let related = ui.tap("node.shop").related;
    assert!(related.contains(&"node.customer".to_string()) && related.contains(&"node.shop.db".to_string()));
    assert!(related.contains(&"edge.shop.web->shop.api".to_string()));
    assert!(!related.contains(&"edge.mail->customer".to_string()));
    // A line: itself and its two ends (and the group around them).
    let mut related = ui.tap("edge.customer->shop.web").related;
    related.sort();
    assert_eq!(related, ["edge.customer->shop.web", "node.customer", "node.shop", "node.shop.web"]);
    assert!(ui.tap("canvas").related.is_empty());
}

#[test]
fn everything_with_something_inside_can_be_gone_into() {
    // No view of `shop.api` is written here, only of the landscape.
    let text = SHOP.split("views {").next().unwrap().to_string() + "views { view index { include * } }";
    let mut ui = C4Ui::new(vec![Source { name: "shop.c4".into(), text }], false);
    let screen = ui.screen();
    assert_eq!(texts(&screen.views), ["Landscape"], "generated views are not listed");
    assert_eq!(screen.status, "14 elements · 15 relationships · 1 view");
    let screen = ui.tap("open.shop");
    assert_eq!((screen.view.as_str(), screen.title.as_str()), ("shop", "Shop"));
    let screen = ui.tap("open.shop.api");
    assert_eq!((screen.view.as_str(), screen.title.as_str()), ("shop_api", "API"));
    assert_eq!(ui.tap("back").view, "shop");
    assert_eq!(ui.tap("view.shop_api").view, "shop_api", "and by its id, as a link would");
}

#[test]
fn a_model_can_be_split_over_files() {
    // A model with no specification: every kind is unknown.
    let model = "model { a = box 'A'; b = box 'B'; a -> b }";
    let mut ui = C4Ui::new(vec![Source { name: "model.c4".into(), text: model.into() }], true);
    let screen = ui.screen();
    assert_eq!(screen.problems.len(), 2);
    let editor = screen.editor.unwrap();
    assert_eq!((editor.add.id.as_str(), editor.remove), ("file.add", None));

    // The specification goes in a second file, as in a LikeC4 project.
    let editor = ui.tap("file.add").editor.unwrap();
    assert_eq!(texts(&editor.files), ["model.c4", "file-2.c4"]);
    assert_eq!((editor.file, editor.text.as_str()), (1, ""));
    assert_eq!(editor.remove.unwrap().hint, "Remove file-2.c4");
    let screen = ui.set_source(1, "specification { element box }");
    assert!(screen.problems.is_empty());
    assert_eq!(screen.status, "2 elements · 1 relationship · 1 view");
    assert_eq!(ui.sources()[1].text, "specification { element box }");

    // Back to the first file, and the second can go again.
    assert_eq!(ui.tap("file.0").editor.unwrap().text, model);
    ui.tap("file.1");
    let screen = ui.tap("file.remove");
    assert_eq!(screen.problems.len(), 2);
    let editor = screen.editor.unwrap();
    assert_eq!((editor.file, texts(&editor.files)), (0, vec!["model.c4"]));
    // The only file stays, and a model that is only shown cannot be added to.
    assert_eq!(ui.tap("file.remove").editor.unwrap().files.len(), 1);
    let mut shown = shop(false);
    shown.tap("file.add");
    assert_eq!(shown.sources().len(), 1);
}

#[test]
fn opening_another_model_replaces_this_one() {
    let mut ui = shop(true);
    ui.tap("view.shop");
    let other = Source {
        name: "other.c4".into(),
        text: "specification { element box } model { a = box 'A' }".into(),
    };
    let screen = ui.set_sources(vec![other.clone()], false);
    assert_eq!(
        (screen.view.as_str(), screen.status.as_str()),
        ("index", "1 element · 0 relationships · 1 view")
    );
    assert!(screen.editor.is_none() && screen.back.is_none());
    assert_eq!(screen.controls.open, "Open…");
    // A viewer shown nothing says how to be shown something.
    let waiting = C4Ui::new(Vec::new(), false).screen();
    assert!(waiting.empty.starts_with("No model is open."), "{}", waiting.empty);
    assert!(C4Ui::new(Vec::new(), true).screen().empty.starts_with("Nothing to draw yet."));
    // The same again changes nothing, selection included.
    ui.tap("node.a");
    assert_eq!(ui.set_sources(vec![other], false).selected, "node.a");
    // The files changed on disk: what is showing stays.
    ui.set_sources(vec![Source { name: "shop.c4".into(), text: SHOP.into() }], false);
    ui.tap("view.api");
    let edited = SHOP.replace("'Catalogue'", "'Products'");
    let screen = ui.set_sources(vec![Source { name: "shop.c4".into(), text: edited }], false);
    assert_eq!(screen.view, "api");
    assert!(screen.diagram.unwrap().nodes.iter().any(|n| n.title == "Products"));
}

#[test]
fn the_screen_is_json_without_the_diagram() {
    let mut ui = shop(true);
    let value: serde_json::Value = serde_json::to_value(ui.tap("node.customer")).unwrap();
    let mut keys: Vec<_> = value.as_object().unwrap().keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "back",
            "controls",
            "description",
            "details",
            "editor",
            "empty",
            "exports",
            "legend",
            "problems",
            "problemsTitle",
            "related",
            "selected",
            "status",
            "title",
            "view",
            "views",
            "viewsTitle"
        ]
    );
    assert_eq!(value["exports"][0]["fileName"], "index.svg");
    assert_eq!(value["exports"][0]["kind"], "Svg");
    assert_eq!(value["details"]["close"]["id"], "details.close");
    assert_eq!(ui.view_ids(), ["index", "shop", "api", "orders", "checkout", "checkoutSequence"]);
}

#[test]
fn a_step_of_a_dynamic_view_says_which_it_is() {
    let mut ui = shop(false);
    ui.tap("view.checkout");
    let diagram = ui.diagram().expect("a diagram");
    let labels: Vec<String> = diagram.edges.iter().map(|e| e.title.clone()).collect();
    assert_eq!(labels[0], "1. presses Buy");
    assert_eq!(labels[2], "3. checks prices", "said as the model says it");
    assert_eq!(labels[5], "6.1. saves the order");
    // An answer going back points back.
    assert_eq!(
        (diagram.edges[4].source.as_str(), diagram.edges[4].target.as_str()),
        ("payments", "shop.api.orders")
    );
    let screen = ui.tap("edge.shop.api.orders->payments#4");
    assert_eq!(screen.selected, "edge.shop.api.orders->payments#4");
    let details = screen.details.expect("details");
    assert_eq!(details.subtitle, "Step 4");
    assert_eq!(details.description, "charges the card");
    assert_eq!(details.sections[0].title, "Relationships");
    // Nothing in the model goes from the provider back to the shop.
    let details = ui.tap("edge.payments->shop.api.orders#5").details.expect("details");
    assert_eq!(details.sections.len(), 1);
}
