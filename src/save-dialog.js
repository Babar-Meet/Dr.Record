import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";

// Feature 3 — post-stop save dialog. Backend (`rename_take` /
// `delete_take`) is authoritative; this layer mirrors messages only.
// Double-click safe: single-execution guard + backend take-token guard.

const $ = (s) => document.querySelector(s);
let takeId = null;
let busy = false;

const ILLEGAL = new Set(["<", ">", ":", '"', "/", "\\", "|", "?", "*"]);
const RESERVED = new Set(["CON", "PRN", "AUX", "NUL",
  "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
  "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"]);

function mirrorValidate(name) {
  if (!name || !name.trim()) return "Name cannot be empty.";
  if (name.length > 255) return `Name is too long (${name.length}/255 characters).`;
  if (name.endsWith(" ") || name.endsWith(".")) return "Name cannot end with a space or dot.";
  for (const c of name) {
    if (ILLEGAL.has(c)) return `Character "${c}" is not allowed on Windows.`;
    if (c.charCodeAt(0) < 32) return "Control characters are not allowed.";
  }
  const stem = name.split(".")[0].trim().toUpperCase();
  if (RESERVED.has(stem)) return `"${stem}" is a reserved Windows name.`;
  return null;
}

function stemOf(path) {
  const base = (path ?? "").split(/[/\\]/).pop() ?? "";
  return base.toLowerCase().endsWith(".mp4") ? base.slice(0, -4) : base;
}

function formatBytes(n) {
  if (!n || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${units[i]}`;
}

function formatInfo(info) {
  if (!info) return "";
  const parts = [];
  if (info.width && info.height) parts.push(`${info.width}×${info.height}`);
  if (info.fps) parts.push(info.fps);
  if (info.duration_secs != null) parts.push(`${Number(info.duration_secs).toFixed(1)}s`);
  if (info.size_bytes != null) parts.push(formatBytes(info.size_bytes));
  if (info.quality) parts.push(String(info.quality));
  if (info.source && info.source !== "all") parts.push(String(info.source));
  return parts.join(" · ");
}

async function init() {
  // Window is reused across takes (hidden, never closed): always reload.
  busy = false;
  takeId = null;
  $("#retryBtn").style.display = "none";
  $("#takeWarn").textContent = "";
  $("#nameError").textContent = "";
  $("#takeInfo").textContent = "";
  $("#takeMeta").textContent = "";
  try {
    const pending = await invoke("get_pending_take");
    if (!pending) {
      $("#nameError").textContent = "Nothing to save (no pending recording).";
      $("#saveBtn").disabled = true;
      $("#deleteBtn").disabled = true;
      return;
    }
    takeId = pending.take_id;
    // Re-enable: the startup init (no pending take yet) disables them, and
    // the window is reused across takes — a stale disabled state is exactly
    // "typing + Enter saves but buttons look dead".
    $("#saveBtn").disabled = false;
    $("#deleteBtn").disabled = false;
    // File facts (resolution, fps, size): best-effort, never blocks saving.
    try {
      const info = await invoke("get_save_info", { takeId });
      $("#takeInfo").textContent = formatInfo(info);
    } catch {
      $("#takeInfo").textContent = "";
    }
    const prefill = stemOf(pending.final_path ?? pending.default_path);
    $("#fileName").value = prefill;
    $("#takeMeta").textContent = pending.default_path ?? "";
    if (!pending.valid) {
      $("#retryBtn").style.display = "";
      const why = pending.reason ? ` Detail: ${pending.reason}` : "";
      $("#takeWarn").textContent = `Finalization failed. Your temporary files were kept — retry without re-recording.${why}`;
    }
    $("#fileName").focus();
    $("#fileName").select();
  } catch (e) {
    $("#nameError").textContent = String(e);
  }
}

async function doSave() {
  if (busy || !takeId) return;
  const name = $("#fileName").value;
  const err = mirrorValidate(name);
  if (err) {
    $("#nameError").textContent = err;
    return;
  }
  busy = true;
  try {
    const finalPath = await invoke("rename_take", { takeId, newName: name, overwrite: false });
    $("#nameError").textContent = "";
    $("#takeMeta").textContent = `Saved: ${finalPath}`;
    takeId = null;
    // Hide (never close): the window is pre-created once and reused per take.
    try { await invoke("hide_save_dialog"); } catch { window.close(); }
  } catch (e) {
    const msg = String(e);
    if (msg.includes("Collision")) {
      const ok = await ask(`${name} already exists. Overwrite it?`, { title: "Overwrite?", kind: "warning" });
      if (ok) {
        try {
          const finalPath = await invoke("rename_take", { takeId, newName: name, overwrite: true });
          $("#takeMeta").textContent = `Saved: ${finalPath}`;
          takeId = null;
          try { await invoke("hide_save_dialog"); } catch { window.close(); }
          return;
        } catch (e2) {
          $("#nameError").textContent = String(e2);
        }
      }
    } else {
      $("#nameError").textContent = msg;
    }
  } finally {
    busy = false;
  }
}

async function doDelete() {
  if (busy || !takeId) return;
  const ok = await ask("Permanently delete this recording and its temporary files?", { title: "Delete?", kind: "warning" });
  if (!ok) return;
  busy = true;
  try {
    await invoke("delete_take", { takeId });
    takeId = null;
    try { await invoke("hide_save_dialog"); } catch { window.close(); }
  } catch (e) {
    $("#nameError").textContent = String(e);
  } finally {
    busy = false;
  }
}

document.addEventListener("DOMContentLoaded", () => {
  init();
  // Reused window: reload for every new take. take-finalized is the trigger
  // (deterministic, backend-driven) — NOT window focus, which may never fire
  // under foreground lockdown and would also wipe a typed name mid-edit.
  listen("take-finalized", () => { if (!busy) init(); });
  $("#saveBtn").addEventListener("click", doSave);
  $("#deleteBtn").addEventListener("click", doDelete);
  $("#retryBtn").addEventListener("click", async () => {
    if (busy || !takeId) return;
    busy = true;
    try {
      await invoke("retry_mux", { takeId });
      $("#takeWarn").textContent = "";
      $("#retryBtn").style.display = "none";
      init();
    } catch (e) {
      $("#nameError").textContent = String(e);
    } finally {
      busy = false;
    }
  });
  $("#fileName").addEventListener("input", () => {
    // Inline error only; dialog stays open, nothing bad is saved.
    $("#nameError").textContent = mirrorValidate($("#fileName").value) ?? "";
  });
  document.addEventListener("keydown", async (e) => {
    if (e.key === "Enter") { e.preventDefault(); doSave(); }
    if (e.key === "Escape") {
      e.preventDefault();
      // Cancel path: keep the file under its default name, no orphans.
      if (takeId && !busy) {
        busy = true;
        try { await invoke("resolve_pending_take", { action: "cancel" }); } catch { /* keep default */ }
        takeId = null;
        try { await invoke("hide_save_dialog"); } catch { window.close(); }
      } else {
        try { await invoke("hide_save_dialog"); } catch { window.close(); }
      }
    }
  });
});
