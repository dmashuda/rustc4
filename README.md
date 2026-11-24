# rustc4

Architecture diagrams as code, in the [C4](https://c4model.com) style: describe
a system once, as a model, and get every diagram of it from that. It reads the
[LikeC4](https://likec4.dev) language and does what LikeC4 does (an
interactive viewer, images, a static site), with everything that matters
written once, in Rust, and compiled to each place it runs.

![The example shop, from the inside](docs/shop.svg)

```
specification {
  element actor { style { shape person; color green } }
  element system
  element service
}

model {
  customer = actor 'Customer'
  shop = system 'Shop' {
    web = service 'Storefront' 'What a customer sees' 'React' { style { shape browser } }
    api = service 'API' 'One front door' 'Rust'
    web -> api 'calls' 'HTTPS'
  }
  customer -> shop.web 'shops with'
}

views {
  view index { include * }          // customer -> shop
  view shop of shop { include * }   // inside the shop, and who uses it
}
```

## Using it

Everything is built by Bazel (bazelisk reads `.bazelversion`); nothing else
needs installing.

```bash
bazel run //apps/web:serve                 # the playground, on http://localhost:8080
bazel run //apps/macos:rustc4              # the macOS app (needs Xcode)
bazel run //cli -- check examples          # report problems in a model
bazel run //cli -- export svg examples -o out      # every view as an image
bazel run //cli -- export png examples -o out --view shop --scale 2
bazel run //cli -- build examples -o dist  # a static site: viewer + model + SVGs
bazel test //...
```

`bazel build --config=release //cli` gives a single `rustc4` binary with the
web viewer inside it: `rustc4 build` needs nothing else to write a site, and
any file server can host the result.

In the viewer: click a box or a line to see what it is and what it connects
to, double-click a box with a magnifying glass (or click the glass) to go to
the closer view of it, drag to move, pinch or ctrl-scroll to zoom. The address
bar follows the view, so a link opens on it. The playground has the source
beside the diagram and redraws as you type.

The macOS app is the same viewer, native. It opens on the example with its
source to edit; **Open…** (⌘O), dropping a folder on the window, or a path on
the command line shows a model from disk instead, and the window then follows
the files as you save them from whatever editor you use.

## One model from many repositories

Keep each system's `.c4` files in its own repository, and read them all as
one model. Every command takes several paths, so with the repositories
checked out side by side:

```bash
rustc4 check  repos/*/architecture              # one model, checked as a whole
rustc4 bundle repos/*/architecture -o org.json  # ...gathered into one file
rustc4 build  org.json -o site                  # ...and a site of all of it
```

A bundle is the sources in one file; every command reads one wherever it
reads a directory. What makes the pieces fit:

- **Names are global.** `payments.api -> ledger.api` in one repository means
  the `ledger.api` another defines. Two repositories defining the same
  element is an error naming both files; adding to another's element is
  `extend ledger { ... }`.
- **Specifications may repeat.** Each repository can carry the element kinds
  it is checked against; the same kind specified the same way twice is fine,
  two that disagree is an error.
- **Everything remembers where it was written**, file and line, so from any
  box there is a way back to the repository that owns it.
- **View ids are global too**, so only one place should define `index`.

Every element with something inside it can be gone into, whether or not
anyone wrote a view of it: the landscape shows the top level, a click on a
system shows its services, a click on a service its components. Selecting a
box fades everything it is not connected to, which is what makes a crowded
view readable.

A hundred services side by side at the top level lay out in milliseconds,
but no drawing of a hundred boxes and their lines is easy to read. Group them
(`payments = domain { ledger = service ... }`): the landscape then shows a
dozen domains, each a click away from its services.

## For language models: MCP

`rustc4 mcp <path>...` serves the model over the
[Model Context Protocol](https://modelcontextprotocol.io) on stdio, so an
assistant can ask what exists and what depends on what instead of guessing.
It re-reads the files when they change.

```bash
claude mcp add architecture -- /path/to/rustc4 mcp /path/to/org.json
```

| Tool | Answers |
| --- | --- |
| `overview` | What is in the model: counts, kinds, the top level |
| `search` | "What handles refunds?" Elements by name, description, technology, tag; relationships by what they say |
| `element` | One element in full: what it is, the file it is written in, what it contains, uses and is used by |
| `neighbors` | Dependencies out to several steps, either way: "what breaks if this changes?" |
| `path` | "How does the mobile app reach the ledger?" |
| `views`, `view` | A diagram as text: its boxes, nested, and its lines |

Answers are short text that names every element in full, so one answer leads
to the next question. The same questions work at the command line:
`rustc4 query org.json -- used-by ledger.api 3`.

## How it is put together

The architecture is [med-log](https://github.com/dmashuda/med-log)'s: a Bazel
monorepo where Rust owns all behaviour and front ends only draw.

```
core/c4_model     Rust: the language (lexer, parser), the model, and computing
                  what a view shows. No dependencies.
core/c4_layout    Rust: a computed view -> a Diagram. Sizes boxes from their
                  text, lays them out, routes the lines. No font, no platform.
core/c4_render    Rust: a Diagram -> SVG text.
core/c4_ui        Rust: the viewer as a state machine. A Screen out, taps in.
core/c4_query     Rust: questions about a model, answered in text.
bindings/wasm     wasm-bindgen wrapper of c4_ui -> JS/TS (web)
bindings/uniffi   UniFFI wrapper of c4_ui      -> Swift (Apple)
apps/web          renderer: TypeScript + esbuild. Draws the Screen, pans and zooms.
apps/apple/C4UI   renderer: SwiftUI. Draws the Screen, and the Diagram in a Canvas.
apps/macos        macos_application
cli               Rust: check, export, build (static site), bundle, query, mcp
examples          models to try
tools/, bazel/    toolchains, from med-log
```

### One diagram, drawn the same everywhere

A `Diagram` (`core/c4_layout/src/diagram.rs`) is finished: every box has a
position and colors, every line of text has a baseline and has already been
wrapped, every relationship is a list of Bézier points with the outline of its
arrowhead. A renderer draws shapes, paths and text at the coordinates given.

That works without a font because text is measured from a table of
Helvetica's widths (`text.rs`), and renderers draw with a font of that family.
So the CLI, wasm in a browser and the native app all break lines in the same
place and produce the same picture. The web draws the SVG Rust renders; the
macOS app draws the `Diagram` itself with SwiftUI's `Canvas`, shape for shape
what `c4_render` writes.

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

### The language

The LikeC4 dialect, most of it:

| | |
| --- | --- |
| `specification` | `element` kinds with `style`, `relationship` kinds, `tag`, `color name #hex` |
| `model` | elements (`name = kind 'Title'` or `kind name 'Title'`), nested to any depth; `title`, `description`, `technology`, `#tags`, `link`, `metadata`, `style`; relationships `a -> b 'title' 'technology'`, `-> b` from inside an element, `a -[kind]-> b`, with `line`, `color`, `head`, `tail`; `extend a.b { }` |
| `views` | `view id of element`, `view b extends a`, `title`, `description`, `include` / `exclude` of `*`, `a.b`, `a.*`, `a.**`, `a._`, `element.kind = k`, `element.tag = #t`, `-> a`, `a ->`, `-> a ->`, `a -> b`; `include a with { color red }`; `style targets { }`; `autoLayout TopBottom / LeftRight / BottomTop / RightLeft` |
| Styles | shapes `rectangle person browser mobile cylinder storage queue`; the theme colors or your own; `border`, `opacity` |

Sources may be split over any number of `.c4` (or `.likec4`) files; they are
read as one model. A mistake is reported with its file, line and column, and
everything that could be understood is still drawn.

Not there yet: dynamic and deployment views, `where` conditions, `global`
blocks, icons, and manual layout. They are reported as unsupported rather
than silently ignored.

## What is next

- **iOS and Android.** `apps/apple/C4UI` is written to be shared with an iOS
  app; Android needs the Kotlin half of the UniFFI binding and a renderer,
  as med-log has.
- **A bundled font for PNG export**, so the CLI's PNGs do not depend on what
  the machine has installed (they use Helvetica, Arial or the nearest
  sans-serif found).
- **Flows**: behaviour tests written once in Rust and run through every
  renderer, as med-log's are.
- `rustc4 serve` with live reload, and a language server.
