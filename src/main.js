import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

let capturingHotkey = false;
let capturingTarget = "record";
let currentHotkey = "Ctrl+Shift+Alt+R";
let currentAnnotationHotkey = "Ctrl+Shift+Alt+A";
let isSaving = false;

const $ = (s) => document.querySelector(s);

// ---------------------------------------------------------------------------
// Source dropdown
// ---------------------------------------------------------------------------

function normalizeTitle(fullTitle) {
  if (!fullTitle) return "Unknown";
  const parts = fullTitle.split(" - ");
  if (parts.length > 1) {
    return parts[parts.length - 1].trim();
  }
  return fullTitle.trim();
}

async function loadPreview(value) {
  const wrapper = $("#previewImgWrapper");
  if (value === "all") {
    wrapper.innerHTML = '<div class="source-preview-placeholder">Desktop (All Monitors)</div>';
    return;
  }

  wrapper.innerHTML = '<div class="source-preview-placeholder">Loading...</div>';
  try {
    const base64 = await invoke("get_thumbnail", { source: value });
    if (base64) {
      wrapper.innerHTML = `<img src="${base64}" />`;
    } else {
      wrapper.innerHTML = '<div class="source-preview-placeholder">Preview not available</div>';
    }
  } catch (e) {
    wrapper.innerHTML = '<div class="source-preview-placeholder">Preview failed</div>';
  }
}

// Monitor selections are keyed by the monitor's stable device id
// (e.g. "monitor:\\.\DISPLAY1") so the dropdown, the preview thumbnail, and
// the recorded capture always refer to the same physical monitor.
// Old configs stored "monitor:<index>"; normalize those to the matching
// device id against the freshly enumerated list.
function normalizeMonitorSource(target, monitors) {
  if (typeof target === "string" && target.startsWith("monitor:")) {
    const key = target.slice("monitor:".length);
    if (/^\d+$/.test(key)) {
      const mon = monitors[parseInt(key, 10)];
      if (mon && mon.device_id) return `monitor:${mon.device_id}`;
      return "all";
    }
  }
  return target;
}

async function populateSources(savedSource) {
  const select = $("#sourceSelect");
  const hint = $("#sourceHint");

  // Keep current selection if we're refreshing
  const prevValue = select.value;

  select.innerHTML = ""; // clear

  try {
    const { monitors } = await invoke("enum_sources");

    // --- All screens ---
    const allOpt = document.createElement("option");
    allOpt.value = "all";
    allOpt.textContent = "⊞  All Screens";
    select.appendChild(allOpt);

    // --- Individual monitors ---
    if (monitors && monitors.length > 0) {
      const monGroup = document.createElement("optgroup");
      monGroup.label = "Monitors";
      for (const mon of monitors) {
        const opt = document.createElement("option");
        const id = mon.device_id ?? mon.index;
        opt.value = `monitor:${id}`;
        opt.textContent = `🖥  ${mon.label}  (${mon.width}×${mon.height})`;
        monGroup.appendChild(opt);
      }
      select.appendChild(monGroup);
    }

    // Restore saved / previous / refreshed value (migrating legacy indices)
    const target = normalizeMonitorSource(
      savedSource ?? prevValue ?? "all",
      monitors ?? []
    );
    if ([...select.options].some((o) => o.value === target)) {
      select.value = target;
    } else {
      select.value = "all";
    }

    updateSourceHint(select.value, hint);
    await loadPreview(select.value);
  } catch (e) {
    console.error("enum_sources error:", e);
    hint.textContent = "Could not enumerate sources.";
  }

  select.addEventListener("change", async () => {
    updateSourceHint(select.value, hint);
    await loadPreview(select.value);
  });
}

async function populateMicrophones(savedMic) {
  const select = $("#micSelect");
  const prevValue = select.value;
  select.innerHTML = "";
  
  const noneOpt = document.createElement("option");
  noneOpt.value = "None";
  noneOpt.textContent = "None";
  select.appendChild(noneOpt);

  try {
    const mics = await invoke("get_microphones");
    for (const mic of mics) {
      const opt = document.createElement("option");
      opt.value = mic;
      opt.textContent = mic;
      select.appendChild(opt);
    }
  } catch (e) {
    console.error("Failed to get microphones:", e);
  }

  const target = savedMic ?? prevValue ?? "None";
  if ([...select.options].some((o) => o.value === target)) {
    select.value = target;
  } else {
    select.value = "None";
  }
}

function updateSourceHint(value, hintEl) {
  if (!hintEl) return;
  if (value === "all") {
    hintEl.textContent = "Records the full virtual desktop (all monitors combined).";
  } else if (value.startsWith("monitor:")) {
    hintEl.textContent = "Records only that monitor's area.";
  } else {
    hintEl.textContent = "";
  }
}

// ---------------------------------------------------------------------------
// Config load / save
// ---------------------------------------------------------------------------

async function loadConfig() {
  try {
    const config = await invoke("load_config");
    $("#outputDir").value = config.output_dir || "";
    $("#hotkeyInput").value = config.hotkey || "Ctrl+Shift+Alt+R";
    currentHotkey = config.hotkey || "Ctrl+Shift+Alt+R";
    $("#annotationHotkeyInput").value = config.annotation_hotkey || "Ctrl+Shift+Alt+A";
    currentAnnotationHotkey = config.annotation_hotkey || "Ctrl+Shift+Alt+A";

    // Populate sources then select the saved one
    const source = config.recording_source || "all";
    await populateSources(source);
    
    // Populate mics
    await populateMicrophones(config.microphone_name);

    $("#framerateSelect").value = String(config.framerate ?? 60);
    $("#qualitySelect").value = config.quality || "high";
    $("#showOverlay").checked = config.show_overlay !== false;
    $("#autoStart").checked = config.auto_start === true;
    
    // System audio
    if (config.record_system_audio !== undefined) {
      $("#recordSystemAudio").checked = config.record_system_audio;
    }
    // Mic master switch (default off); the dropdown only picks the device.
    $("#recordMic").checked = config.record_mic === true;
  } catch (e) {
    showNotification("Failed to load config: " + e, "error");
  }
}

async function saveConfigAndStart() {
  const config = {
    output_dir: $("#outputDir").value,
    hotkey: currentHotkey,
    annotation_hotkey: currentAnnotationHotkey,
    recording_source: $("#sourceSelect").value,
    framerate: parseInt($("#framerateSelect").value, 10),
    quality: $("#qualitySelect").value,
    show_overlay: $("#showOverlay").checked,
    auto_start: $("#autoStart").checked,
    record_system_audio: $("#recordSystemAudio").checked,
    record_mic: $("#recordMic").checked,
    microphone_name: $("#micSelect").value,
  };

  if (!config.output_dir) {
    showNotification("Please select an output directory first", "error");
    return;
  }

  if (hotkeysEqual(config.annotation_hotkey, config.hotkey)) {
    showNotification("Annotation hotkey must differ from the Start/Stop hotkey.", "error");
    return;
  }

  try {
    await invoke("save_config", { config });
    await invoke("reload_annotation_hotkey", { hotkey: currentAnnotationHotkey });
    await invoke("reload_hotkey", { hotkey: currentHotkey });
    showNotification("Saved! Hotkey: " + currentHotkey, "success");
    setTimeout(() => invoke("hide_settings"), 1200);
  } catch (e) {
    showNotification("Failed to save: " + e, "error");
  }
}

// ---------------------------------------------------------------------------
// Directory picker
// ---------------------------------------------------------------------------

async function pickDirectory() {
  try {
    const dir = await open({ directory: true, multiple: false, title: "Choose output folder" });
    if (dir) $("#outputDir").value = dir;
  } catch (e) {
    showNotification("Failed to pick directory: " + e, "error");
  }
}

// ---------------------------------------------------------------------------
// Hotkey capture
// ---------------------------------------------------------------------------

function startHotkeyCapture() {
  capturingHotkey = true;
  capturingTarget = "record";
  const input = $("#hotkeyInput");
  input.value = "Press shortcut...";
  input.classList.add("capturing");
  $("#captureHotkeyBtn").textContent = "Listening...";
}

function startAnnotationHotkeyCapture() {
  capturingHotkey = true;
  capturingTarget = "annotation";
  const input = $("#annotationHotkeyInput");
  input.value = "Press shortcut...";
  input.classList.add("capturing");
  $("#captureAnnotationHotkeyBtn").textContent = "Listening...";
}

// Order/format-insensitive hotkey compare ("Ctrl+Shift+Alt+A" ==
// "ctrl+alt+shift+a"): capture order (Ctrl,Alt,Shift,Win) differs from the
// stored default order (Ctrl,Shift,Alt), and a naive compare lets the two
// actions share one shortcut — the annotation key then starts/stops video.
function hotkeysEqual(a, b) {
  const norm = (s) => {
    const mods = new Set();
    let key = "";
    for (const part of String(s).split("+")) {
      const p = part.trim().toLowerCase();
      if (!p) continue;
      if (p === "ctrl" || p === "control") mods.add("ctrl");
      else if (p === "alt" || p === "option") mods.add("alt");
      else if (p === "shift") mods.add("shift");
      else if (["win", "meta", "super", "command", "cmd"].includes(p)) mods.add("meta");
      else key = p;
    }
    return [...mods].sort().join("+") + "+" + key;
  };
  return norm(a) === norm(b);
}

function handleKeyCapture(e) {
  if (!capturingHotkey) return;
  e.preventDefault();
  e.stopPropagation();

  const key = e.key;
  if (key === "Escape") {
    cancelHotkeyCapture();
    return;
  }

  const skipKeys = {
    Control: true, Shift: true, Alt: true, Meta: true,
    Escape: true, Tab: true, CapsLock: true,
  };

  if (skipKeys[key]) return;

  const parts = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Win");

  parts.push(key.length === 1 ? key.toUpperCase() : key);

  if (parts.length < 2) return;

  const combo = parts.join("+");
  if (capturingTarget === "annotation") {
    if (hotkeysEqual(combo, currentHotkey)) {
      showNotification("Annotation hotkey must differ from the Start/Stop hotkey.", "error");
      cancelHotkeyCapture();
      return;
    }
    currentAnnotationHotkey = combo;
    $("#annotationHotkeyInput").value = combo;
  } else {
    if (hotkeysEqual(combo, currentAnnotationHotkey)) {
      showNotification("Start/Stop hotkey must differ from the annotation hotkey.", "error");
      cancelHotkeyCapture();
      return;
    }
    currentHotkey = combo;
    $("#hotkeyInput").value = combo;
  }
  cancelHotkeyCapture();
  showNotification("Hotkey set to: " + combo, "success");
}

function cancelHotkeyCapture() {
  capturingHotkey = false;
  const input = capturingTarget === "annotation" ? $("#annotationHotkeyInput") : $("#hotkeyInput");
  const btn = capturingTarget === "annotation" ? $("#captureAnnotationHotkeyBtn") : $("#captureHotkeyBtn");
  input.classList.remove("capturing");
  btn.textContent = "Capture";
  if (!input.value || input.value === "Press shortcut...") {
    input.value = capturingTarget === "annotation" ? currentAnnotationHotkey : currentHotkey;
  }
  capturingTarget = "record";
}

// ---------------------------------------------------------------------------
// Status / display info
// ---------------------------------------------------------------------------

async function updateStatus() {
  if (isSaving) {
    const dot = $("#statusDot");
    const text = $("#statusText");
    dot.className = "status-dot saving";
    text.textContent = "Saving...";
    return;
  }
  try {
    const recording = await invoke("get_status");
    const dot = $("#statusDot");
    const text = $("#statusText");

    if (recording) {
      dot.className = "status-dot recording";
      text.textContent = "Recording...";
    } else {
      dot.className = "status-dot stopped";
      text.textContent = "Idle";
    }
  } catch (e) {
    // ignore
  }
}

async function testRecord() {
  try {
    const config = await invoke("load_config");
    if (!config.output_dir) {
      showNotification("Please select output directory first", "error");
      return;
    }

    await invoke("start_rec", {});
    showNotification("Recording test... stopping in 5s", "success");
    updateStatus();

    setTimeout(async () => {
      try {
        await invoke("stop_rec", {});
        showNotification("Test recording saved!", "success");
        updateStatus();
      } catch (e) {
        showNotification("Error: " + e, "error");
      }
    }, 5000);
  } catch (e) {
    showNotification("Error: " + e, "error");
  }
}

async function updateDisplayInfo() {
  try {
    const info = await invoke("get_display_info");
    const el = $("#displayInfo");
    if (info && info.width && info.height) {
      el.textContent = `Display: ${info.width}x${info.height} @ ${info.refreshRate || 60} Hz`;
    }
  } catch {
    const el = $("#displayInfo");
    if (window.screen) {
      el.textContent = `Display: ${screen.width}x${screen.height}`;
    }
  }
}

// ---------------------------------------------------------------------------
// Notification helper
// ---------------------------------------------------------------------------

function showNotification(msg, type = "info") {
  const el = $("#notification");
  el.textContent = msg;
  el.className = "notification show " + type;
  clearTimeout(el._hide);
  el._hide = setTimeout(() => el.classList.remove("show"), 3000);
}

// ---------------------------------------------------------------------------
// Audio level listener
// ---------------------------------------------------------------------------

function setAudioLevel(elId, rms) {
  const el = $(`#${elId}`);
  if (!el) return;
  
  let percent = 0;
  if (rms > 0) {
    const db = 20 * Math.log10(rms);
    // map -60dB to 0%, 0dB to 100%
    percent = Math.min(100, Math.max(0, (db + 60) * (100 / 60)));
  }
  
  el.style.width = `${percent}%`;
  
  if (percent > 85) {
    el.style.backgroundColor = "var(--red)";
  } else if (percent > 65) {
    el.style.backgroundColor = "orange";
  } else {
    el.style.backgroundColor = "var(--green)";
  }
}

// ---------------------------------------------------------------------------
// Init
// ---------------------------------------------------------------------------

document.addEventListener("DOMContentLoaded", async () => {
  await loadConfig();
  updateStatus();
  updateDisplayInfo();

  const interval = setInterval(updateStatus, 1000);

  const unlistenStatus = await listen("status-changed", (e) => {
    if (e.payload === "saving") {
      isSaving = true;
    } else {
      isSaving = false;
    }
    updateStatus();
  });
  const unlistenError = await listen("recording-error", (e) => {
    showNotification("Recording error: " + e.payload, "error");
  });
  const unlistenAudioError = await listen("audio-error", (e) => {
    showNotification("Audio warning: " + e.payload, "error");
  });
  const unlistenWindowClosed = await listen("recording-stopped-window-closed", () => {
    updateStatus();
    showNotification("Target window closed — recording saved.", "info");
  });
  const unlistenCrashed = await listen("recording-crashed", (event) => {
    updateStatus();
    const errorMsg = event.payload;
    showNotification("Recording failed!", "error");
    console.error("FFmpeg Crashed:\n", errorMsg);
    alert("Recording failed because FFmpeg crashed.\n\n" + errorMsg);
  });

  // Rich take events: log offsets, surface warnings/errors. Saving itself
  // lives ONLY in the dedicated save popup (no second dialog in settings).
  const unlistenFinalized = await listen("take-finalized", (e) => {
    const r = e.payload ?? {};
    const off = r.offsets_ms ?? {};
    if (off.system != null || off.mic != null) {
      console.info(`A/V offsets ms (positive = audio late): system=${off.system} mic=${off.mic}`);
    }
  });
  const unlistenWarning = await listen("save-warning", (e) => {
    const warnings = e.payload ?? [];
    for (const w of warnings) {
      showNotification(`Audio warning (${w.track}): final file may be video-only.`, "error");
    }
  });
  const unlistenSaveError = await listen("save-error", (e) => {
    const msg = e.payload?.message ?? "Finalization failed.";
    const retryable = e.payload?.retryable;
    showNotification(retryable ? `${msg} You can retry without re-recording.` : msg, "error");
  });
  
  const unlistenSysAudio = await listen("audio-level-system", (event) => {
    setAudioLevel("sysAudioLevel", event.payload);
  });
  
  const unlistenMicAudio = await listen("audio-level-mic", (event) => {
    setAudioLevel("micAudioLevel", event.payload);
  });

  window.addEventListener("beforeunload", () => {
    clearInterval(interval);
    unlistenStatus();
    unlistenError();
    unlistenAudioError();
    unlistenWindowClosed();
    unlistenFinalized();
    unlistenWarning();
    unlistenSaveError();
    unlistenSysAudio();
    unlistenMicAudio();
  });

  $("#pickDirBtn").addEventListener("click", pickDirectory);
  $("#captureHotkeyBtn").addEventListener("click", startHotkeyCapture);
  $("#captureAnnotationHotkeyBtn").addEventListener("click", startAnnotationHotkeyCapture);
  $("#saveBtn").addEventListener("click", saveConfigAndStart);
  $("#testRecBtn").addEventListener("click", testRecord);
  $("#refreshSourcesBtn").addEventListener("click", async () => {
    await populateSources(null); // null = keep current selection
    showNotification("Sources refreshed", "success");
  });
  $("#refreshMicsBtn").addEventListener("click", async () => {
    await populateMicrophones(null);
    restartPreviewsSoon();
    showNotification("Microphones refreshed", "success");
  });

  // Level meters follow the CURRENT selections live: without this the meters
  // stay on the old device until the next save + reopen.
  let previewTimer = null;
  async function restartPreviewsSoon() {
    clearTimeout(previewTimer);
    previewTimer = setTimeout(async () => {
      try {
        await invoke("restart_audio_previews", {
          recordSystemAudio: $("#recordSystemAudio").checked,
          recordMic: $("#recordMic").checked,
          microphoneName: $("#micSelect").value ?? "None",
        });
      } catch {
        // Meters stay on the previous device; recording path unaffected.
      }
    }, 300);
  }
  $("#micSelect").addEventListener("change", restartPreviewsSoon);
  $("#recordMic").addEventListener("change", restartPreviewsSoon);
  $("#recordSystemAudio").addEventListener("change", restartPreviewsSoon);

  document.addEventListener("keydown", handleKeyCapture);
  document.addEventListener("keyup", (e) => {
    if (e.key === "Escape") cancelHotkeyCapture();
  });
});
