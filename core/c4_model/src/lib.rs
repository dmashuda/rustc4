//! The architecture model and its language.
//!
//! Source files (the LikeC4 dialect: `specification`, `model`, `views`) are
//! read into one [`Model`] by [`load`], and [`Model::compute_view`] works out
//! what a view shows. Nothing here knows how a diagram is laid out or drawn,
//! and nothing here depends on another crate.

mod ast;
mod lexer;
mod model;
mod parser;
mod view;

pub use ast::Direction;
pub use model::{
    Arrow, Diagnostic, Element, ElementStyle, Line, Model, Origin, Relation, RelationStyle, Severity, Shape,
    Source, View, Workspace, load,
};
pub use view::{ComputedView, ViewEdge, ViewNode};

/// `#rrggbb` or `#rgb` as red, green and blue.
pub fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    model::parse_hex(value)
}
