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
bazel run //apps/macos:rustc4          # the macOS app
bazel run //cli -- export png examples -o /tmp/out --background '#fff'
```

After every change, `bazel test //...` must pass, Rust must be rustfmt-clean,
and clippy must be quiet.

A layout change cannot be judged from tests alone: export the examples as
PNG and look at them.

## Architecture rules

The structure is med-log's (github.com/dmashuda/med-log): one Rust core,
thin bindings, renderers that only draw.

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
- **Ids are API.** `view.<id>`, `node.<fqn>`, `open.<fqn>`,
  `edge.<source>-><target>`, `back`, `canvas`, `details.close`, `file.<n>`.
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
- The CLI embeds `//apps/web:viewer` (`cli/BUILD.bazel`); `rustc4 build`
  writes it beside `model.json`. The viewer decides between showing a
  published model and being the playground by whether `model.json` is there.

## Layout, in brief

`core/c4_layout/src/graph.rs`, in the order it runs: break cycles, rank
(longest path, then shorten), add a placeholder per row for each long line
(one carries the label), order rows by barycenter keeping groups together
and sibling groups in one order everywhere, place x by iterating "move
toward neighbours, then project onto the separation constraints" (`project`
is a small VPSC), place y with room for group borders, route through the
placeholders. Everything is computed top-to-bottom and turned at the end.

Groups are not nodes: a group is a pair of border variables in the x
constraints and a span of rows. A line to a group as a whole ends at an
invisible anchor inside it and is clipped at the border.

## Origin

Build architecture and toolchains copied from med-log (and through it,
rust-core-experiment, whose LOG.md documents the toolchain gotchas). The
product is modelled on LikeC4.
