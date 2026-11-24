// Loads the real wasm-bindgen output in Node and exercises the API, proving
// the Rust -> wasm -> JS path without a browser. What the viewer does is
// tested in //core/c4_ui; this is about what crosses the boundary and how.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import init, { C4Ui } from "./rustc4_wasm_bindgen/rustc4.js";

const wasm = await readFile(new URL("./rustc4_wasm_bindgen/rustc4_bg.wasm", import.meta.url));
await init({ module_or_path: wasm });

const text = await readFile(new URL("../../examples/shop.c4", import.meta.url), "utf8");
const sources = JSON.stringify([{ name: "shop.c4", text }]);

// The screen is JSON; the diagram is SVG.
const ui = new C4Ui(sources, true);
let screen = JSON.parse(ui.screen());
assert.equal(screen.title, "Landscape");
assert.equal(screen.view, "index");
assert.deepEqual(
  screen.views.map((v) => [v.id, v.text, v.selected]),
  [
    ["view.index", "Landscape", true],
    ["view.shop", "Inside the shop", false],
    ["view.api", "Inside the API", false],
    ["view.orders", "An order, end to end", false],
  ],
);
assert.equal(screen.diagram, undefined, "the diagram does not cross as JSON");
assert.equal(screen.editor.text, text);

let svg = ui.svg(undefined);
assert.match(svg, /^<svg xmlns="http:\/\/www\.w3\.org\/2000\/svg" viewBox="0 0 \d+ \d+"/);
assert.match(svg, /<g class="node" data-id="shop" data-kind="system" data-view="shop">/);
assert.ok(!svg.includes('<rect width='), "transparent unless asked");
assert.match(ui.svg("#ffffff"), /<rect width="\d+" height="\d+" fill="#ffffff"\/>/);

// A tap returns the next screen.
screen = JSON.parse(ui.tap("open.shop"));
assert.equal(screen.view, "shop");
assert.equal(screen.back.id, "back");
assert.notEqual(ui.svg(undefined), svg);
screen = JSON.parse(ui.tap("node.shop.api"));
assert.equal(screen.selected, "node.shop.api");
assert.equal(screen.details.title, "API");

// An edit returns the screen too, problems and all.
screen = JSON.parse(ui.setSource(0, text.replace("customer -> shop.web", "customer -> shop.webb")));
assert.equal(screen.problems.length, 1);
assert.equal(screen.problems[0].text, "There is no element 'shop.webb'.");
assert.ok(ui.svg(undefined).startsWith("<svg"), "what could be understood is still drawn");

// Files can be added, and the sources come back out to be kept.
screen = JSON.parse(ui.tap("file.add"));
assert.deepEqual(screen.editor.files.map((f) => f.text), ["shop.c4", "file-2.c4"]);
assert.equal(screen.editor.file, 1);
ui.setSource(1, "// more");
assert.deepEqual(JSON.parse(ui.sources()).map((s) => s.name), ["shop.c4", "file-2.c4"]);
assert.equal(JSON.parse(ui.sources())[1].text, "// more");
assert.equal(JSON.parse(ui.tap("file.remove")).editor.files.length, 1);

// Sources that are not JSON are no sources.
const empty = new C4Ui("not json", false);
screen = JSON.parse(empty.screen());
assert.equal(screen.views.length, 0);
assert.equal(empty.svg(undefined), "");
assert.equal(screen.editor, null);
assert.match(screen.empty, /^Nothing to draw yet/);

console.log("ok");
