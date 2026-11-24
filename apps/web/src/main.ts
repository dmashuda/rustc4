// Renderer only: draws the Rust Screen and reports what the user touched.
// Which view is showing, what a click on a box does, what the panels say
// and how the diagram looks all come from //core. What is here besides
// drawing: panning and zooming, where the source comes from and is kept,
// the address bar, and turning a diagram into a downloaded file.
import init, { C4Ui } from "../../../bindings/wasm/rustc4_wasm_bindgen/rustc4.js";

// The .wasm is copied next to main.js by //apps/web:site.
await init({ module_or_path: new URL("./rustc4_bg.wasm", import.meta.url) });

// ------------------------------------------------------------------ types --
// The shape of c4_ui's Screen as JSON; see core/c4_ui/src/lib.rs.

type Item = { id: string; text: string; detail: string; hint: string; selected: boolean; enabled: boolean };
type Section = { title: string; rows: Item[] };
type Details = {
  title: string;
  subtitle: string;
  technology: string;
  description: string;
  tags: string[];
  links: { url: string; text: string }[];
  actions: Item[];
  sections: Section[];
  close: Item;
};
type Problem = { file: string; line: number; col: number; error: boolean; text: string };
type Editor = { title: string; files: Item[]; file: number; text: string };
type Export = { kind: "Svg" | "Png"; text: string; hint: string; fileName: string };
type Screen = {
  title: string;
  description: string;
  view: string;
  viewsTitle: string;
  views: Item[];
  back: Item | null;
  empty: string;
  selected: string;
  details: Details | null;
  exports: Export[];
  problemsTitle: string;
  problems: Problem[];
  editor: Editor | null;
  status: string;
  controls: { zoomIn: string; zoomOut: string; fit: string };
};
type Source = { name: string; text: string };

// ---------------------------------------------------------------- sources --
// A published site has its model beside the page (model.json, written by
// `rustc4 build`) and only shows it. Without one this is the playground:
// the source is edited here and kept in the browser.

const STORE_KEY = "rustc4.playground.v1";

function stored(): Source[] | null {
  try {
    const saved = JSON.parse(localStorage.getItem(STORE_KEY) ?? "null");
    if (Array.isArray(saved) && saved.every((s) => typeof s?.name === "string" && typeof s?.text === "string")) {
      return saved;
    }
  } catch {
    // Unreadable or unavailable storage: start from the example.
  }
  return null;
}

function store(sources: Source[]) {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(sources));
  } catch {
    // A private window: the edit still shows, it just is not kept.
  }
}

async function loadSources(): Promise<{ sources: Source[]; editable: boolean }> {
  try {
    const response = await fetch("model.json");
    if (response.ok) {
      const model = await response.json();
      if (Array.isArray(model?.sources)) return { sources: model.sources, editable: false };
    }
  } catch {
    // No model here: the playground.
  }
  const saved = stored();
  if (saved) return { sources: saved, editable: true };
  let text = "";
  try {
    const response = await fetch("example.c4");
    if (response.ok) text = await response.text();
  } catch {
    // Start empty; the screen says what to type.
  }
  return { sources: [{ name: "model.c4", text }], editable: true };
}

const { sources, editable } = await loadSources();
const ui = new C4Ui(JSON.stringify(sources), editable);

// -------------------------------------------------------------------- dom --

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const app = $("app");
const canvas = $("canvas");
const world = $("world");
const source = $<HTMLTextAreaElement>("source");

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  props: Partial<HTMLElementTagNameMap[K]> & { data?: Record<string, string> } = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  const { data, ...rest } = props;
  Object.assign(node, rest);
  for (const [key, value] of Object.entries(data ?? {})) node.dataset[key] = value;
  node.append(...children);
  return node;
}

/// A button for an Item: tapping it is all it does.
function button(item: Item, className = ""): HTMLButtonElement {
  const node = el("button", { type: "button", className, textContent: item.text, title: item.hint, disabled: !item.enabled });
  node.dataset.id = item.id;
  node.dataset.selected = String(item.selected);
  node.addEventListener("click", () => tap(item.id));
  return node;
}

// ------------------------------------------------------------ pan and zoom --
// The drawing sits in `world`, moved and scaled over the canvas. None of
// this reaches Rust: it changes how the diagram is looked at, not what it is.

const view = { x: 0, y: 0, k: 1 };
let diagramSize = { w: 0, h: 0 };

function applyView() {
  world.style.transform = `translate(${view.x}px, ${view.y}px) scale(${view.k})`;
}

function fit() {
  const { clientWidth: cw, clientHeight: ch } = canvas;
  if (!diagramSize.w || !cw || !ch) return;
  // Whole diagram in view, never blown up past its natural size.
  view.k = Math.min(1, (cw - 32) / diagramSize.w, (ch - 32) / diagramSize.h);
  view.x = (cw - diagramSize.w * view.k) / 2;
  view.y = (ch - diagramSize.h * view.k) / 2;
  applyView();
}

function zoomAt(factor: number, cx: number, cy: number) {
  const k = Math.min(4, Math.max(0.1, view.k * factor));
  view.x = cx - ((cx - view.x) * k) / view.k;
  view.y = cy - ((cy - view.y) * k) / view.k;
  view.k = k;
  applyView();
}

canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    const box = canvas.getBoundingClientRect();
    if (event.ctrlKey || event.metaKey) {
      // A pinch arrives as a wheel with ctrl held.
      zoomAt(Math.exp(-event.deltaY * 0.01), event.clientX - box.left, event.clientY - box.top);
    } else {
      view.x -= event.deltaX;
      view.y -= event.deltaY;
      applyView();
    }
  },
  { passive: false },
);

// Dragging moves the drawing; a press that goes nowhere is a click on
// whatever is under it.
let drag: { id: number; x: number; y: number; moved: boolean } | null = null;

canvas.addEventListener("pointerdown", (event) => {
  if (event.button !== 0) return;
  drag = { id: event.pointerId, x: event.clientX, y: event.clientY, moved: false };
});

canvas.addEventListener("pointermove", (event) => {
  if (!drag || drag.id !== event.pointerId) return;
  const [dx, dy] = [event.clientX - drag.x, event.clientY - drag.y];
  if (!drag.moved && Math.hypot(dx, dy) < 4) return;
  if (!drag.moved) {
    drag.moved = true;
    canvas.setPointerCapture(event.pointerId);
    canvas.dataset.dragging = "true";
  }
  view.x += dx;
  view.y += dy;
  drag.x = event.clientX;
  drag.y = event.clientY;
  applyView();
});

/// The id to tap for what was clicked in the drawing.
function targetId(target: EventTarget | null, opening: boolean): string {
  const hit = target instanceof Element ? target.closest<SVGGElement>("g[data-id]") : null;
  if (!hit || !world.contains(hit)) return "canvas";
  const id = hit.dataset.id ?? "";
  if (hit.classList.contains("node")) {
    const wantsOpen = opening || (target as Element).closest(".open") !== null;
    return wantsOpen && hit.dataset.view ? `open.${id}` : `node.${id}`;
  }
  return `edge.${id}`;
}

function endDrag(event: PointerEvent) {
  if (!drag || drag.id !== event.pointerId) return;
  const moved = drag.moved;
  drag = null;
  delete canvas.dataset.dragging;
  if (!moved && event.type === "pointerup") tap(targetId(event.target, false));
}
canvas.addEventListener("pointerup", endDrag);
canvas.addEventListener("pointercancel", endDrag);

canvas.addEventListener("dblclick", (event) => {
  const id = targetId(event.target, true);
  if (id.startsWith("open.")) tap(id);
});

new ResizeObserver(() => {
  // Keep a diagram that was fitted fitted; leave one the user moved alone.
  if (!userMoved) fit();
}).observe(canvas);
let userMoved = false;
for (const type of ["wheel", "pointermove"] as const) {
  canvas.addEventListener(type, () => (userMoved = userMoved || type === "wheel" || !!drag?.moved), { passive: true });
}

// ----------------------------------------------------------------- export --

function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const link = el("a", { href: url, download: name });
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

async function exportDiagram(kind: Export["kind"], name: string) {
  const svg = ui.svg(undefined);
  if (!svg) return;
  const svgBlob = new Blob([svg], { type: "image/svg+xml" });
  if (kind === "Svg") return download(svgBlob, name);
  // The browser rasterizes the same SVG, at twice the size for sharp text.
  const url = URL.createObjectURL(svgBlob);
  try {
    const image = new Image();
    await new Promise((resolve, reject) => {
      image.onload = resolve;
      image.onerror = reject;
      image.src = url;
    });
    const scale = 2;
    const bitmap = el("canvas", { width: diagramSize.w * scale, height: diagramSize.h * scale });
    bitmap.getContext("2d")?.drawImage(image, 0, 0, bitmap.width, bitmap.height);
    bitmap.toBlob((png) => png && download(png, name), "image/png");
  } finally {
    URL.revokeObjectURL(url);
  }
}

// ------------------------------------------------------------------- draw --

let shownSvg = "";
let shownView: string | null = null;
let shownFile = -1;

function drawDetails(details: Details | null) {
  const panel = $("details");
  panel.hidden = !details;
  if (!details) return panel.replaceChildren();
  const close = button(details.close, "close");
  const parts: Node[] = [
    el("div", { className: "top" }, el("h2", { textContent: details.title }), close),
    el("p", { className: "subtitle", textContent: details.subtitle }),
  ];
  if (details.technology) parts.push(el("p", { className: "technology", textContent: details.technology }));
  if (details.description) parts.push(el("p", { className: "description", textContent: details.description }));
  if (details.tags.length) {
    parts.push(el("div", { className: "tags" }, ...details.tags.map((tag) => el("span", { textContent: `#${tag}` }))));
  }
  if (details.links.length) {
    const links = details.links.map((link) =>
      el("a", { href: link.url, textContent: link.text, target: "_blank", rel: "noreferrer noopener" }),
    );
    parts.push(el("div", { className: "links" }, ...links));
  }
  for (const action of details.actions) parts.push(button(action, "action"));
  for (const section of details.sections) {
    parts.push(el("h3", { textContent: section.title }));
    for (const row of section.rows) {
      const body = [row.text, ...(row.detail ? [el("small", { textContent: row.detail })] : [])];
      // A row with no id is something to read, not to tap.
      const node = row.id ? button({ ...row, text: "" }, "row") : el("div", { className: "row", title: row.hint });
      node.append(...body);
      parts.push(node);
    }
  }
  panel.replaceChildren(...parts);
}

function drawEditor(editor: Editor | null, screen: Screen) {
  const pane = $("edit");
  pane.hidden = !editor;
  app.dataset.editing = String(!!editor);
  if (!editor) return;
  $("edit-title").textContent = editor.title;
  $("files").replaceChildren(...(editor.files.length > 1 ? editor.files.map((file) => button(file)) : []));
  // Only when another file is chosen: what is being typed is not replaced.
  if (shownFile !== editor.file) {
    shownFile = editor.file;
    source.value = editor.text;
  }
  const problems = $("problems");
  problems.hidden = screen.problems.length === 0;
  problems.replaceChildren(
    el("h3", { textContent: screen.problemsTitle }),
    ...screen.problems.map((problem) => {
      const where = el("span", { className: "where", textContent: `${problem.line}:${problem.col}` });
      const row = el("button", { type: "button", title: problem.file, data: { error: String(problem.error) } }, where, problem.text);
      row.addEventListener("click", () => goTo(problem.line, problem.col));
      return row;
    }),
  );
}

/// Puts the cursor where a problem is.
function goTo(line: number, col: number) {
  const lines = source.value.split("\n");
  let offset = 0;
  for (let i = 0; i < Math.min(line - 1, lines.length); i++) offset += lines[i].length + 1;
  offset += Math.max(0, col - 1);
  source.focus();
  source.setSelectionRange(offset, offset);
  // Roughly centered: a textarea cannot be asked where a line is.
  const lineHeight = parseFloat(getComputedStyle(source).lineHeight) || 19;
  source.scrollTop = Math.max(0, (line - 1) * lineHeight - source.clientHeight / 2);
}

function drawDiagram(screen: Screen) {
  const svg = ui.svg(undefined);
  if (svg !== shownSvg) {
    shownSvg = svg;
    // The SVG is made by Rust, which escapes every piece of model text in it.
    world.innerHTML = svg;
    const drawn = world.querySelector("svg");
    diagramSize = { w: drawn?.width.baseVal.value ?? 0, h: drawn?.height.baseVal.value ?? 0 };
  }
  if (shownView !== screen.view) {
    shownView = screen.view;
    userMoved = false;
    fit();
  }
  const [kind, ...rest] = screen.selected.split(".");
  const selected = rest.join(".");
  for (const group of world.querySelectorAll<SVGGElement>("g[data-id]")) {
    const isNode = group.classList.contains("node");
    const on = selected !== "" && group.dataset.id === selected && (kind === "node") === isNode;
    if (on) group.dataset.selected = "true";
    else delete group.dataset.selected;
  }
  const empty = $("empty");
  empty.hidden = screen.empty === "";
  empty.textContent = screen.empty;
}

function draw(screen: Screen) {
  document.title = screen.title;
  $("title").textContent = screen.title;
  $("description").textContent = screen.description;
  $("views-title").textContent = screen.viewsTitle;
  $("views").replaceChildren(...screen.views.map((item) => button(item)));
  $("status").textContent = screen.status;
  $("back").replaceChildren(...(screen.back ? [button(screen.back)] : []));
  $("exports").replaceChildren(
    ...screen.exports.map((item) => {
      const node = el("button", { type: "button", textContent: item.text, title: item.hint });
      node.addEventListener("click", () => exportDiagram(item.kind, item.fileName));
      return node;
    }),
  );
  const center = () => [canvas.clientWidth / 2, canvas.clientHeight / 2] as const;
  const zoom = (glyph: string, label: string, act: () => void) => {
    const node = el("button", { type: "button", textContent: glyph, title: label, ariaLabel: label });
    node.addEventListener("click", () => {
      userMoved = true;
      act();
    });
    return node;
  };
  $("zoom").replaceChildren(
    ...(screen.exports.length === 0
      ? []
      : [
          zoom("−", screen.controls.zoomOut, () => zoomAt(1 / 1.25, ...center())),
          zoom("+", screen.controls.zoomIn, () => zoomAt(1.25, ...center())),
          zoom("⤢", screen.controls.fit, () => {
            userMoved = false;
            fit();
          }),
        ]),
  );
  drawEditor(screen.editor, screen);
  drawDiagram(screen);
  drawDetails(screen.details);
  // The address says which view is showing, so a link opens on it.
  const hash = screen.view ? `#${encodeURIComponent(screen.view)}` : "";
  if (location.hash !== hash) history.replaceState(null, "", hash || location.pathname + location.search);
}

function tap(id: string) {
  draw(JSON.parse(ui.tap(id)));
}

// ----------------------------------------------------------------- wiring --

let pending = 0;
source.addEventListener("input", () => {
  // Typing redraws once it pauses, not on every key.
  clearTimeout(pending);
  pending = window.setTimeout(() => {
    const screen: Screen = JSON.parse(ui.setSource(shownFile, source.value));
    sources[shownFile] = { ...sources[shownFile], text: source.value };
    store(sources);
    draw(screen);
  }, 200);
});

// Tab indents rather than leaving the editor.
source.addEventListener("keydown", (event) => {
  if (event.key !== "Tab" || event.shiftKey || event.metaKey || event.ctrlKey || event.altKey) return;
  event.preventDefault();
  source.setRangeText("  ", source.selectionStart, source.selectionEnd, "end");
  source.dispatchEvent(new Event("input"));
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && document.activeElement !== source) tap("canvas");
});

const fromAddress = () => decodeURIComponent(location.hash.slice(1));
window.addEventListener("hashchange", () => {
  if (fromAddress() !== shownView) tap(`view.${fromAddress()}`);
});

if (fromAddress()) tap(`view.${fromAddress()}`);
else draw(JSON.parse(ui.screen()));
