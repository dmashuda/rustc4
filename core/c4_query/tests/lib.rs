use super::*;
use c4_model::{Source, load};

fn shop() -> Model {
    let text = include_str!("../../../examples/shop.c4").to_string();
    let ws = load(&[Source { name: "shop/arch/shop.c4".into(), text }]);
    assert!(ws.diagnostics.is_empty());
    ws.model
}

#[test]
fn an_overview_to_start_from() {
    let text = overview(&shop());
    assert!(text.starts_with("14 elements, 15 relationships, 6 views.\nKinds: service 5, "), "{text}");
    assert!(text.contains("\n- shop (Shop, system), 9 inside: Sells things online"), "{text}");
    assert!(text.contains("\n- customer (Customer, actor): Browses the catalogue and places orders"));
}

#[test]
fn search_ranks_names_over_mentions() {
    let model = shop();
    let text = search(&model, "orders", None, None, 10);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].ends_with("elements:"), "{text}");
    assert_eq!(lines[1], "- shop.api.orders (Orders, service): Takes an order from basket to doorstep");
    // Mentioned in descriptions, further down.
    assert!(text.contains("- customer (Customer, actor): Browses the catalogue and places orders"), "{text}");
    assert!(
        text.contains("- shop.db (Database, database) [PostgreSQL]: Products, customers, orders"),
        "{text}"
    );

    // By technology, by tag, by kind alone, and by what a relationship says.
    assert!(search(&model, "kafka", None, None, 10).contains("shop.events"));
    assert_eq!(
        search(&model, "", None, Some("#next"), 10),
        "1 element:\n- shop.fulfilment (Fulfilment, service) [Rust]: Tells the warehouse what to ship"
    );
    assert!(search(&model, "", Some("actor"), None, 10).starts_with("2 elements:"));
    let cards = search(&model, "cards", None, None, 10);
    assert!(
        cards.contains("1 relationship:\n- shop.api.orders -> payments: charges cards through [HTTPS]"),
        "{cards}"
    );
    assert!(search(&model, "zebra", None, None, 10).starts_with("Nothing matches 'zebra'."));
    assert!(search(&model, "service", None, None, 2).ends_with("more."), "the rest are counted");
}

#[test]
fn an_element_in_full() {
    let model = shop();
    let text = element(&model, "shop.api").unwrap();
    assert!(text.starts_with(
            "shop.api (API, service)\nTechnology: Rust\nDescription: One front door for every client\nWritten in: shop/arch/shop.c4:"
        ));
    assert!(text.contains("\nInside: shop (Shop, system)"));
    assert!(text.contains("\nIn views: shop, api, orders"), "{text}");
    assert!(text.contains("\n\nContains:\n- shop.api.catalogue (Catalogue, service)"));
    assert!(text.contains(
        "- payments (Payment provider, external): charges cards through [HTTPS] (through shop.api.orders)"
    ));
    assert!(text.contains("\n\nUsed by:\n- shop.web (Storefront, app): calls [HTTPS, JSON]"));
    // A short name that only one element has is enough.
    assert!(element(&model, "fulfilment").unwrap().starts_with("shop.fulfilment (Fulfilment, service)"));
    let missing = element(&model, "shop.order").unwrap_err();
    assert!(
        missing.starts_with("There is no element 'shop.order'. Did you mean:\n- shop.api.orders"),
        "{missing}"
    );
}

#[test]
fn following_dependencies_at_the_level_asked() {
    let model = shop();
    // A whole system: what its parts reach are other top-level things.
    let text = neighbors(&model, "shop", Direction::Both, 3).unwrap();
    assert!(text.contains("shop uses:\nDirectly (3):\n- payments (Payment provider, external): charges cards through [HTTPS]"), "{text}");
    assert!(
        text.contains("\n2 steps away (1):\n- customer (Customer, actor): emails (through mail)"),
        "{text}"
    );
    assert!(
        text.contains("shop is used by:\nDirectly (1):\n- customer (Customer, actor): shops with"),
        "{text}"
    );

    // One part: only its own, and as parts.
    let text = neighbors(&model, "shop.events", Direction::Out, 5).unwrap();
    assert!(
        text.contains("Directly (1):\n- shop.fulfilment (Fulfilment, service) [Rust]")
            || text.contains("- shop.fulfilment (Fulfilment, service): delivers"),
        "{text}"
    );
    assert!(text.contains("2 steps away (2):\n- mail (Email service, external): sends updates through (through shop.fulfilment)"), "{text}");
    let text = neighbors(&model, "payments", Direction::Out, 2).unwrap();
    assert!(text.ends_with("payments uses:\nNothing: it uses nothing outside itself."), "{text}");
}

#[test]
fn how_one_thing_reaches_another() {
    let model = shop();
    let text = path(&model, "customer", "payments").unwrap();
    assert!(text.starts_with("2 steps:\ncustomer (Customer, actor)\n  -> shops with  (customer -> shop.web)\nshop (Shop, system)\n"), "{text}");
    assert!(text.ends_with("  -> charges cards through [HTTPS]  (shop.api.orders -> payments)\npayments (Payment provider, external)"), "{text}");
    // Between parts, part by part.
    let text = path(&model, "shop.web", "shop.fulfilment").unwrap();
    assert!(text.starts_with("3 steps:"), "{text}");
    assert!(text.contains("shop.events (Order events, queue)"));
    // Only the other way round.
    let text = path(&model, "payments", "shop.web").unwrap();
    assert!(text.starts_with("Nothing leads from payments to shop.web, but the other way round"), "{text}");
    assert!(path(&model, "shop", "shop.api").unwrap().contains("one is part of the other"));
    assert!(path(&model, "payments", "staff").unwrap().starts_with("No chain of relationships"));
    assert!(path(&model, "nope", "shop").is_err());
}

#[test]
fn views_as_text() {
    let model = shop();
    assert!(views(&model).starts_with(
        "- index: Landscape. The shop and everything around it\n- shop: Inside the shop (of shop)"
    ));
    let text = view(&model, "index").unwrap();
    assert!(text.starts_with(
        "index: Landscape\nThe shop and everything around it\n\n5 elements:\n- customer (Customer, actor)"
    ));
    assert!(text.contains("- shop (Shop, system) (closer: view shop)"));
    assert!(text.contains("\n\n5 relationships:\n- customer -> shop: shops with"), "{text}");
    let text = view(&model, "shop").unwrap();
    assert!(
        text.contains("- shop (Shop, system)\n  - shop.web (Storefront, app) [TypeScript, React]"),
        "{text}"
    );
    assert!(text.contains("- shop.api -> shop.db: reads; reads and writes"), "{text}");
    assert!(
        view(&model, "nope").unwrap_err().starts_with("There is no view 'nope'. The views are:\n- index")
    );
}
