import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

let capturingHotkey = false;
let currentHotkey = "Ctrl+Shift+R";

const $ = (s) => document.querySelector(s);

async function loadConfig() {
  try {
    const config = await invoke("load_config");
    $("#outputDir").value = config.output_dir || "";
    $("#hotkeyInput").value = config.hotkey || "Ctrl+Shift+R";
    currentHotkey = config.hotkey || "Ctrl+Shift+R";
    document.querySelector(`input[name="mode"][value="${config.recording_mode || "fullscreen"}"]`).checked = true;
    $("#showOverlay").checked = config.show_overlay !== false;

    if (config.first_run) {
      $("#firstRunBanner").style.display = "block";
    } else {
      $("#firstRunBanner").style.display = "none";
    }
  } catch (e) {
    showNotification("Failed to load config: " + e, "error");
  }
}

async function saveConfigAndStart() {
  const config = {
    output_dir: $("#outputDir").value,
    hotkey: currentHotkey,
    recording_mode: document.querySelector("input[name='mode']:checked").value,
    framerate: 30,
    show_overlay: $("#showOverlay").checked,
    first_run: false,
  };

  if (!config.output_dir) {
    showNotification("Please select an output directory first", "error");
    return;
  }

  try {
    await invoke("save_config", { config });
    await invoke("setup_first_run");
    showNotification("Config saved! Hotkey: " + config.hotkey, "success");
    setTimeout(() => window.close(), 1200);
  } catch (e) {
    showNotification("Failed to save: " + e, "error");
  }
}

async function pickDirectory() {
  try {
    const dir = await open({ directory: true, multiple: false, title: "Choose output folder" });
    if (dir) $("#outputDir").value = dir;
  } catch (e) {
    showNotification("Failed to pick directory: " + e, "error");
  }
}

function startHotkeyCapture() {
  capturingHotkey = true;
  const input = $("#hotkeyInput");
  input.value = "Press shortcut...";
  input.classList.add("capturing");
  $("#captureHotkeyBtn").textContent = "Listening...";
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

  const parts = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push("Win");

  const map = {
    "Control": null, "Shift": null, "Alt": null, "Meta": null,
    "Escape": null, "Tab": null, "CapsLock": null,
  };

  if (!map[key] && key.length >= 1) {
    parts.push(key.length === 1 ? key.toUpperCase() : key);
  }

  if (parts.length < 2) return;

  const combo = parts.join("+");
  currentHotkey = combo;
  $("#hotkeyInput").value = combo;
  cancelHotkeyCapture();
  showNotification("Hotkey set to: " + combo, "success");
}

function cancelHotkeyCapture() {
  capturingHotkey = false;
  const input = $("#hotkeyInput");
  input.classList.remove("capturing");
  $("#captureHotkeyBtn").textContent = "Capture";
  if (!input.value || input.value === "Press shortcut...") {
    input.value = currentHotkey;
  }
}

async function updateStatus() {
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

    await invoke("start_rec");
    showNotification("Recording test... stopping in 5s", "success");
    updateStatus();

    setTimeout(async () => {
      try {
        const result = await invoke("stop_rec");
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

function showNotification(msg, type = "info") {
  const el = $("#notification");
  el.textContent = msg;
  el.className = "notification show " + type;
  clearTimeout(el._hide);
  el._hide = setTimeout(() => el.classList.remove("show"), 3000);
}

document.addEventListener("DOMContentLoaded", async () => {
  await loadConfig();
  updateStatus();

  const interval = setInterval(updateStatus, 1000);

  const unlistenStatus = await listen("status-changed", () => updateStatus());
  const unlistenError = await listen("recording-error", (e) => {
    showNotification("Recording error: " + e.payload, "error");
  });

  window.addEventListener("beforeunload", () => {
    clearInterval(interval);
    unlistenStatus();
    unlistenError();
  });

  $("#pickDirBtn").addEventListener("click", pickDirectory);
  $("#captureHotkeyBtn").addEventListener("click", startHotkeyCapture);
  $("#saveBtn").addEventListener("click", saveConfigAndStart);
  $("#testRecBtn").addEventListener("click", testRecord);

  document.addEventListener("keydown", handleKeyCapture);
  document.addEventListener("keyup", (e) => {
    if (e.key === "Escape") cancelHotkeyCapture();
  });
});
