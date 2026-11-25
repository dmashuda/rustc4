use super::*;

fn call(server: &mut Server, message: Value) -> Value {
    serde_json::from_str(&server.handle(&message.to_string()).expect("a reply")).unwrap()
}

fn tool(server: &mut Server, name: &str, arguments: Value) -> (String, bool) {
    let reply = call(
        server,
        json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": { "name": name, "arguments": arguments } }),
    );
    assert_eq!(reply["id"], 7);
    let result = &reply["result"];
    assert_eq!(result["content"][0]["type"], "text");
    (result["content"][0]["text"].as_str().unwrap().to_string(), result["isError"].as_bool().unwrap())
}

#[test]
fn the_handshake_and_the_list() {
    let mut server = Server::new(vec![PathBuf::from("examples/shop.c4")]);
    let reply = call(
        &mut server,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2024-11-05" } }),
    );
    assert_eq!(reply["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(reply["result"]["serverInfo"]["name"], "rustc4");
    assert!(reply["result"]["capabilities"]["tools"].is_object());
    // A notification gets no reply.
    assert!(server.handle(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
    assert_eq!(
        call(&mut server, json!({ "jsonrpc": "2.0", "id": "a", "method": "ping" }))["result"],
        json!({})
    );

    let reply = call(&mut server, json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }));
    let tools = reply["result"]["tools"].as_array().unwrap();
    let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["overview", "search", "element", "neighbors", "path", "views", "view"]);
    assert!(
        tools
            .iter()
            .all(|t| t["inputSchema"]["type"] == "object" && t["description"].as_str().unwrap().len() > 20)
    );

    let reply = call(&mut server, json!({ "jsonrpc": "2.0", "id": 3, "method": "resources/list" }));
    assert_eq!(reply["error"]["code"], -32601);
    let reply: Value = serde_json::from_str(&server.handle("{not json").unwrap()).unwrap();
    assert_eq!(reply["error"]["code"], -32700);
}

#[test]
fn tools_answer_in_text() {
    let mut server = Server::new(vec![PathBuf::from("examples/shop.c4")]);
    let (text, failed) = tool(&mut server, "overview", json!({}));
    assert!(!failed && text.starts_with("14 elements, 15 relationships, 6 views."), "{text}");
    let (text, _) = tool(&mut server, "search", json!({ "query": "kafka" }));
    assert!(text.contains("shop.events (Order events, queue) [Kafka]"), "{text}");
    let (text, _) = tool(&mut server, "element", json!({ "name": "shop.events" }));
    assert!(text.contains("Written in: examples/shop.c4:"), "{text}");
    let (text, _) = tool(&mut server, "neighbors", json!({ "name": "shop", "direction": "used_by" }));
    assert!(
        text.contains("shop is used by:\nDirectly (1):\n- customer") && !text.contains("shop uses:"),
        "{text}"
    );
    let (text, _) = tool(&mut server, "path", json!({ "from": "customer", "to": "payments" }));
    assert!(text.starts_with("2 steps:"), "{text}");
    let (text, _) = tool(&mut server, "view", json!({ "id": "api" }));
    assert!(text.starts_with("api: Inside the API"), "{text}");
    assert!(tool(&mut server, "views", json!({})).0.contains("- orders: An order, end to end"));

    // What cannot be answered says why, and is marked.
    let (text, failed) = tool(&mut server, "element", json!({ "name": "shop.event" }));
    assert!(failed && text.contains("Did you mean:\n- shop.events"), "{text}");
    assert_eq!(tool(&mut server, "element", json!({})), ("'element' needs 'name', a string".into(), true));
    assert_eq!(tool(&mut server, "explode", json!({})), ("There is no tool 'explode'.".into(), true));
    assert!(tool(&mut server, "neighbors", json!({ "name": "shop", "direction": "sideways" })).1);
}

#[test]
fn answers_follow_the_files() {
    let dir = PathBuf::from(
        std::env::var("TEST_TMPDIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string()),
    )
    .join("mcp-follow");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("m.c4"), "specification { element box } model { a = box 'A' }").unwrap();
    let mut server = Server::new(vec![dir.clone()]);
    assert!(tool(&mut server, "overview", json!({})).0.starts_with("1 element, "));
    std::fs::write(
        dir.join("m.c4"),
        "specification { element box } model { a = box 'A'; b = box 'B'; a -> c }",
    )
    .unwrap();
    let (text, _) = tool(&mut server, "overview", json!({}));
    assert!(text.starts_with("2 elements, "), "{text}");
    assert!(text.contains("Note: the sources have 1 problem"), "{text}");
    assert!(text.contains("There is no element 'c'."), "{text}");
    // Unreadable for a moment: the last model stays.
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(tool(&mut server, "overview", json!({})).0.starts_with("2 elements, "));
}

#[test]
fn a_whole_session_over_the_wire() {
    let input = concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        "\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        "\n\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"views"}}"#,
        "\n"
    );
    let mut output = Vec::new();
    serve(vec![PathBuf::from("examples/shop.c4")], &mut input.as_bytes(), &mut output);
    let lines: Vec<Value> =
        String::from_utf8(output).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2, "one reply per request, none for the notification");
    assert_eq!(lines[0]["result"]["protocolVersion"], "2025-06-18");
    assert!(lines[1]["result"]["content"][0]["text"].as_str().unwrap().starts_with("- index: Landscape"));
}
