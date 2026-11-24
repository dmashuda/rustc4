//! The model, for a language model to ask about: an MCP server on stdio.
//!
//! MCP is JSON-RPC, one message a line. The whole of it used here is
//! `initialize`, `tools/list` and `tools/call`; the tools are the questions
//! in //core/c4_query, which answer in text. The sources are read again
//! whenever they have changed, so the answers follow the files.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use c4_model::{Severity, Source, Workspace};
use c4_query::Direction;
use serde_json::{Value, json};

/// One question, however it was asked: at the command line or as a tool.
pub enum Question {
    Overview,
    Search { query: String, kind: Option<String>, tag: Option<String>, limit: usize },
    Element { name: String },
    Neighbors { name: String, direction: Direction, depth: usize },
    Path { from: String, to: String },
    Views,
    View { id: String },
}

/// The answer, or what was wrong with the question.
pub fn answer(ws: &Workspace, question: &Question) -> Result<String, String> {
    let model = &ws.model;
    match question {
        Question::Overview => {
            let mut text = c4_query::overview(model);
            let errors = ws.diagnostics.iter().filter(|d| d.severity == Severity::Error).count();
            if errors > 0 {
                // Whoever reads the answers should know they may be missing something.
                text.push_str(&format!(
                    "\n\nNote: the sources have {errors} problem{}, so parts of the model may be missing. The first: {}",
                    if errors == 1 { "" } else { "s" },
                    ws.diagnostics.iter().find(|d| d.severity == Severity::Error).expect("counted")
                ));
            }
            Ok(text)
        }
        Question::Search { query, kind, tag, limit } => {
            Ok(c4_query::search(model, query, kind.as_deref(), tag.as_deref(), *limit))
        }
        Question::Element { name } => c4_query::element(model, name),
        Question::Neighbors { name, direction, depth } => {
            c4_query::neighbors(model, name, *direction, *depth)
        }
        Question::Path { from, to } => c4_query::path(model, from, to),
        Question::Views => Ok(c4_query::views(model)),
        Question::View { id } => c4_query::view(model, id),
    }
}

/// A question from the command line: `search orders --kind service`.
pub fn question(args: &[String]) -> Result<Question, String> {
    let mut words: Vec<&str> = Vec::new();
    let (mut kind, mut tag) = (None, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--kind" => kind = Some(it.next().ok_or("--kind needs a value")?.clone()),
            "--tag" => tag = Some(it.next().ok_or("--tag needs a value")?.clone()),
            word => words.push(word),
        }
    }
    let need = |n: usize, what: &str| -> Result<String, String> {
        words.get(n).map(|w| w.to_string()).ok_or(format!("{} needs {what}", words[0]))
    };
    let depth = |n: usize| -> Result<usize, String> {
        match words.get(n) {
            None => Ok(1),
            Some(text) => text.parse().map_err(|_| format!("'{text}' is not a number of steps")),
        }
    };
    Ok(match words.first().copied() {
        Some("overview") => Question::Overview,
        Some("search") => Question::Search { query: words[1..].join(" "), kind, tag, limit: 20 },
        Some("element") => Question::Element { name: need(1, "an element's name")? },
        Some("uses") => Question::Neighbors {
            name: need(1, "an element's name")?,
            direction: Direction::Out,
            depth: depth(2)?,
        },
        Some("used-by") => Question::Neighbors {
            name: need(1, "an element's name")?,
            direction: Direction::In,
            depth: depth(2)?,
        },
        Some("path") => Question::Path { from: need(1, "two elements")?, to: need(2, "two elements")? },
        Some("views") => Question::Views,
        Some("view") => Question::View { id: need(1, "a view's id")? },
        Some(other) => return Err(format!("unknown question '{other}' (try --help)")),
        None => {
            return Err("ask something: overview, search, element, uses, used-by, path, views or view".into());
        }
    })
}

/// What a client is told there is to call.
fn tools() -> Value {
    let name = json!({ "type": "string", "description": "An element's full name, like 'shop.api', as other answers give it. A short name works if only one element has it." });
    json!([
        {
            "name": "overview",
            "description": "Start here. What the architecture model contains: how much, of what kinds, and the top-level systems and people with a line on each.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "search",
            "description": "Find elements (systems, services, components, people, data stores) by words in their name, title, description, technology or tags; also finds relationships by what they say. Use it to turn a topic ('refunds', 'kafka') into element names.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Words to look for. May be empty when filtering by kind or tag alone." },
                    "kind": { "type": "string", "description": "Only elements of this kind, e.g. 'service'. The overview lists the kinds." },
                    "tag": { "type": "string", "description": "Only elements with this tag." },
                    "limit": { "type": "integer", "description": "How many to list (default 20)." }
                },
                "required": ["query"]
            }
        },
        {
            "name": "element",
            "description": "Everything about one element: description, technology, tags, the file and line it is written at (which says which repository owns it), what it contains, and each thing it uses and is used by.",
            "inputSchema": { "type": "object", "properties": { "name": name }, "required": ["name"] }
        },
        {
            "name": "neighbors",
            "description": "Follow dependencies out from an element, several steps if asked: what it uses (its dependencies), what uses it (what a change to it could break), or both. Followed at the level of the element named: for a service, in services.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": name,
                    "direction": { "type": "string", "enum": ["uses", "used_by", "both"], "description": "Default 'both'." },
                    "depth": { "type": "integer", "description": "How many steps to follow (default 1, at most 10)." }
                },
                "required": ["name"]
            }
        },
        {
            "name": "path",
            "description": "How one element reaches another: the shortest chain of relationships from one to the other, with what each step is.",
            "inputSchema": { "type": "object", "properties": { "from": name, "to": name }, "required": ["from", "to"] }
        },
        {
            "name": "views",
            "description": "The diagrams (views) the model defines, with their ids.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "view",
            "description": "One view as text: the elements it shows, nested as they are drawn, and the relationships between them. Every element with something inside it also has a view, named after it with '_' for '.'.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string", "description": "A view's id, from views." } },
                "required": ["id"]
            }
        }
    ])
}

/// A tool call as a question.
fn tool_question(name: &str, args: &Value) -> Result<Question, String> {
    let text = |key: &str| -> Result<String, String> {
        args[key].as_str().map(str::to_string).ok_or(format!("'{name}' needs '{key}', a string"))
    };
    let optional = |key: &str| args[key].as_str().filter(|s| !s.is_empty()).map(str::to_string);
    let number = |key: &str, default: usize| args[key].as_u64().map(|n| n as usize).unwrap_or(default);
    Ok(match name {
        "overview" => Question::Overview,
        "search" => Question::Search {
            query: args["query"].as_str().unwrap_or_default().to_string(),
            kind: optional("kind"),
            tag: optional("tag"),
            limit: number("limit", 20),
        },
        "element" => Question::Element { name: text("name")? },
        "neighbors" => Question::Neighbors {
            name: text("name")?,
            direction: match args["direction"].as_str().unwrap_or("both") {
                "uses" => Direction::Out,
                "used_by" => Direction::In,
                "both" => Direction::Both,
                other => return Err(format!("direction is 'uses', 'used_by' or 'both', not '{other}'")),
            },
            depth: number("depth", 1),
        },
        "path" => Question::Path { from: text("from")?, to: text("to")? },
        "views" => Question::Views,
        "view" => Question::View { id: text("id")? },
        other => return Err(format!("There is no tool '{other}'.")),
    })
}

pub struct Server {
    paths: Vec<PathBuf>,
    sources: Vec<Source>,
    workspace: Workspace,
}

impl Server {
    pub fn new(paths: Vec<PathBuf>) -> Server {
        let mut server = Server { paths, sources: Vec::new(), workspace: Workspace::default() };
        server.refresh();
        server
    }

    /// Reads the sources again, and the model if any of them changed. If
    /// they cannot be read just now (a file half written, a checkout in
    /// progress), the last model that could be stays.
    fn refresh(&mut self) {
        let Ok(sources) = crate::read_sources(&self.paths) else { return };
        let same = sources.len() == self.sources.len()
            && sources.iter().zip(&self.sources).all(|(a, b)| a.name == b.name && a.text == b.text);
        if !same {
            self.workspace = c4_model::load(&sources);
            self.sources = sources;
        }
    }

    /// One message in; the reply to send, if it wants one.
    pub fn handle(&mut self, line: &str) -> Option<String> {
        let message: Value = match serde_json::from_str(line) {
            Ok(message) => message,
            Err(e) => {
                let error = json!({ "code": -32700, "message": format!("not JSON: {e}") });
                return Some(json!({ "jsonrpc": "2.0", "id": null, "error": error }).to_string());
            }
        };
        // No id: a notification, which is never answered.
        let id = message.get("id").filter(|id| !id.is_null())?.clone();
        let params = &message["params"];
        let result = match message["method"].as_str().unwrap_or_default() {
            "initialize" => Ok(json!({
                // The version the client speaks, if it said; nothing used
                // here differs between them.
                "protocolVersion": params["protocolVersion"].as_str().unwrap_or("2025-06-18"),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "rustc4", "version": "0.1.0" },
                "instructions": "The architecture of the systems here, as a C4 model: people, systems, the services and components inside them, and what uses what. Ask it before assuming how services connect or which one owns something. Start with overview or search; every answer names elements by full name, which the other tools take."
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools() })),
            "tools/call" => {
                self.refresh();
                let empty = json!({});
                let args = if params["arguments"].is_object() { &params["arguments"] } else { &empty };
                let answered = tool_question(params["name"].as_str().unwrap_or_default(), args)
                    .and_then(|question| answer(&self.workspace, &question));
                // A question that cannot be answered is still an answer,
                // marked so the model reads it and asks better.
                let (text, failed) = match answered {
                    Ok(text) => (text, false),
                    Err(text) => (text, true),
                };
                Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": failed }))
            }
            other => Err(json!({ "code": -32601, "message": format!("no method '{other}'") })),
        };
        Some(
            match result {
                Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                Err(error) => json!({ "jsonrpc": "2.0", "id": id, "error": error }),
            }
            .to_string(),
        )
    }
}

/// Answers messages from `input` on `output` until `input` ends.
pub fn serve(paths: Vec<PathBuf>, input: &mut dyn BufRead, output: &mut dyn Write) {
    let mut server = Server::new(paths);
    let mut line = String::new();
    loop {
        line.clear();
        match input.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = server.handle(line.trim())
            && writeln!(output, "{reply}").and_then(|_| output.flush()).is_err()
        {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
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
            tools.iter().all(
                |t| t["inputSchema"]["type"] == "object" && t["description"].as_str().unwrap().len() > 20
            )
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
        assert!(!failed && text.starts_with("14 elements, 15 relationships, 5 views."), "{text}");
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
        assert_eq!(
            tool(&mut server, "element", json!({})),
            ("'element' needs 'name', a string".into(), true)
        );
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
}
