//! wasm-bindgen surface over `c4_ui` for the web front end.
//!
//! The screen crosses as JSON, which the page parses into plain objects; the
//! diagram crosses as SVG text, the same the CLI exports. What the user did
//! comes back as the id of what they touched.

use c4_model::Source;
use c4_render::SvgOptions;
use wasm_bindgen::prelude::*;

/// The viewer.
#[wasm_bindgen]
pub struct C4Ui {
    inner: c4_ui::C4Ui,
}

#[derive(serde::Deserialize)]
struct SourceJson {
    name: String,
    text: String,
}

fn parse(sources: &str) -> Vec<Source> {
    let sources: Vec<SourceJson> = serde_json::from_str(sources).unwrap_or_default();
    sources.into_iter().map(|s| Source { name: s.name, text: s.text }).collect()
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("the screen is plain data")
}

#[wasm_bindgen]
impl C4Ui {
    /// `sources` is JSON: `[{"name": "shop.c4", "text": "model { ... }"}]`.
    /// `editable`: whether the screen offers the source for editing.
    ///
    /// Cannot fail: sources that cannot be read as JSON are no sources, and
    /// the screen says there is nothing to draw.
    #[wasm_bindgen(constructor)]
    pub fn new(sources: &str, editable: bool) -> C4Ui {
        C4Ui { inner: c4_ui::C4Ui::new(parse(sources), editable) }
    }

    /// What to draw around the diagram: a `Screen`, as JSON.
    pub fn screen(&self) -> String {
        json(&self.inner.screen())
    }

    /// The user activated the thing with this id; returns the screen.
    pub fn tap(&mut self, id: &str) -> String {
        json(&self.inner.tap(id))
    }

    /// The text of source file `file` changed; returns the screen.
    #[wasm_bindgen(js_name = setSource)]
    pub fn set_source(&mut self, file: usize, text: &str) -> String {
        json(&self.inner.set_source(file, text))
    }

    /// Other sources altogether, as the constructor takes them: the files
    /// changed where they are kept. What was showing stays if it still
    /// exists; returns the screen.
    #[wasm_bindgen(js_name = setSources)]
    pub fn set_sources(&mut self, sources: &str, editable: bool) -> String {
        json(&self.inner.set_sources(parse(sources), editable))
    }

    /// The sources as they stand, as the constructor takes them: for the
    /// page to keep between visits.
    pub fn sources(&self) -> String {
        let sources: Vec<_> = self
            .inner
            .sources()
            .iter()
            .map(|s| serde_json::json!({ "name": s.name, "text": s.text }))
            .collect();
        json(&sources)
    }

    /// The showing diagram as SVG, or an empty string when there is none.
    /// `background`: a color to paint behind it, for an export; `undefined`
    /// leaves it transparent.
    pub fn svg(&self, background: Option<String>) -> String {
        match self.inner.diagram() {
            Some(diagram) => c4_render::svg(diagram, &SvgOptions { background }),
            None => String::new(),
        }
    }
}
