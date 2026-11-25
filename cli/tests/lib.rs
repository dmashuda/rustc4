use super::*;

fn rustc4(args: &[&str]) -> (i32, String, String) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = run(&args, &mut out, &mut err);
    (code, String::from_utf8(out).unwrap(), String::from_utf8(err).unwrap())
}

#[test]
fn a_source_is_named_with_slashes_on_every_platform() {
    assert_eq!(slashed("examples\\delivery\\delivery.c4", '\\'), "examples/delivery/delivery.c4");
    assert_eq!(slashed("examples/a\\b.c4", '/'), "examples/a\\b.c4");
}

/// An icon's file is found beside the model whether it is text or not,
/// and goes into a bundle with it.
#[test]
fn an_icon_is_read_from_a_file_that_is_text_or_not() {
    let dir = scratch("icons");
    let model = "specification { element box }
model { b = box { icon ./logo.png }; c = box { icon ./logo.svg } }";
    std::fs::write(dir.join("m.c4"), model).unwrap();
    std::fs::write(dir.join("logo.png"), [0x89, b'P', b'N', b'G']).unwrap();
    std::fs::write(dir.join("logo.svg"), "<svg/>").unwrap();
    let sources = read_sources(std::slice::from_ref(&dir)).unwrap();
    let ws = c4_model::load(&sources);
    assert!(ws.diagnostics.is_empty() && ws.wanted.is_empty(), "{:?}", ws.diagnostics);
    let icon = |name: &str| ws.model.element(name).unwrap().style.icon.clone().unwrap();
    assert_eq!(icon("b"), "data:image/png;base64,iVBORw==");
    assert_eq!(icon("c"), "data:image/svg+xml;base64,PHN2Zy8+");

    let again = site::unbundle(&site::bundle(&sources).unwrap()).unwrap();
    let mut names: Vec<_> = again.iter().map(|s| s.name.as_str()).collect();
    names.sort();
    assert_eq!(names, ["logo.png", "logo.svg", "m.c4"]);
    assert!(c4_model::load(&again).diagnostics.is_empty());
}

/// A fresh directory under the test's own temporary one.
pub(crate) fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(
        std::env::var("TEST_TMPDIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string()),
    )
    .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn checks_and_lists() {
    assert_eq!(
        rustc4(&["check", "examples/shop.c4"]),
        (0, "ok: 14 elements, 15 relationships, 6 views\n".into(), String::new())
    );
    // The other example: LikeC4's own "Big Bank", as LikeC4 writes it.
    assert_eq!(rustc4(&["check", "examples/bigbank.c4"]).1, "ok: 19 elements, 26 relationships, 9 views\n");
    let (code, out, _) = rustc4(&["views", "examples/shop.c4"]);
    assert_eq!(code, 0);
    assert_eq!(out.lines().next(), Some("index\tLandscape"));
    assert_eq!(out.lines().count(), 6);
}

#[test]
fn problems_fail_the_check_with_a_place() {
    let dir = scratch("broken");
    std::fs::write(dir.join("m.c4"), "specification { element box }\nmodel {\n  a = box\n  a -> b\n}\n")
        .unwrap();
    let (code, out, err) = rustc4(&["check", dir.to_str().unwrap()]);
    assert_eq!((code, out.as_str()), (1, ""));
    assert!(err.contains("m.c4:4:8: error: There is no element 'b'."), "{err}");
    assert!(err.ends_with("1 error\n"));
}

#[test]
fn a_legend_is_there_unless_turned_off() {
    let dir = scratch("legend");
    let source = dir.join("m.c4");
    std::fs::write(&source, "specification { element box { notation 'A box' } } model { a = box }").unwrap();
    let (source, out_dir) = (source.to_str().unwrap(), dir.to_str().unwrap());
    let drawn = |args: &[&str], file: &str| {
        let (code, _, err) = rustc4(args);
        assert_eq!((code, err.as_str()), (0, ""));
        std::fs::read_to_string(dir.join(file)).unwrap()
    };
    let svg = drawn(&["export", "svg", source, "-o", out_dir], "index.svg");
    assert!(svg.contains(r#"<g class="legend">"#) && svg.contains(">A box</text>"));
    let json = drawn(&["export", "json", source, "-o", out_dir], "index.json");
    assert!(json.contains(r#""legend": {"#), "{json}");
    let site = drawn(&["build", source, "-o", out_dir], "views/index.svg");
    assert!(site.contains(r#"<g class="legend">"#));
    let svg = drawn(&["export", "svg", source, "-o", out_dir, "--no-legend"], "index.svg");
    assert!(!svg.contains("legend"));
    let site = drawn(&["build", source, "-o", out_dir, "--no-legend"], "views/index.svg");
    assert!(!site.contains("legend"));
}

#[test]
fn exports_every_view_in_each_format() {
    let dir = scratch("export");
    let out_dir = dir.to_str().unwrap();
    let (code, out, err) = rustc4(&["export", "svg", "examples/shop.c4", "-o", out_dir]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert_eq!(out.lines().count(), 6);
    let svg = std::fs::read_to_string(dir.join("shop.svg")).unwrap();
    assert!(svg.starts_with("<svg ") && svg.contains("Storefront"));

    let (code, _, _) = rustc4(&["export", "json", "examples/shop.c4", "-o", out_dir, "--view", "api"]);
    assert_eq!(code, 0);
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("api.json")).unwrap()).unwrap();
    assert_eq!(json["id"], "api");
    assert!(json["nodes"].as_array().unwrap().len() > 4);
    assert!(!dir.join("shop.json").exists(), "only the view asked for");

    let (code, _, err) =
        rustc4(&["export", "png", "examples/shop.c4", "-o", out_dir, "--view", "index", "--scale", "1"]);
    assert_eq!((code, err.as_str()), (0, ""));
    let png = std::fs::read(dir.join("index.png")).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
}

#[test]
fn builds_a_site() {
    let dir = scratch("site");
    let (code, out, err) = rustc4(&["build", "examples/shop.c4", "-o", dir.to_str().unwrap()]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.ends_with(": 11 files, 6 views\n"), "{out}");
    for file in [
        "index.html",
        "style.css",
        "main.js",
        "rustc4_bg.wasm",
        "model.json",
        "views/index.svg",
        "views/orders.svg",
        "views/checkout.svg",
    ] {
        assert!(dir.join(file).is_file(), "{file}");
    }
    assert_eq!(&std::fs::read(dir.join("rustc4_bg.wasm")).unwrap()[..4], b"\0asm");
    let model: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("model.json")).unwrap()).unwrap();
    assert_eq!(model["sources"][0]["name"], "shop.c4");
    assert!(model["sources"][0]["text"].as_str().unwrap().contains("specification"));
}

#[test]
fn says_what_is_wrong_with_the_command() {
    assert_eq!(rustc4(&[]).0, 2);
    assert_eq!(rustc4(&["--help"]).0, 0);
    let (code, _, err) = rustc4(&["export", "gif", "examples/shop.c4"]);
    assert_eq!((code, err.as_str()), (2, "rustc4: export what? 'svg', 'png' or 'json', not 'gif'\n"));
    assert_eq!(rustc4(&["check"]).2, "rustc4: give at least one file or directory\n");
    assert!(rustc4(&["check", "nowhere"]).2.starts_with("rustc4: nowhere: "));
    let (code, _, err) = rustc4(&["export", "svg", "examples/shop.c4", "--view", "nope"]);
    assert_eq!(code, 2);
    assert_eq!(
        err,
        "rustc4: there is no view 'nope' (there are: index, shop, api, orders, checkout, checkoutSequence)\n"
    );
    assert_eq!(rustc4(&["frobnicate"]).2, "rustc4: unknown command 'frobnicate' (try --help)\n");
}

#[test]
fn bundles_and_answers_questions() {
    // Two "repositories": the shop, and another that adds to it.
    let dir = scratch("bundle");
    std::fs::create_dir_all(dir.join("audit")).unwrap();
    let extra = "specification { element service } model { extend shop { audit = service 'Audit log' } audit -> db 'writes' }";
    std::fs::write(dir.join("audit/audit.c4"), extra).unwrap();
    let bundle = dir.join("all.json");
    let (code, out, err) = rustc4(&[
        "bundle",
        "examples/shop.c4",
        dir.join("audit").to_str().unwrap(),
        "-o",
        bundle.to_str().unwrap(),
    ]);
    assert_eq!((code, err.as_str()), (0, ""));
    assert!(out.ends_with("all.json: 2 files\n"), "{out}");
    // The bundle reads back as the model it was made from.
    let (_, out, _) = rustc4(&["check", bundle.to_str().unwrap()]);
    assert_eq!(out, "ok: 15 elements, 16 relationships, 6 views\n");
    assert_eq!(
        rustc4(&["bundle", "examples/shop.c4"]).2,
        "rustc4: bundle needs somewhere to write: -o <file.json>\n"
    );

    let (code, out, _) =
        rustc4(&["query", bundle.to_str().unwrap(), "--", "search", "audit", "--kind", "service"]);
    assert_eq!(code, 0);
    assert_eq!(out, "1 element:\n- shop.audit (Audit log, service)\n");
    let (_, out, _) = rustc4(&["query", bundle.to_str().unwrap(), "--", "element", "audit"]);
    assert!(out.contains("Written in: ") && out.contains("audit.c4:1"), "{out}");
    let (_, out, _) = rustc4(&["query", "examples/shop.c4", "--", "used-by", "shop.db", "2"]);
    assert!(out.contains("2 steps away"), "{out}");
    let (code, _, err) = rustc4(&["query", "examples/shop.c4", "--", "element", "nope"]);
    assert_eq!(code, 1);
    assert!(err.starts_with("There is no element 'nope'."));
    assert_eq!(
        rustc4(&["query", "examples/shop.c4", "--", "sing"]).2,
        "rustc4: unknown question 'sing' (try --help)\n"
    );
    assert!(rustc4(&["query", "examples/shop.c4"]).2.contains("put the question after '--'"));
}

#[test]
fn serve_wants_an_address_to_listen_on() {
    assert_eq!(
        rustc4(&["serve", "examples/shop.c4", "--host", "everyone"]).2,
        "rustc4: 'everyone' is not an address to listen on: use one like 127.0.0.1 or 0.0.0.0\n"
    );
    assert_eq!(host("0.0.0.0"), Ok(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
    assert!(host("::").is_ok_and(|a| a.is_unspecified()));
}
