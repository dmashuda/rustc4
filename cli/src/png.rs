//! SVG to PNG, with resvg. The one place a real font is needed: text is
//! drawn with whichever of the fonts the diagram asks for the machine has.

use resvg::{tiny_skia, usvg};

pub struct Renderer {
    options: usvg::Options<'static>,
}

impl Renderer {
    pub fn new() -> Renderer {
        let mut options = usvg::Options::default();
        let fonts = options.fontdb_mut();
        fonts.load_system_fonts();
        // What "sans-serif" means if none of the named fonts is installed.
        for family in ["Helvetica", "Arial", "Liberation Sans", "DejaVu Sans", "Noto Sans"] {
            let query =
                usvg::fontdb::Query { families: &[usvg::fontdb::Family::Name(family)], ..Default::default() };
            if fonts.query(&query).is_some() {
                fonts.set_sans_serif_family(family);
                break;
            }
        }
        Renderer { options }
    }

    pub fn render(&self, svg: &str, scale: f32) -> Result<Vec<u8>, String> {
        let tree =
            usvg::Tree::from_str(svg, &self.options).map_err(|e| format!("could not read the SVG: {e}"))?;
        let size = tree.size();
        let (w, h) = ((size.width() * scale).ceil() as u32, (size.height() * scale).ceil() as u32);
        let mut pixmap =
            tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or("the diagram is too large for a PNG")?;
        resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
        pixmap.encode_png().map_err(|e| e.to_string())
    }
}
