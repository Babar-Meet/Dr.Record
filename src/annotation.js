import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

// Feature 2 — canvas 2D annotation burned in via gdigrab (screen pixels).
// The backend owns arm/disarm (`toggle_annotation`); this layer owns pixels.
// Esc / Done exits draw mode only; recording always continues.
// Geometry: this window spans the virtual-desktop union, so client coords are
// already union-relative (no per-monitor shift; see overlay.rs). The toolbar
// stays visible for the whole armed session (user demand) and hides on
// disarm; Esc wipes the layer and exits.

const canvas = document.getElementById("drawCanvas");
const ctx = canvas.getContext("2d");
const toolbar = document.getElementById("annoToolbar");
const textInput = document.getElementById("annoTextInput");
let pendingTextPt = null;

let armed = false;
let tool = "pen";
let color = "#ff3b30";
let thickness = "medium";
let drawing = false;
let startPt = null;
let prevPt = null;
let snapshot = null;

const THICKNESS_PX = { thin: 2, medium: 5, thick: 9 };

function setupCanvas() {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(window.innerWidth * dpr);
  canvas.height = Math.floor(window.innerHeight * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
}

function strokeWidth() {
  let w = THICKNESS_PX[thickness] ?? 5;
  if (tool === "highlighter") w *= 3;
  return w;
}

function applyStyle() {
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = strokeWidth();
  ctx.globalAlpha = tool === "highlighter" ? 0.35 : 1.0;
  ctx.globalCompositeOperation = tool === "eraser" ? "destination-out" : "source-over";
}

function pokeToolbar() {
  // Toolbar stays visible for the whole armed session and hides only on
  // disarm: users must see their tools until they exit draw mode.
  toolbar.classList.remove("hidden");
}

async function setArmed(next, source) {
  armed = next;
  try {
    await getCurrentWindow().setIgnoreCursorEvents(!armed);
  } catch {
    // Backend already toggled passthrough; keep drawing state consistent.
  }
  toolbar.classList.toggle("hidden", !armed);
  if (armed) pokeToolbar();
  document.body.style.cursor = armed ? "crosshair" : "default";
}

function canvasPos(e) {
  const r = canvas.getBoundingClientRect();
  return { x: e.clientX - r.left, y: e.clientY - r.top };
}

function drawSegment(a, b) {
  applyStyle();
  ctx.beginPath();
  ctx.moveTo(a.x, a.y);
  ctx.lineTo(b.x, b.y);
  ctx.stroke();
}

function drawArrow(a, b) {
  applyStyle();
  ctx.beginPath();
  ctx.moveTo(a.x, a.y);
  ctx.lineTo(b.x, b.y);
  ctx.stroke();
  const angle = Math.atan2(b.y - a.y, b.x - a.x);
  const head = 12 + strokeWidth();
  for (const d of [Math.PI / 6, -Math.PI / 6]) {
    ctx.beginPath();
    ctx.moveTo(b.x, b.y);
    ctx.lineTo(b.x - head * Math.cos(angle + d), b.y - head * Math.sin(angle + d));
    ctx.stroke();
  }
}

function drawCircle(a, b) {
  applyStyle();
  const cx = (a.x + b.x) / 2;
  const cy = (a.y + b.y) / 2;
  const rx = Math.abs(b.x - a.x) / 2;
  const ry = Math.abs(b.y - a.y) / 2;
  if (rx < 1 || ry < 1) return;
  ctx.beginPath();
  ctx.ellipse(cx, cy, rx, ry, 0, 0, Math.PI * 2);
  ctx.stroke();
}

function drawRect(a, b) {
  applyStyle();
  const w = b.x - a.x;
  const h = b.y - a.y;
  if (Math.abs(w) < 2 || Math.abs(h) < 2) return;
  ctx.beginPath();
  ctx.rect(a.x, a.y, w, h);
  ctx.stroke();
}

function restoreSnapshot() {
  if (snapshot) ctx.putImageData(snapshot, 0, 0);
}

canvas.addEventListener("pointerdown", (e) => {
  if (!armed) return;
  if (pendingTextPt) commitTextInput();
  drawing = true;
  startPt = canvasPos(e);
  prevPt = startPt;
  try {
    snapshot = ctx.getImageData(0, 0, canvas.width, canvas.height);
  } catch {
    snapshot = null;
  }
  canvas.setPointerCapture(e.pointerId);
});

const SHAPE_TOOLS = new Set(["arrow", "circle", "rect"]);

canvas.addEventListener("pointermove", (e) => {
  if (!armed || !drawing) return;
  const pt = canvasPos(e);
  if (tool === "pen" || tool === "highlighter" || tool === "eraser") {
    // Freehand: connect each move to the previous point so fast strokes
    // stay continuous instead of dotted.
    if (prevPt) drawSegment(prevPt, pt);
    prevPt = pt;
  } else if (startPt && SHAPE_TOOLS.has(tool)) {
    restoreSnapshot();
    if (tool === "arrow") drawArrow(startPt, pt);
    else if (tool === "circle") drawCircle(startPt, pt);
    else if (tool === "rect") drawRect(startPt, pt);
  }
});

function endStroke(e) {
  if (!drawing) return;
  drawing = false;
  const pt = e ? canvasPos(e) : prevPt;
  if (startPt && pt) {
    if (tool === "arrow") {
      restoreSnapshot();
      drawArrow(startPt, pt);
    } else if (tool === "circle") {
      restoreSnapshot();
      drawCircle(startPt, pt);
    } else if (tool === "rect") {
      restoreSnapshot();
      drawRect(startPt, pt);
    } else if (tool === "text") {
      // Inline toolbar input (no window.prompt in the webview).
      // Empty submit is a no-op staying in mode.
      showTextInput(pt);
    }
  }
  snapshot = null;
  startPt = null;
  prevPt = null;
}

// Inline label input for the text tool: Enter commits, Esc cancels,
// empty submit is a no-op; mode never changes either way.
function showTextInput(pt) {
  pendingTextPt = pt;
  textInput.value = "";
  textInput.classList.add("visible");
  pokeToolbar();
  setTimeout(() => textInput.focus(), 0);
}

function commitTextInput() {
  const value = textInput.value;
  const pt = pendingTextPt;
  textInput.classList.remove("visible");
  textInput.value = "";
  pendingTextPt = null;
  if (value && pt) {
    applyStyle();
    ctx.font = `${16 + strokeWidth() * 2}px "Segoe UI", sans-serif`;
    ctx.fillText(value, pt.x, pt.y);
  }
}

function cancelTextInput() {
  textInput.classList.remove("visible");
  textInput.value = "";
  pendingTextPt = null;
}

textInput.addEventListener("keydown", (e) => {
  e.stopPropagation();
  pokeToolbar();
  if (e.key === "Enter") { e.preventDefault(); commitTextInput(); }
  else if (e.key === "Escape") { e.preventDefault(); cancelTextInput(); }
});
textInput.addEventListener("blur", () => { if (pendingTextPt) commitTextInput(); });

canvas.addEventListener("pointerup", endStroke);
canvas.addEventListener("pointercancel", () => { drawing = false; snapshot = null; startPt = null; prevPt = null; });

function clearAll() {
  // No-op with state unchanged when nothing is drawn.
  if (canvas.width === 0) return;
  const blank = document.createElement("canvas");
  blank.width = canvas.width;
  blank.height = canvas.height;
  const empty = blank.getContext("2d").getImageData(0, 0, blank.width, blank.height).data;
  const current = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  let dirty = false;
  for (let i = 3; i < current.length; i += 4096) {
    if (current[i] !== empty[i]) { dirty = true; break; }
  }
  if (!dirty) return;
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.restore();
}

toolbar.addEventListener("click", async (e) => {
  const btn = e.target.closest("button");
  if (!btn) return;
  pokeToolbar();
  if (btn.id === "exitDrawBtn") {
    // Explicit disarm (never toggle): a stale Done must not re-arm.
    try { await invoke("annotation_hide"); } catch { /* backend explains */ }
    return;
  }
  if (btn.dataset.tool) {
    if (btn.dataset.tool === "clear") { clearAll(); return; }
    if (btn.dataset.tool !== "text" && pendingTextPt) cancelTextInput();
    tool = btn.dataset.tool;
    toolbar.querySelectorAll("[data-tool]").forEach((b) => b.classList.toggle("active", b === btn));
    try { await invoke("set_annotation_tool", { tool, color, thickness }); } catch { /* non-fatal */ }
    return;
  }
  if (btn.dataset.color) {
    color = btn.dataset.color;
    toolbar.querySelectorAll(".swatch").forEach((b) => b.classList.toggle("active", b === btn));
    try { await invoke("set_annotation_tool", { tool, color, thickness }); } catch { /* non-fatal */ }
  }
});

toolbar.querySelectorAll("[data-thickness]").forEach((b) => b.addEventListener("click", async () => {
  thickness = b.dataset.thickness;
  toolbar.querySelectorAll("[data-thickness]").forEach((x) => x.classList.toggle("active", x === b));
  pokeToolbar();
  try { await invoke("set_annotation_tool", { tool, color, thickness }); } catch { /* non-fatal */ }
}));

document.addEventListener("keydown", async (e) => {
  // Esc = wipe the layer clean AND exit draw mode; recording continues.
  // The next arm always starts from a blank canvas. Explicit disarm (never
  // toggle) so a stale Esc can't re-arm; the backend also grabs Esc
  // globally while armed for the case this window isn't focused.
  if (e.key === "Escape" && armed) {
    e.preventDefault();
    clearAll();
    try { await invoke("annotation_hide"); } catch { /* backend explains */ }
  }
});

document.addEventListener("DOMContentLoaded", async () => {
  setupCanvas();
  window.addEventListener("resize", setupCanvas);
  await listen("annotation-state", (e) => {
    const next = !!e.payload?.armed;
    if (e.payload?.tool) tool = e.payload.tool;
    setArmed(next, e.payload?.source ?? "backend");
  });
  await listen("annotation-clear", () => clearAll());
  // Sync with the backend on load: if this window was (re)created after
  // the arm event fired, the listener above missed it and the canvas would
  // sit in "not draw mode" despite the backend being armed.
  try {
    const current = await invoke("get_annotation_state");
    if (current?.tool) {
      tool = current.tool;
      toolbar.querySelectorAll("[data-tool]").forEach((b) => b.classList.toggle("active", b.dataset.tool === tool));
    }
    setArmed(!!current?.armed, "init-sync");
  } catch {
    // Start disarmed (click-through); backend confirms via annotation-state.
    setArmed(false, "init");
  }
});
