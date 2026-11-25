# CLAUDE.md

Guidance for Claude Code when working in this repository.

## Build & test

Everything goes through Bazel (bazelisk reads `.bazelversion`).

```bash
bazel test //...                       # all tests
bazel test //core/...                  # Rust only, fastest loop
bazel run @rules_rust//tools/rustfmt   # format Rust
bazel build --aspects=@rules_rust//rust:defs.bzl%rust_clippy_aspect \
    --output_groups=clippy_checks //core/... //bindings/... //cli/...
bazel run //apps/web:serve             # the playground on :8080
bazel run //apps/macos:rustc4 -- examples/shop.c4   # the macOS app, following that file
bazel run //cli -- serve examples/shop.c4   # a model on :8080, redrawn on save
bazel run //cli -- export png examples -o /tmp/out --background '#fff'
```

After every change, `bazel test //...` must pass, Rust must be rustfmt-clean,
and clippy must be quiet.

A layout change cannot be judged from tests alone: export the examples as
PNG and look at them. (`examples` holds separate models, so that command
reports what more than one of them defines and exits 1; every view is
still written. `examples/delivery` is the one with icons, sizes and
stacks; `examples/library` the one with every kind of line.)

## Architecture rules

One Rust core, thin bindings, renderers that only draw.

- **All behaviour lives in Rust.** The language and what a view shows:
  `core/c4_model`. Sizes, positions, colors, line breaks, curves:
  `core/c4_layout`. What is on the viewer's screen and what a tap does:
  `core/c4_ui`.
- **A `Diagram` is finished.** Renderers draw it at the coordinates given
  and decide nothing: no wrapping text, no choosing colors, no routing. If a
  renderer needs to work something out about the drawing, that belongs in
  `c4_layout` and in the `Diagram`.
- **Renderers in `apps/` are generic.** They draw the `Screen` and call
  `tap(id)` / `set_source(file, text)`. A renderer must not contain
  user-facing text, branch on an id's meaning beyond the kinds documented at
  the top of `core/c4_ui/src/lib.rs`, or hold state about what is showing.
  What stays platform-side: styling of the chrome, pan and zoom, the address
  bar, where the playground's source is kept, turning a diagram into a file.
- **Questions about the model live in `core/c4_query`** and answer in text;
  `rustc4 query` and the MCP server (`cli/src/mcp.rs`) are two ways of
  asking the same ones. A new MCP tool is a function there, a `Question`
  variant, and an entry in `tools()`. Answers stay short (`MAX_LINES`) and
  name every element by its full name.
- **A model may come from many repositories.** Nothing may assume one file
  or one specification: names and view ids are global, identical kind
  specifications may repeat, and every element and relationship keeps its
  `Origin`. `Model::views` are the written views; `generated_views` (one per
  element with children and no written view) are reachable but not listed.
- **A dynamic view shows its steps**, not the model's relationships: each
  step is its own line, numbered in its label (`3`, `3.1` inside
  `parallel`), drawn by the same layout. A step takes its words and looks
  from the relationship the model has between its two ends unless it says
  otherwise; one written backwards (`a <- b`) takes nothing. A step an
  element takes alone (`a -> a`) is a loop out of its side, with room
  kept for its words.
- **A box says its `summary`**, or its description if it has none; the
  details panel gives the description (the summary if there is none), and
  `describe` gives both.
- **`variant sequence`** is the same steps laid out by
  `c4_layout/src/sequence.rs` instead of `graph.rs`: the steps' ends side
  by side in the order they first appear (no groups, nobody who only
  stands by), a `Lifeline` down from each, one level line per step.
  Lifelines are in the `Diagram`, drawn behind everything, and are not
  tapped or faded.
- **An icon is a URL in the `Diagram`**, with the rectangle to draw it
  in: `https:` for a renderer to fetch, or `data:` holding an image file
  the source named. Such a file is one more `Source` (named `.svg`, or
  `.png` / `.jpg` / `.gif` / `.webp` with its bytes in base64, which
  `Source::from_bytes` does; not read as a model), as is the text of a
  `descriptionFile` (`.md`, `.txt`);
  `Workspace::wanted` (`C4Ui::wanted`) lists those not given, for whoever
  reads files to read and give: the CLI and the macOS app do. The core
  never touches the file system.
- **A legend is part of the `Diagram`** (`Diagram::legend`), over the
  drawing and inside its width and height, whenever the view shows
  something whose kind has a `notation`, unless it is turned off
  (`c4_layout::Options`, `--no-legend`, the `legend` tap). Its samples are ordinary `Node`s
  and `Edge`s with empty ids, drawn by the same code as the diagram's and
  never tapped, selected or faded.
- **Ids are API.** `view.<id>`, `node.<fqn>`, `open.<fqn>`,
  `edge.<source>-><target>` (a step of a dynamic view:
  `edge.<source>-><target>#<step>`), `back`, `legend`, `canvas`, `details.close`, `file.<n>`,
  `file.add`, `file.remove`.
  Tests address things by them.
- `c4_model` has no dependencies. `c4_layout` adds only serde (the Diagram
  serializes itself). Neither may depend on a font, a clock, the file system
  or anything platform-specific: both compile to wasm and will compile to
  iOS and Android.
- Core crates are visible only to `//core`, `//bindings` and `//cli`. Apps
  reach Rust through the bindings, never directly.
- **Text is measured from the table in `c4_layout/src/text.rs`**, never from
  a real font, so every target wraps identically. Renderers must draw with
  `Diagram::font_family`.
- **Layout is deterministic.** Same source, same diagram, on every target:
  no hash maps iterated, no randomness, no threads. Tests compare diagrams
  for equality.
- **The language is LikeC4's.** A construct that LikeC4 accepts and this
  does not support is reported as a warning ("not supported yet") and
  skipped, never a silent difference in the drawing. New syntax should
  follow LikeC4's unless there is a reason written down.
- **Errors never stop the drawing.** The parser recovers at the next line,
  the model leaves out what it could not resolve, and whatever is left is
  drawn. Every diagnostic has a file, line and column and says what to do.
- **Tests are not written in `src/`.** The unit tests of `src/<file>.rs`
  are `tests/<file>.rs` in the same crate, which the source file names at
  its end (`#[cfg(test)] #[path = "../tests/<file>.rs"] mod tests;`), so
  they still see what is private. Something only tests need goes in the
  test file too.
- A new field on `Screen` or `Diagram` touches: the Rust struct, the remote
  declaration in `bindings/uniffi/src/lib.rs` (the compiler catches a
  mismatch), the type in `apps/web/src/main.ts`, and for `Diagram` both
  drawings of it: `core/c4_render` (SVG) and
  `apps/apple/C4UI/Sources/DiagramDrawing.swift`. Those two must stay shape
  for shape the same. The web gets the screen as JSON, so `bindings/wasm`
  does not change. UniFFI has no `usize`: use `u32`.
- Several of the bindings' names are also SwiftUI's (`Label`, `Edge`,
  `Section`, `Link`, `Shape`, `Anchor`): in Swift write `C4FFI.Label` or
  `SwiftUI.Label`.
- Apple tests (`//bindings/uniffi:C4FFITests`, `//apps/apple/C4UI:C4UITests`)
  only run on a Mac; CI is Linux and skips them. `C4UITests` leaves a picture
  of the window in `bazel-testlogs/apps/apple/C4UI/C4UITests/test.outputs/`:
  look at it after changing the Swift views.
- The macOS app has no editor: it shows files and follows them
  (`C4Model.open`, looking four times a second as `serve` does) and
  remembers what was open for the next launch. Only the web playground
  draws `Screen::editor`.
- The CLI embeds `//apps/web:viewer` (`cli/BUILD.bazel`); `rustc4 build`
  writes it beside `model.json`. The viewer decides between showing a
  published model and being the playground by whether `model.json` is there.
- `rustc4 serve` (`cli/src/serve.rs`) is that site from memory, std only:
  it looks at the files four times a second, and `model.json` comes with an
  `x-rustc4-changes` header naming an event stream; the viewer listens there
  and, on an event, fetches the model again and calls `setSources`. It
  listens on 127.0.0.1 unless `--host` or `RUSTC4_HOST` says otherwise.
- The release's image (`tools/docker/Dockerfile`) compiles nothing: it is
  the released Linux binary and a font for PNGs on a base with only glibc.
  It sets `RUSTC4_HOST=0.0.0.0`. Something new the binary needs at run time
  (a library, a file) has to be put there too.

## Layout, in brief

`core/c4_layout/src/graph.rs`, in the order it runs: break cycles, rank
(longest path, then shorten), add a placeholder per row for each long line
(one carries the label), order rows by barycenter keeping groups together
and sibling groups in one order everywhere, then swap two neighbours of one
group wherever that crosses fewer lines, place x by iterating "move
toward neighbours, then project onto the separation constraints" (`project`
is a small VPSC), place y with room for group borders, route through the
placeholders, then straighten: between the corners a line must keep (its
ends, and beside its label) it leaves out every placeholder it can without
coming near a box, a label or a group it is not in; its label moves with
it when there is room clear of every other line. Everything is computed top-to-bottom and turned at the end.

A view's `rank` blocks come in as sets of boxes that rank as one box
(`Graph::set`); the set held at the first row has every line turned to
leave it, the one at the last to arrive. A line between two boxes of one
set gets a single placeholder in the row below and goes round under them.

Groups are not nodes: a group is a pair of border variables in the x
constraints and a span of rows. A line to a group as a whole ends at an
invisible anchor inside it and is clipped at the border.

## Origin

Build architecture and toolchains descend from rust-core-experiment
(github.com/dmashuda/rust-core-experiment), whose LOG.md documents the
toolchain gotchas. The product is modelled on LikeC4; `examples/bigbank.c4`
is adapted from it, under the license beside it.
