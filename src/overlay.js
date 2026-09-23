import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

function formatTime(secs) {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

async function updateTimer() {
  const el = document.getElementById("recTime");
  if (!el) return;
  try {
    const secs = await invoke("get_elapsed_secs");
    el.textContent = formatTime(secs);
  } catch {
    // not recording
  }
}

document.addEventListener("DOMContentLoaded", async () => {
  const timer = setInterval(updateTimer, 1000);

  // Annotation toggle: button AND hotkey/Esc paths converge on the backend
  // `toggle_annotation` command; toggling never stops the recording.
  let toggling = false;
  const annotateBtn = document.getElementById("annotateBtn");
  const armedDot = document.getElementById("armedDot");
  async function toggleAnnotate(source) {
    if (toggling) return; // double-click = single toggle
    toggling = true;
    try {
      await invoke("toggle_annotation", { source });
    } catch (e) {
      // Disabled while not recording; pill stays, nothing crashes.
      console.warn("annotate toggle:", e);
    } finally {
      setTimeout(() => { toggling = false; }, 300);
    }
  }
  if (annotateBtn) {
    annotateBtn.addEventListener("click", () => toggleAnnotate("button"));
  }

  const unlistenAnnotate = await listen("annotation-state", (e) => {
    const armed = !!e.payload?.armed;
    const label = document.getElementById("annotateLabel");
    if (annotateBtn) {
      annotateBtn.classList.toggle("is-armed", armed);
      annotateBtn.setAttribute("aria-pressed", armed ? "true" : "false");
      annotateBtn.title = armed ? "Drawing — click to stop annotating (Esc)" : "Annotate (draw on screen)";
      annotateBtn.setAttribute("aria-label", armed ? "Stop annotating (Esc exits draw mode)" : "Toggle annotation draw mode");
    }
    if (label) label.textContent = armed ? "Drawing" : "Annotate";
    if (armedDot) armedDot.style.display = armed ? "" : "none";
  });

  const unlistenStop = await listen("recording-stopped", () => {
    clearInterval(timer);
    window.close();
  });

  const unlistenStatus = await listen("status-changed", (e) => {
    if (e.payload === "saving") {
      clearInterval(timer);
      const label = document.querySelector(".rec-label");
      if (label) label.textContent = "SAVING";
      const dot = document.getElementById("recDot");
      if (dot) dot.style.backgroundColor = "orange";
      const time = document.getElementById("recTime");
      if (time) time.style.display = "none";
      // Saving/mux runs synchronously in the backend: further toggles
      // would queue behind it and look like a hang/crash, so park the
      // pen until the save dialog resolves.
      if (annotateBtn) {
        annotateBtn.disabled = true;
        annotateBtn.classList.remove("is-armed");
        annotateBtn.title = "Saving…";
      }
    }
  });

  window.addEventListener("beforeunload", () => {
    clearInterval(timer);
    unlistenStop();
    unlistenStatus();
    unlistenAnnotate();
  });

  updateTimer();
});
