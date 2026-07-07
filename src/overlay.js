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

  const unlistenStop = await listen("recording-stopped", () => {
    clearInterval(timer);
    window.close();
  });

  window.addEventListener("beforeunload", () => {
    clearInterval(timer);
    unlistenStop();
  });

  updateTimer();
});
