# Contributing to rustc4

Issues and pull requests are welcome. This is what building, testing and
releasing take; [README.md](README.md) is about using the result.

## Building

Everything is built by Bazel. Install
[Bazelisk](https://github.com/bazelbuild/bazelisk) as `bazel` (it reads
`.bazelversion`); the Rust, C++ and Node toolchains are downloaded by the
build, so on Linux nothing else needs installing; on macOS it also needs
Xcode, to link with. It is developed on macOS and tested on Linux. The first build
fetches and compiles those toolchains and takes a while; later ones are
quick.

```bash
bazel run //cli -- check examples/shop.c4   # the command line: anything `rustc4` takes, after the `--`
bazel run //cli -- serve examples/shop.c4   # a model on http://127.0.0.1:8080, redrawn on save
bazel run //apps/web:serve                  # the playground, on http://localhost:8080
```

The `rustc4` of a release is the CLI built with `--config=release`:

```bash
bazel build --config=release //cli   # bazel-bin/cli/cli
```

The Windows one is built from Linux or macOS, not on Windows:

```bash
bazel build --config=release --platforms=//bazel:windows_x86_64 //cli   # bazel-bin/cli/cli.exe
```

## Before sending a pull request

What CI checks:

```bash
bazel test //...                       # everything
bazel run @rules_rust//tools/rustfmt   # format the Rust
bazel build --aspects=@rules_rust//rust:defs.bzl%rust_clippy_aspect \
    --output_groups=clippy_checks //core/... //bindings/... //cli/...
```

`bazel test //core/...` is Rust only, and the fastest loop.

A change to the layout cannot be judged from tests alone: export the
examples as PNG and look at them.

```bash
bazel run //cli -- export png examples -o /tmp/out --background '#fff'
```

(`examples` holds separate models, so that reports what more than one of
them defines and exits 1; every view is still written.)

The rules the code keeps to (what belongs in Rust and what in a renderer,
why layout must be deterministic, what a new field touches) are in
[CLAUDE.md](CLAUDE.md), which is written for coding assistants and reads as
well for people.

## How it is put together

A Bazel monorepo where Rust owns all behaviour and front ends only draw. The
build and its toolchains descend from
[rust-core-experiment](https://github.com/dmashuda/rust-core-experiment),
whose `LOG.md` records what it took to make them work.

```
core/c4_model     Rust: the language (lexer, parser), the model, and computing
                  what a view shows. No dependencies.
core/c4_layout    Rust: a computed view -> a Diagram. Sizes boxes from their
                  text, lays them out, routes the lines. No font, no platform.
core/c4_render    Rust: a Diagram -> SVG text.
core/c4_ui        Rust: the viewer as a state machine. A Screen out, taps in.
core/c4_query     Rust: questions about a model, answered in text.
bindings/wasm     wasm-bindgen wrapper of c4_ui -> JS/TS (web)
apps/web          renderer: TypeScript + esbuild. Draws the Screen, pans and zooms.
cli               Rust: check, export, build (static site), bundle, query, mcp
examples          models to try (`bigbank.c4` is LikeC4's; see `examples/LICENSE.likec4`)
tools/, bazel/    toolchains
```

### One diagram, drawn the same everywhere

A `Diagram` (`core/c4_layout/src/diagram.rs`) is finished: every box has a
position and colors, every line of text has a baseline and has already been
wrapped, every relationship is a list of Bézier points with the outline of its
arrowhead. A renderer draws shapes, paths and text at the coordinates given.

That works without a font because text is measured from a table of
Helvetica's widths (`text.rs`), and renderers draw with a font of that family.
So the CLI and wasm in a browser break lines in the same place and produce
the same picture: the web draws the SVG Rust renders, the same the CLI
exports.

The layout (`graph.rs`) is a layered one written for this, since Graphviz is
not something that compiles to wasm and a phone: rank, order to reduce
crossings, place by solving separation constraints, route through the rows.
Groups are part of it throughout, so a box inside a system and a box outside
it can share a row and still never overlap the system's border.

### One screen, any renderer

`C4Ui::screen()` returns what is on the screen: the list of views, the
diagram, the panel about whatever is selected, the problems in the source.
A renderer draws that and reports `tap(id)` when something is activated and
`set_source(file, text)` when the source is edited. Which view shows, what a
click does and every word on the screen are decided in Rust, so adding to the
viewer changes Rust and nothing else. What stays with the platform is what
only it can do: panning and zooming, the address bar, saving a file.

Still to come here: **flows**, behaviour tests written once in Rust and run
through every renderer.

## Releasing

A release is a version tag on a commit that is on `main`:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

`.github/workflows/release.yml` checks the tag is on `main`, builds the
CLI for each platform, attests the archives and publishes them. The two
Linux builds also become an image (`tools/docker/Dockerfile`: the binary
and a font on a base with glibc and no shell), tried on each architecture
and pushed to `ghcr.io/dmashuda/rustc4` under the version and `latest`. A
tag with a suffix (`v0.2.0-rc.1`) is published as a pre-release, and leaves
`latest` alone. Run from the Actions tab instead, it builds the same
archives and images and publishes nothing, which is the way to try a change
to it.

To build the image by hand, unpack the Linux archives of a release as
`linux-amd64/rustc4` and `linux-arm64/rustc4` in one directory, and

```bash
docker build -f tools/docker/Dockerfile -t rustc4 <that directory>
```
