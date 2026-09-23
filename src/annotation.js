import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

// Annotation layer: Dr.Player drawbar UX (tools, shortcuts, drag, slider,
// undo/redo, clear, exit) over a vector shape store burned into the capture
// by gdigrab. Recorder differences from Player, all deliberate:
// - Exit keeps marks (Player discards): marks must persist in the video.
// - Esc wipes the layer AND exits (user demand); recording never stops.
// - DPR-aware canvas (Player is CSS-pixel); pointer events (mouse+touch).
// - No seek/step controls: a live recording has no timeline.
// - Text via T key + inline input (Player has no visible text trigger;
//   a modal dialog suits a player, inline suits this fullscreen layer).
// The backend owns arm/disarm; this layer owns shapes. Disarmed = hidden
// window (click-through); armed = visible layer + toolbar.

const canvas = document.getElementById("drawCanvas");
const ctx = canvas.getContext("2d");
const toolbar = document.getElementById("annoToolbar");
const textInput = document.getElementById("annoTextInput");
const sizeSlider = document.getElementById("csize");
const colorPicker = document.getElementById("cpicker");

let armed = false;
let tool = "pen";
let color = "#ff0000";
let size = 3;
let drawing = false;
let shapes = [];
let undoStack = [];
let redoStack = [];
const MAX_HISTORY = 50;
let selShape = null;
let selShapeOffX = 0;
let selShapeOffY = 0;
let pendingTextPt = null;

function setupCanvas() {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.floor(window.innerWidth * dpr);
  canvas.height = Math.floor(window.innerHeight * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  renderAll();
}

function applyStyle(c, s) {
  ctx.strokeStyle = c;
  ctx.lineWidth = s;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.fillStyle = c;
}

function drawShape(c, s) {
  if (s.type === "text") {
    c.font = `${s.fontSize}px "Segoe UI", sans-serif`;
    c.fillStyle = s.color;
    c.fillText(s.text, s.x, s.y);
    return;
  }
  applyStyle(s.color, s.size);
  c.beginPath();
  switch (s.type) {
    case "pen": {
      const pts = s.points;
      if (pts.length === 1) pts.push({ ...pts[0] });
      c.moveTo(pts[0].x, pts[0].y);
      for (let i = 1; i < pts.length; i++) c.lineTo(pts[i].x, pts[i].y);
      c.stroke();
      break;
    }
    case "line":
      c.moveTo(s.x1, s.y1);
      c.lineTo(s.x2, s.y2);
      c.stroke();
      break;
    case "arrow": {
      c.moveTo(s.x1, s.y1);
      c.lineTo(s.x2, s.y2);
      c.stroke();
      const angle = Math.atan2(s.y2 - s.y1, s.x2 - s.x1);
      const h = Math.min(16, Math.max(8, s.size * 5));
      c.beginPath();
      c.moveTo(s.x2, s.y2);
      c.lineTo(s.x2 - h * Math.cos(angle - Math.PI / 6), s.y2 - h * Math.sin(angle - Math.PI / 6));
      c.lineTo(s.x2 - h * Math.cos(angle + Math.PI / 6), s.y2 - h * Math.sin(angle + Math.PI / 6));
      c.closePath();
      c.fill();
      break;
    }
    case "rect":
      c.strokeRect(Math.min(s.x1, s.x2), Math.min(s.y1, s.y2), Math.abs(s.x2 - s.x1), Math.abs(s.y2 - s.y1));
      break;
    case "circle":
      c.arc(s.x1, s.y1, Math.sqrt((s.x2 - s.x1) ** 2 + (s.y2 - s.y1) ** 2), 0, Math.PI * 2);
      c.stroke();
      break;
  }
}

function renderAll() {
  const dpr = window.devicePixelRatio || 1;
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  for (const s of shapes) drawShape(ctx, s);
}

function saveDrawState() {
  undoStack.push(JSON.parse(JSON.stringify(shapes)));
  if (undoStack.length > MAX_HISTORY) undoStack.shift();
  redoStack = [];
}

function undoDraw() {
  if (undoStack.length === 0 || !armed) return;
  redoStack.push(JSON.parse(JSON.stringify(shapes)));
  shapes = undoStack.pop();
  selShape = null;
  renderAll();
}

function redoDraw() {
  if (redoStack.length === 0 || !armed) return;
  undoStack.push(JSON.parse(JSON.stringify(shapes)));
  shapes = redoStack.pop();
  selShape = null;
  renderAll();
}

function clearDrawCanvas() {
  if (!armed) return;
  if (shapes.length === 0 && undoStack.length === 0) return;
  saveDrawState();
  shapes = [];
  selShape = null;
  renderAll();
}

function wipeLayer() {
  shapes = [];
  undoStack = [];
  redoStack = [];
  selShape = null;
  cancelTextInput();
  renderAll();
}

function hitTest(x, y) {
  const thresh = 12;
  for (let i = shapes.length - 1; i >= 0; i--) {
    const s = shapes[i];
    if (s.type === "text") {
      ctx.font = `${s.fontSize}px "Segoe UI", sans-serif`;
      const m = ctx.measureText(s.text);
      if (x >= s.x && x <= s.x + m.width && y >= s.y - s.fontSize && y <= s.y) return i;
    } else if (s.type === "pen") {
      for (const p of s.points) {
        if (Math.abs(x - p.x) <= thresh && Math.abs(y - p.y) <= thresh) return i;
      }
    } else {
      const x1 = Math.min(s.x1, s.x2), x2 = Math.max(s.x1, s.x2);
      const y1 = Math.min(s.y1, s.y2), y2 = Math.max(s.y1, s.y2);
      if (x >= x1 - thresh && x <= x2 + thresh && y >= y1 - thresh && y <= y2 + thresh) return i;
    }
  }
  return -1;
}

function canvasPos(e) {
  const r = canvas.getBoundingClientRect();
  return { x: e.clientX - r.left, y: e.clientY - r.top };
}

function setCanvasCursor(cursor) {
  document.body.style.cursor = cursor;
}

async function setArmed(next) {
  armed = next;
  try {
    await getCurrentWindow().setIgnoreCursorEvents(!armed);
  } catch {
    // Backend already toggled passthrough; keep state consistent.
  }
  toolbar.classList.toggle("open", armed);
  if (armed) {
    // Reset toolbar placement each arm (Player behavior).
    toolbar.style.left = "";
    toolbar.style.top = "";
    toolbar.style.bottom = "";
    toolbar.style.transform = "";
    setCanvasCursor(tool === "hand" ? "grab" : "crosshair");
  } else {
    // Never leave a stroke hanging across sessions.
    drawing = false;
    selShape = null;
    cancelTextInput();
    setCanvasCursor("default");
  }
}

function syncToolState() {
  // One-way push ONLY: user changed something locally. Never call this from
  // the annotation-state listener — the backend echoes every set as an
  // event, and syncing the echo back creates an infinite feedback storm.
  try {
    invoke("set_annotation_tool", { tool, color, thickness: String(size) });
  } catch {
    // Non-fatal: pixels are local, backend state is advisory.
  }
}

// Apply a tool arriving FROM the backend (event/init sync): local state
// only, never echoed back.
function applyToolLocal(t) {
  const known = ["pen", "line", "arrow", "rect", "circle", "hand", "text"];
  if (!known.includes(t)) return;
  tool = t;
  document.querySelectorAll('.dbtn[data-tool]').forEach((b) => b.classList.toggle("active", b.dataset.tool === t));
}

function switchTool(t) {
  if (drawing) {
    drawing = false;
    if (shapes.length > 0) {
      const last = shapes[shapes.length - 1];
      const incomplete = last.type === "pen"
        ? last.points.length <= 1
        : last.x1 === last.x2 && last.y1 === last.y2;
      if (incomplete) {
        shapes.pop();
        renderAll();
      }
    }
  }
  if (pendingTextPt) cancelTextInput();
  document.querySelectorAll('.dbtn[data-tool]').forEach((b) => b.classList.toggle("active", b.dataset.tool === t));
  tool = t;
  selShape = null;
  if (armed) setCanvasCursor(t === "hand" ? "grab" : "crosshair");
  syncToolState();
}

// --- Inline text input (T tool) -------------------------------------------
function showTextInput(pt) {
  pendingTextPt = pt;
  textInput.value = "";
  textInput.classList.add("visible");
  setTimeout(() => textInput.focus(), 0);
}

function commitTextInput() {
  const value = textInput.value;
  const pt = pendingTextPt;
  textInput.classList.remove("visible");
  textInput.value = "";
  pendingTextPt = null;
  // Empty submit is a no-op staying in mode (matches Player text flow).
  if (value && value.trim() && pt) {
    saveDrawState();
    shapes.push({
      type: "text",
      x: pt.x,
      y: pt.y,
      text: value,
      fontSize: Math.max(12, size * 5),
      color,
    });
    renderAll();
  }
}

function cancelTextInput() {
  textInput.classList.remove("visible");
  textInput.value = "";
  pendingTextPt = null;
}

textInput.addEventListener("keydown", (e) => {
  e.stopPropagation();
  if (e.key === "Enter") {
    e.preventDefault();
    commitTextInput();
  } else if (e.key === "Escape") {
    e.preventDefault();
    cancelTextInput();
  }
});
textInput.addEventListener("blur", () => {
  if (pendingTextPt) commitTextInput();
});

// --- Canvas pointer handling ----------------------------------------------
canvas.addEventListener("pointerdown", (e) => {
  if (!armed || e.button !== 0) return;
  if (textInput.classList.contains("visible")) return;
  e.preventDefault();
  const { x, y } = canvasPos(e);

  if (tool === "hand") {
    const idx = hitTest(x, y);
    if (idx >= 0) {
      selShape = shapes[idx];
      if (selShape.type === "text") {
        selShapeOffX = x - selShape.x;
        selShapeOffY = y - selShape.y;
      } else if (selShape.type === "pen") {
        selShapeOffX = x - selShape.points[0].x;
        selShapeOffY = y - selShape.points[0].y;
      } else {
        selShapeOffX = x - (selShape.x1 + selShape.x2) / 2;
        selShapeOffY = y - (selShape.y1 + selShape.y2) / 2;
      }
      setCanvasCursor("grabbing");
      drawing = true;
    }
    return;
  }

  if (tool === "text") {
    showTextInput({ x, y });
    return;
  }

  saveDrawState();
  drawing = true;
  if (tool === "pen") {
    shapes.push({ type: "pen", color, size, points: [{ x, y }] });
  } else {
    shapes.push({ type: tool, color, size, x1: x, y1: y, x2: x, y2: y });
  }
  try {
    canvas.setPointerCapture(e.pointerId);
  } catch {
    // Touch quirks: stroke still tracks via pointermove.
  }
});

canvas.addEventListener("pointermove", (e) => {
  if (!armed || !drawing) return;
  e.preventDefault();
  const { x, y } = canvasPos(e);

  if (selShape) {
    if (selShape.type === "text") {
      selShape.x = x - selShapeOffX;
      selShape.y = y - selShapeOffY;
    } else if (selShape.type === "pen") {
      const dx = x - selShapeOffX - selShape.points[0].x;
      const dy = y - selShapeOffY - selShape.points[0].y;
      for (const pt of selShape.points) {
        pt.x += dx;
        pt.y += dy;
      }
      selShapeOffX = x - selShape.points[0].x;
      selShapeOffY = y - selShape.points[0].y;
    } else {
      const w2 = (selShape.x2 - selShape.x1) / 2;
      const h2 = (selShape.y2 - selShape.y1) / 2;
      const newCx = x - selShapeOffX;
      const newCy = y - selShapeOffY;
      selShape.x1 = newCx - w2;
      selShape.x2 = newCx + w2;
      selShape.y1 = newCy - h2;
      selShape.y2 = newCy + h2;
    }
    renderAll();
    return;
  }

  const s = shapes[shapes.length - 1];
  if (!s || s.type === "text") {
    drawing = false;
    return;
  }
  if (s.type === "pen") {
    s.points.push({ x, y });
  } else {
    s.x2 = x;
    s.y2 = y;
  }
  renderAll();
});

function endStroke(e) {
  if (!drawing) {
    // Mouse Back/Forward buttons: undo/redo (Player parity).
    if (e && (e.button === 3 || e.button === 4)) {
      e.preventDefault();
      if (e.button === 3) undoDraw();
      else redoDraw();
    }
    return;
  }
  if (e && e.button !== 0) return;
  drawing = false;
  if (selShape && tool === "hand") setCanvasCursor("grab");
  selShape = null;
}

canvas.addEventListener("pointerup", endStroke);
canvas.addEventListener("pointercancel", () => {
  drawing = false;
  if (selShape && tool === "hand") setCanvasCursor("grab");
  selShape = null;
});
canvas.addEventListener("mouseleave", () => {
  if (selShape && tool === "hand") setCanvasCursor("grab");
  selShape = null;
  if (drawing) drawing = false;
});

// --- Toolbar wiring (Player parity) ---------------------------------------
let dragData = null;
document.querySelector(".dhandle").addEventListener("mousedown", (e) => {
  if (e.button !== 0) return;
  e.preventDefault();
  e.stopPropagation();
  const rect = toolbar.getBoundingClientRect();
  toolbar.style.left = `${rect.left}px`;
  toolbar.style.top = `${rect.top}px`;
  toolbar.style.transform = "none";
  toolbar.style.bottom = "auto";
  dragData = { offX: e.clientX - rect.left, offY: e.clientY - rect.top };
});
document.addEventListener("mousemove", (e) => {
  if (!dragData) return;
  toolbar.style.left = `${e.clientX - dragData.offX}px`;
  toolbar.style.top = `${e.clientY - dragData.offY}px`;
});
document.addEventListener("mouseup", () => {
  dragData = null;
});

document.querySelectorAll(".dbtn[data-tool]").forEach((btn) => {
  btn.addEventListener("click", () => switchTool(btn.dataset.tool));
});

document.querySelectorAll(".cswatch").forEach((el) => {
  el.addEventListener("click", () => {
    document.querySelectorAll(".cswatch").forEach((s) => s.classList.remove("sel"));
    el.classList.add("sel");
    color = el.dataset.color;
    colorPicker.value = color;
    syncToolState();
  });
});

colorPicker.addEventListener("input", (e) => {
  color = e.target.value;
  document.querySelectorAll(".cswatch").forEach((s) => s.classList.remove("sel"));
  syncToolState();
});

sizeSlider.addEventListener("input", (e) => {
  size = Math.max(1, Math.min(40, parseInt(e.target.value, 10) || 3));
  syncToolState();
});

document.getElementById("bundo").addEventListener("click", undoDraw);
document.getElementById("bredo").addEventListener("click", redoDraw);
document.getElementById("bclear").addEventListener("click", clearDrawCanvas);
document.getElementById("exitDrawBtn").addEventListener("click", async () => {
  // Exit keeps marks (burn-in); Esc wipes. Explicit disarm, never toggle.
  try {
    await invoke("annotation_hide");
  } catch {
    // Backend explains (e.g. not recording).
  }
});

// --- Keyboard shortcuts (Player parity: P/L/A/R/C/H/T, 0-9, Esc, Del) -----
document.addEventListener("keydown", async (e) => {
  if (!armed) return;
  if (textInput.classList.contains("visible")) return; // typing a label
  const k = e.key.toLowerCase();
  if (k === "p" && !e.ctrlKey && !e.metaKey) {
    switchTool("pen");
    e.preventDefault();
  } else if (k === "l" && !e.ctrlKey && !e.metaKey) {
    switchTool("line");
    e.preventDefault();
  } else if (k === "a" && !e.ctrlKey && !e.metaKey) {
    switchTool("arrow");
    e.preventDefault();
  } else if (k === "r" && !e.ctrlKey && !e.metaKey) {
    switchTool("rect");
    e.preventDefault();
  } else if (k === "c" && !e.ctrlKey && !e.metaKey) {
    switchTool("circle");
    e.preventDefault();
  } else if (k === "h" && !e.ctrlKey && !e.metaKey) {
    switchTool("hand");
    e.preventDefault();
  } else if (k === "t" && !e.ctrlKey && !e.metaKey) {
    switchTool("text");
    e.preventDefault();
  } else if (e.key === "Escape") {
    // Esc wipes the layer AND exits draw mode; recording continues.
    e.preventDefault();
    wipeLayer();
    try {
      await invoke("annotation_hide");
    } catch {
      // Backend explains.
    }
  } else if (k === "delete" || k === "backspace") {
    if (e.ctrlKey || e.metaKey) {
      e.preventDefault();
      clearDrawCanvas();
    } else if (selShape) {
      e.preventDefault();
      saveDrawState();
      const idx = shapes.indexOf(selShape);
      if (idx >= 0) {
        shapes.splice(idx, 1);
        renderAll();
      }
      selShape = null;
    }
  } else if (k >= "0" && k <= "9") {
    const v = parseInt(k, 10);
    size = 2 + v * 2;
    sizeSlider.value = Math.min(40, size);
    syncToolState();
    e.preventDefault();
  }
});

document.addEventListener("DOMContentLoaded", async () => {
  setupCanvas();
  window.addEventListener("resize", setupCanvas);
  await listen("annotation-state", (e) => {
    const next = !!e.payload?.armed;
    if (typeof e.payload?.tool === "string" && e.payload.tool) {
      applyToolLocal(e.payload.tool);
    }
    setArmed(next);
  });
  await listen("annotation-clear", () => wipeLayer());
  // Sync with the backend on load: a late-loading window misses the arm
  // event and would sit in "not draw mode" despite the backend being armed.
  try {
    const current = await invoke("get_annotation_state");
    if (typeof current?.tool === "string" && current.tool) {
      applyToolLocal(current.tool);
    }
    setArmed(!!current?.armed);
  } catch {
    setArmed(false);
  }
});
