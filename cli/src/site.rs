//! A static site: the web viewer, the model it shows, and each view as an
//! SVG for linking to or embedding elsewhere. Any file server will do.

use std::path::Path;

use c4_model::{Source, Workspace};
use c4_render::SvgOptions;

/// The viewer, as built by //apps/web:viewer.
const VIEWER: [(&str, &[u8]); 4] = [
    ("index.html", include_bytes!(concat!(env!("VIEWER_DIR"), "/index.html"))),
    ("style.css", include_bytes!(concat!(env!("VIEWER_DIR"), "/style.css"))),
    ("main.js", include_bytes!(concat!(env!("VIEWER_DIR"), "/main.js"))),
    ("rustc4_bg.wasm", include_bytes!(concat!(env!("VIEWER_DIR"), "/rustc4_bg.wasm"))),
];

/// Names sources by where they are among themselves, not on this machine:
/// the common directory is dropped.
fn relative_names(sources: &[Source]) -> Vec<String> {
    let dirs: Vec<Vec<&str>> = sources
        .iter()
        .map(|s| {
            let mut parts: Vec<&str> = s.name.split(['/', '\\']).collect();
            parts.pop();
            parts
        })
        .collect();
    let mut common = dirs.first().cloned().unwrap_or_default();
    for dir in &dirs {
        let same = common.iter().zip(dir).take_while(|(a, b)| a == b).count();
        common.truncate(same);
    }
    sources
        .iter()
        .map(|s| s.name.split(['/', '\\']).skip(common.len()).collect::<Vec<_>>().join("/"))
        .collect()
}

/// The sources as one file: what the viewer loads, and what every command
/// here reads back in place of the files it was made from.
pub fn bundle(sources: &[Source]) -> Result<Vec<u8>, String> {
    let names = relative_names(sources);
    let model = serde_json::json!({
        "sources": sources
            .iter()
            .zip(&names)
            .map(|(s, name)| serde_json::json!({ "name": name, "text": s.text }))
            .collect::<Vec<_>>(),
    });
    serde_json::to_vec(&model).map_err(|e| e.to_string())
}

/// The sources in a bundle.
pub fn unbundle(bytes: &[u8]) -> Result<Vec<Source>, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| format!("not a bundle: {e}"))?;
    let sources = value["sources"].as_array().ok_or("not a bundle: it has no \"sources\"")?;
    sources
        .iter()
        .map(|s| match (s["name"].as_str(), s["text"].as_str()) {
            (Some(name), Some(text)) => Ok(Source { name: name.into(), text: text.into() }),
            _ => Err("not a bundle: a source needs a \"name\" and a \"text\"".to_string()),
        })
        .collect()
}

/// Writes the site into `dir`; returns how many files.
pub fn write(dir: &Path, sources: &[Source], ws: &Workspace) -> Result<usize, String> {
    let put = |name: &str, bytes: &[u8]| -> Result<(), String> {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
    };
    let mut count = 0;
    for (name, bytes) in VIEWER {
        put(name, bytes)?;
        count += 1;
    }
    // What the viewer loads: the sources themselves. It reads them with the
    // same code this does, so the site shows exactly what was checked here.
    put("model.json", &bundle(sources)?)?;
    count += 1;
    for id in ws.model.view_ids() {
        let view = ws.model.compute_view(id).expect("listed views exist");
        let diagram = c4_layout::layout(&view, &ws.model.colors);
        put(&format!("views/{id}.svg"), c4_render::svg(&diagram, &SvgOptions::default()).as_bytes())?;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
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
}
