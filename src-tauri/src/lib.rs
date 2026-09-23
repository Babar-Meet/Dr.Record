pub mod annotation;
mod audio;
pub mod av_sync;
mod config;
mod overlay;
mod recorder;
pub mod save_dialog;

use config::Config;
use overlay::{
    annotation_esc_shortcut, close_annotation_window, close_overlay, create_annotation_window,
    create_overlay_window, create_save_dialog_window, create_settings_window,
    hide_save_dialog_window, set_annotation_clickthrough, show_settings,
};
use recorder::{
    cancel_pending_take, delete_take_files, discard_take_files, enum_monitors,
    get_av_offsets as recorder_av_offsets, get_elapsed, list_pending_takes, probe_video_info,
    rename_take_file, retry_finalize as recorder_retry_finalize, start_audio_previews,
    start_recording, stop_recording, stop_recording_ex, validate_output, AvOffsets, MonitorInfo,
    PendingTake, RecorderState, StopResult, WindowInfo,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

#[cfg(target_os = "windows")]
fn set_auto_start(enabled: bool) {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let key = r"Software\Microsoft\Windows\CurrentVersion\Run";
    if let Ok(run) =
        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(key, winreg::enums::KEY_SET_VALUE)
    {
        if enabled {
            if let Ok(exe) = std::env::current_exe() {
                let _ = run.set_value(
                    "Dr.Record",
                    &format!("\"{}\" --autostart", exe.to_string_lossy()),
                );
            }
        } else {
            let _ = run.delete_value("Dr.Record");
        }
    }
}

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalPosition, Manager, Position, RunEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

/// Parse a hotkey string exactly the way the global-shortcut plugin does at
/// registration, returning its stable id. Comparing ids (not strings) makes
/// routing immune to modifier order/case/format differences.
fn hotkey_id(s: &str) -> Option<u32> {
    use std::str::FromStr;
    Shortcut::from_str(s).map(|sc| sc.id()).ok()
}

/// Compare two hotkey strings: equal plugin ids first (exact), falling back
/// to order/case-insensitive modifier-set compare for strings the parser
/// rejects. "win"/"meta"/"super"/"command"/"cmd" count as one modifier.
fn hotkeys_equal(a: &str, b: &str) -> bool {
    if let (Some(x), Some(y)) = (hotkey_id(a), hotkey_id(b)) {
        return x == y;
    }
    fn norm(s: &str) -> (Vec<String>, String) {
        let mut mods: Vec<String> = Vec::new();
        let mut key = String::new();
        for part in s.split('+') {
            let p = part.trim().to_ascii_lowercase();
            if p.is_empty() {
                continue;
            }
            let m = match p.as_str() {
                "ctrl" | "control" => Some("ctrl"),
                "alt" | "option" => Some("alt"),
                "shift" => Some("shift"),
                "win" | "meta" | "super" | "command" | "cmd" => Some("meta"),
                _ => None,
            };
            if let Some(m) = m {
                if !mods.iter().any(|x| x == m) {
                    mods.push(m.to_string());
                }
            } else {
                key = p;
            }
        }
        mods.sort();
        (mods, key)
    }
    norm(a) == norm(b)
}

/// Emit the true status ("saving" while finalizing, else recording/stopped).
/// Stop paths emit "saving" optimistically; if the stop then errors, this
/// pulls the UI back to reality so it can never stick on SAVING forever.
fn emit_actual_status(app: &AppHandle, state: &RecorderState) {
    let status = if state.finalizing.load(Ordering::SeqCst) {
        "saving"
    } else if state.is_recording.load(Ordering::SeqCst) {
        "recording"
    } else {
        "stopped"
    };
    let _ = app.emit("status-changed", status);
}

fn handle_hotkey(app: &AppHandle) {
    let state = app.state::<Arc<RecorderState>>();
    let config = Config::load();
    let rec = |s: &RecorderState| -> bool { s.is_recording.load(Ordering::SeqCst) };

    // Start-during-mux/save-dialog never corrupts state: dialog resolves
    // first, only then may a new recording start; never two dialogs.
    if state.finalizing.load(Ordering::SeqCst) {
        tracing::info!("hotkey ignored: finalizing previous take");
        return;
    }
    if state.pending_dialog.load(Ordering::SeqCst) {
        tracing::info!("hotkey ignored: save dialog pending");
        return;
    }

    if rec(&state) {
        let _ = app.emit("status-changed", "saving");
        match stop_recording_ex(&state) {
            Ok(result) => {
                after_take_finalized(app, &state, &config, &result);
            }
            Err(e) => {
                tracing::error!("Stop error: {}", e);
                emit_actual_status(app, &state);
            }
        }
    } else {
        let state_arc = Arc::clone(&*state);
        match start_recording(&state_arc, &config, app) {
            Ok(path) => {
                tracing::info!("Started: {}", path);
                let _ = app.emit("recording-started", &path);
                if config.show_overlay {
                    let _ = create_overlay_window(app);
                }
                let _ = create_annotation_window(app);
                // Fresh session: prior marks never leak into the new take,
                // and late-loading webviews sync to disarmed via this event.
                let _ = app.emit("annotation-clear", serde_json::json!({}));
                let _ = app.emit(
                    "annotation-state",
                    serde_json::json!({ "armed": false, "tool": "pen", "source": "recording-start" }),
                );
                let _ = app.emit("status-changed", "recording");
            }
            Err(e) => {
                tracing::error!("Start error: {}", e);
                let _ = app.emit("recording-error", &e);
            }
        }
    }
}

/// (Re)start the settings level-meter previews on a FRESH thread.
/// cpal/WASAPI initializes COM per calling thread; restarting previews on
/// the hotkey-event (or any reused) thread fails with "Cannot change thread
/// mode after it is set", leaving meters dead after every take. A new
/// thread always has a clean COM apartment. Best-effort: failures only
/// affect meters, never recording.
fn spawn_audio_previews(state: &Arc<RecorderState>, config: &Config, app: &AppHandle) {
    let state = Arc::clone(state);
    let config = config.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        start_audio_previews(&state, &config, &app);
    });
}

/// Shared post-stop wiring: compat event + rich events + save dialog.
/// Non-technical user errors; technical detail stays in `tracing` logs.
fn after_take_finalized(
    app: &AppHandle,
    state: &Arc<RecorderState>,
    config: &Config,
    result: &StopResult,
) {
    tracing::info!(
        "take finalized: valid={} reason={} sys_offset={:?} mic_offset={:?} warnings={}",
        result.valid,
        result.validation_reason,
        result.offsets_ms.system,
        result.offsets_ms.mic,
        result.warnings.len()
    );
    let _ = app.emit("recording-stopped", result.final_path.clone());
    let _ = app.emit("take-finalized", result);
    if !result.warnings.is_empty() {
        let _ = app.emit("save-warning", &result.warnings);
    }
    if !result.valid {
        let _ = app.emit(
            "save-error",
            serde_json::json!({
                "take_id": result.take_id,
                "message": "This recording could not be finished. Your temporary files were kept so you can retry without re-recording.",
                "retryable": !result.retained.is_empty(),
                "detail": result.validation_reason,
            }),
        );
    }
    close_overlay(app);
    let _ = set_annotation_clickthrough(app, true);
    // Disarm everywhere (overlay pill, toolbar, canvas cursor) so a stale
    // "Drawing" state can never survive the stop; marks already captured
    // persist in the video track regardless.
    tracing::info!("after-take: fetching tool for disarm broadcast...");
    let tool = state
        .annotation_tool
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    tracing::info!("after-take: emitting annotation-state...");
    let _ = app.emit(
        "annotation-state",
        serde_json::json!({ "armed": false, "tool": tool, "source": "recording-stopped" }),
    );
    tracing::info!("after-take: emitting stopped...");
    let _ = app.emit("status-changed", "stopped");
    tracing::info!("after-take: opening save dialog...");
    open_save_dialog(app, result);
    tracing::info!("after-take: restarting audio previews...");
    spawn_audio_previews(state, config, app);
    tracing::info!("after-take: done.");
}

/// Post-stop save dialog: dedicated `save-dialog` webview, pre-created
/// hidden at startup; here it is only shown + focused (non-blocking). The
/// window is centered on the primary monitor so automation and muscle
/// memory can find it. The settings view modal in `index.html` listens for
/// the same `take-finalized` event as a fallback, so the dialog appears
/// even if the webview fails.
fn open_save_dialog(app: &AppHandle, result: &StopResult) {
    if app.get_webview_window("save-dialog").is_none() {
        // Defensive re-create (normally pre-created at setup). If this
        // fails the settings modal fallback still covers the take.
        if let Err(e) = create_save_dialog_window(app) {
            tracing::warn!(
                "save dialog webview failed (settings modal fallback): {}",
                e
            );
            return;
        }
    }
    if let Some(window) = app.get_webview_window("save-dialog") {
        if let Ok(Some(monitor)) = app.primary_monitor() {
            let size = monitor.size();
            let scale = monitor.scale_factor();
            let lw = size.width as f64 / scale;
            let lh = size.height as f64 / scale;
            let _ = window.set_position(Position::Logical(LogicalPosition {
                x: (lw - 440.0) / 2.0,
                y: (lh - 320.0) / 2.0,
            }));
        }
        let _ = window.show();
        let _ = window.set_focus();
    }
    tracing::info!("save dialog opened for take {}", result.take_id);
}

#[tauri::command]
fn hide_save_dialog(app: AppHandle) -> Result<(), String> {
    hide_save_dialog_window(&app);
    Ok(())
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
struct Sources {
    monitors: Vec<MonitorInfo>,
    windows: Vec<WindowInfo>,
}

/// Return all monitors and visible windows so the UI can build its dropdown.
#[tauri::command]
fn enum_sources() -> Result<Sources, String> {
    Ok(Sources {
        monitors: enum_monitors(),
        windows: vec![],
    })
}

#[tauri::command]
fn get_microphones() -> Result<Vec<String>, String> {
    Ok(audio::get_microphones())
}

#[tauri::command]
fn get_thumbnail(source: String) -> Result<Option<String>, String> {
    Ok(recorder::get_thumbnail(&source))
}

#[tauri::command]
fn start_rec(app: AppHandle, state: tauri::State<Arc<RecorderState>>) -> Result<String, String> {
    let config = Config::load();
    if state.is_recording.load(Ordering::SeqCst) {
        return Err("Already recording".to_string());
    }
    if state.finalizing.load(Ordering::SeqCst) {
        return Err("Still finalizing the previous take; try again in a moment.".to_string());
    }
    if state.pending_dialog.load(Ordering::SeqCst) {
        return Err("Resolve the save dialog first.".to_string());
    }
    let state_arc = Arc::clone(&*state);
    let result = start_recording(&state_arc, &config, &app)?;
    if config.show_overlay {
        create_overlay_window(&app)?;
    }
    let _ = create_annotation_window(&app);
    // Fresh session: prior marks never leak into the new take (same as the
    // hotkey start path above).
    let _ = app.emit("annotation-clear", serde_json::json!({}));
    let _ = app.emit(
        "annotation-state",
        serde_json::json!({ "armed": false, "tool": "pen", "source": "recording-start" }),
    );
    let _ = app.emit("status-changed", "recording");
    Ok(result)
}

#[tauri::command]
fn stop_rec(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
) -> Result<Option<String>, String> {
    if !state.is_recording.load(Ordering::SeqCst) {
        return Err("Not recording".to_string());
    }
    let config = Config::load();
    let _ = app.emit("status-changed", "saving");
    // Thin wrapper over the rich path so old callers keep working.
    let result = match stop_recording_ex(&state) {
        Ok(r) => r,
        Err(e) => {
            emit_actual_status(&app, &state);
            return Err(e);
        }
    };
    let final_path = result.final_path.clone();
    after_take_finalized(&app, &state.inner().clone(), &config, &result);
    Ok(final_path)
}

/// Rich stop result: offsets, validation, warnings, retained temps.
#[tauri::command]
fn stop_rec_ex(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
) -> Result<StopResult, String> {
    if !state.is_recording.load(Ordering::SeqCst) {
        return Err("Not recording".to_string());
    }
    let config = Config::load();
    let _ = app.emit("status-changed", "saving");
    let result = match stop_recording_ex(&state) {
        Ok(r) => r,
        Err(e) => {
            emit_actual_status(&app, &state);
            return Err(e);
        }
    };
    after_take_finalized(&app, &state.inner().clone(), &config, &result);
    Ok(result)
}

/// Re-attempt finalization from retained temps (no re-record).
#[tauri::command]
fn retry_finalize(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
    take_id: String,
) -> Result<StopResult, String> {
    let result = recorder_retry_finalize(&state, &take_id)?;
    let config = Config::load();
    after_take_finalized(&app, &state.inner().clone(), &config, &result);
    Ok(result)
}

/// Alias kept for the save-dialog UI wording ("Retry mux").
#[tauri::command]
fn retry_mux(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
    take_id: String,
) -> Result<StopResult, String> {
    retry_finalize(app, state, take_id)
}

/// Validate a final file without stopping anything (0-byte / moov scan +
/// null-decode when available). Never reports corrupt output as success.
#[tauri::command]
fn validate_take(path: String) -> Result<serde_json::Value, String> {
    let (verdict, reason) = validate_output(&path);
    Ok(serde_json::json!({
        "path": path,
        "valid": verdict == crate::av_sync::FinalFileVerdict::Valid,
        "reason": reason,
    }))
}

#[tauri::command]
fn rename_take(
    state: tauri::State<Arc<RecorderState>>,
    take_id: String,
    new_name: String,
    overwrite: Option<bool>,
) -> Result<String, String> {
    rename_take_file(&state, &take_id, &new_name, overwrite.unwrap_or(false))
}

#[tauri::command]
fn delete_take(state: tauri::State<Arc<RecorderState>>, take_id: String) -> Result<(), String> {
    delete_take_files(&state, &take_id)
}

#[tauri::command]
fn discard_take(state: tauri::State<Arc<RecorderState>>, take_id: String) -> Result<(), String> {
    discard_take_files(&state, &take_id)
}

/// File + settings info for the save popup: resolution/duration probed
/// from the final file (best-effort), size from disk, fps/quality/source
/// from the saved config. Never blocks saving: bounded probe, and every
/// field degrades gracefully.
#[tauri::command]
fn get_save_info(
    state: tauri::State<Arc<RecorderState>>,
    take_id: String,
) -> Result<serde_json::Value, String> {
    let pending = lock_take(&state).ok_or_else(|| "No pending take".to_string())?;
    if pending.take_id != take_id {
        return Err("Take id mismatch".to_string());
    }
    let path = pending
        .final_path
        .clone()
        .unwrap_or_else(|| pending.default_path.clone());
    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let config = Config::load();
    let (width, height, fps, duration_secs) = probe_video_info(&path);
    // fps display: probed value wins; configured rate next; "Auto" (0) is
    // never shown as "0 fps".
    let fps_display = fps.unwrap_or_else(|| {
        if config.framerate == 0 {
            "Auto".to_string()
        } else {
            format!("{} fps", config.framerate)
        }
    });
    Ok(serde_json::json!({
        "size_bytes": size,
        "width": width,
        "height": height,
        "fps": fps_display,
        "quality": config.quality,
        "source": config.recording_source,
        "duration_secs": duration_secs,
    }))
}

fn lock_take(state: &Arc<RecorderState>) -> Option<PendingTake> {
    state
        .pending_take
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

#[tauri::command]
fn get_pending_take(
    state: tauri::State<Arc<RecorderState>>,
) -> Result<Option<PendingTake>, String> {
    Ok(state
        .pending_take
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone())
}

#[tauri::command]
fn resolve_pending_take(
    state: tauri::State<Arc<RecorderState>>,
    action: String,
    take_id: Option<String>,
    new_name: Option<String>,
    overwrite: Option<bool>,
) -> Result<Option<String>, String> {
    match action.as_str() {
        "save" => {
            let pending = state
                .pending_take
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .ok_or_else(|| "No pending take".to_string())?;
            let id = take_id.unwrap_or(pending.take_id);
            let name = new_name.ok_or_else(|| "Missing new_name".to_string())?;
            rename_take_file(&state, &id, &name, overwrite.unwrap_or(false)).map(Some)
        }
        "delete" | "discard" => {
            let pending = state
                .pending_take
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
                .ok_or_else(|| "No pending take".to_string())?;
            let id = take_id.unwrap_or(pending.take_id);
            if action == "delete" {
                delete_take_files(&state, &id)?;
            } else {
                discard_take_files(&state, &id)?;
            }
            Ok(None)
        }
        _ => {
            // Cancel path: keep the file under its default name, no orphans.
            Ok(cancel_pending_take(&state))
        }
    }
}

#[tauri::command]
fn get_av_offsets(state: tauri::State<Arc<RecorderState>>) -> Result<AvOffsets, String> {
    Ok(recorder_av_offsets(&state))
}

// ---------------------------------------------------------------------------
// Annotation commands (additive; never touch the recording flag)
// ---------------------------------------------------------------------------

fn emit_annotation_state(app: &AppHandle, state: &Arc<RecorderState>, source: &str) {
    let armed = state.annotation_armed.load(Ordering::SeqCst);
    let tool = state
        .annotation_tool
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    tracing::info!(
        "annotation {} via {} (tool {})",
        if armed { "armed" } else { "disarmed" },
        source,
        tool
    );
    let _ = app.emit(
        "annotation-state",
        serde_json::json!({ "armed": armed, "tool": tool, "source": source }),
    );
}

fn set_annotation_armed(
    app: &AppHandle,
    state: &Arc<RecorderState>,
    armed: bool,
    source: &str,
) -> Result<bool, String> {
    // Mux/save runs synchronously: toggling mid-save queues behind it and
    // reads as a hang, so park annotation until the dialog resolves.
    if state.finalizing.load(Ordering::SeqCst) {
        return Err("Still saving the previous take; annotation unlocks when the save dialog appears.".to_string());
    }
    if state.pending_dialog.load(Ordering::SeqCst) {
        return Err("Resolve the save dialog first.".to_string());
    }
    if !state.is_recording.load(Ordering::SeqCst) {
        return Err("Annotation is unavailable while not recording.".to_string());
    }
    // Ensure the separate fullscreen window exists, then flip passthrough.
    let _ = create_annotation_window(app);
    set_annotation_clickthrough(app, !armed)?;
    state.annotation_armed.store(armed, Ordering::SeqCst);
    let owned: Arc<RecorderState> = Arc::clone(state);
    emit_annotation_state(app, &owned, source);
    Ok(armed)
}

/// Toggle annotation arm/disarm via overlay button, hotkey, or Esc.
/// Never stops or pauses the recording. Idempotent within a 300ms window
/// so hotkey/button double-fire toggles exactly once.
#[tauri::command]
fn toggle_annotation(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
    source: Option<String>,
) -> Result<serde_json::Value, String> {
    let src = source.unwrap_or_else(|| "button".to_string());
    let owned: Arc<RecorderState> = Arc::clone(&*state);
    let armed = toggle_annotation_inner(&app, &owned, &src)?;
    Ok(serde_json::json!({ "armed": armed }))
}

/// 300ms idempotency window for annotation toggles (double-fire safe).
/// Wall-clock counterpart of the tested
/// `annotation::AnnotationState::debounced_toggle` pure logic.
const ANNOTATION_DEBOUNCE_MS: u128 = 300;

fn toggle_annotation_inner(
    app: &AppHandle,
    state: &Arc<RecorderState>,
    src: &str,
) -> Result<bool, String> {
    let now = Instant::now();
    {
        let mut last = state
            .last_annotation_toggle
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(prev) = *last {
            if now.duration_since(prev).as_millis() < ANNOTATION_DEBOUNCE_MS {
                let armed = state.annotation_armed.load(Ordering::SeqCst);
                tracing::info!(
                    "annotation toggle debounced (duplicate within 300ms via {})",
                    src
                );
                return Ok(armed);
            }
        }
        *last = Some(now);
    }
    let armed = !state.annotation_armed.load(Ordering::SeqCst);
    set_annotation_armed(app, state, armed, src)
}

/// Annotation-hotkey path: same debounced toggle as the overlay button,
/// wired to `toggle_annotation("hotkey")` semantics. Never touches recording.
fn handle_annotation_hotkey(app: &AppHandle) {
    let state = app.state::<Arc<RecorderState>>();
    let owned: Arc<RecorderState> = Arc::clone(&*state);
    match toggle_annotation_inner(app, &owned, "hotkey") {
        Ok(armed) => tracing::info!("annotation hotkey: armed={}", armed),
        Err(e) => tracing::info!("annotation hotkey ignored: {}", e),
    }
}

/// (Re)register the record hotkey plus the annotation hotkey. Collision
/// guard: when both strings match (case-insensitive) the record hotkey
/// wins and annotation registration is skipped with a warning, so the two
/// actions can never fight over one shortcut.
fn register_record_and_annotation_hotkeys(app: &AppHandle, record: &str, annotation: &str) {
    let _ = app.global_shortcut().unregister_all();
    match app.global_shortcut().register(record) {
        Ok(_) => tracing::info!("Hotkey registered: {}", record),
        Err(e) => tracing::error!("Failed to register hotkey '{}': {}", record, e),
    }
    if hotkeys_equal(annotation, record) {
        tracing::warn!(
            "annotation hotkey '{}' collides with record hotkey; annotation hotkey skipped",
            annotation
        );
        return;
    }
    match app.global_shortcut().register(annotation) {
        Ok(_) => tracing::info!("Annotation hotkey registered: {}", annotation),
        Err(e) => tracing::error!(
            "Failed to register annotation hotkey '{}': {}",
            annotation,
            e
        ),
    }
    // Global Esc (handled in the shortcut handler: wipe + disarm only when
    // armed, ignored otherwise). Registered here — at startup and on every
    // hotkey reload — because registering from inside the hotkey-event
    // handler deadlocks on the main-thread dispatch.
    match app.global_shortcut().register(annotation_esc_shortcut()) {
        Ok(_) => tracing::info!("Esc shortcut registered: {}", annotation_esc_shortcut()),
        Err(e) => tracing::warn!(
            "Failed to register Esc shortcut '{}': {} (window Esc still works when focused)",
            annotation_esc_shortcut(),
            e
        ),
    }
}

#[tauri::command]
fn annotation_show(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
) -> Result<bool, String> {
    let owned: Arc<RecorderState> = Arc::clone(&*state);
    set_annotation_armed(&app, &owned, true, "button")
}

#[tauri::command]
fn annotation_hide(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
) -> Result<bool, String> {
    let owned: Arc<RecorderState> = Arc::clone(&*state);
    set_annotation_armed(&app, &owned, false, "esc")
}

#[tauri::command]
fn annotation_toggle(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
    source: Option<String>,
) -> Result<serde_json::Value, String> {
    toggle_annotation(app, state, source)
}

/// Clear request forwarded to the canvas (applies to subsequent frames;
/// already-recorded frames keep their marks).
#[tauri::command]
fn annotation_clear(app: AppHandle) -> Result<(), String> {
    let _ = app.emit("annotation-clear", serde_json::json!({}));
    tracing::info!("annotation clear-all requested");
    Ok(())
}

#[tauri::command]
fn set_annotation_tool(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
    tool: String,
    color: Option<String>,
    thickness: Option<String>,
) -> Result<(), String> {
    *state
        .annotation_tool
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = tool.clone();
    tracing::info!(
        "annotation tool: {} color={:?} thickness={:?}",
        tool,
        color,
        thickness
    );
    let _ = app.emit(
        "annotation-state",
        serde_json::json!({ "armed": state.annotation_armed.load(Ordering::SeqCst), "tool": tool, "source": "toolbar" }),
    );
    Ok(())
}

/// Current arm/tool snapshot for late-loading webviews (the annotation
/// window created mid-toggle misses the `annotation-state` event otherwise).
#[tauri::command]
fn get_annotation_state(state: tauri::State<Arc<RecorderState>>) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "armed": state.annotation_armed.load(Ordering::SeqCst),
        "tool": state.annotation_tool.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    }))
}

#[tauri::command]
fn get_status(state: tauri::State<Arc<RecorderState>>) -> Result<bool, String> {
    Ok(state.is_recording.load(Ordering::SeqCst))
}

#[tauri::command]
fn get_elapsed_secs(state: tauri::State<Arc<RecorderState>>) -> Result<u64, String> {
    Ok(get_elapsed(&state))
}

#[tauri::command]
fn load_config() -> Result<Config, String> {
    Ok(Config::load())
}

#[tauri::command]
fn save_config(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
    mut config: Config,
) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    set_auto_start(config.auto_start);
    // Auto-managed fields (loopback calibration) are NOT part of the settings
    // UI: the frontend doesn't send them, so keep the stored values instead
    // of resetting them to defaults on every save.
    let stored = Config::load();
    config.system_delay_ms = stored.system_delay_ms;
    config.system_delay_device = stored.system_delay_device;
    config.mic_delay_ms = stored.mic_delay_ms;
    config.save();

    if !state.is_recording.load(Ordering::SeqCst) {
        spawn_audio_previews(&state, &config, &app);
    }

    Ok(())
}

#[tauri::command]
fn get_output_path(state: tauri::State<Arc<RecorderState>>) -> Result<Option<String>, String> {
    Ok(state
        .output_path
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone())
}

#[tauri::command]
fn open_settings_window(app: AppHandle) -> Result<(), String> {
    show_settings(&app);
    Ok(())
}

#[tauri::command]
fn reload_hotkey(
    app: AppHandle,
    hotkey: String,
    state: tauri::State<Arc<RecorderState>>,
) -> Result<(), String> {
    tracing::info!("Reloading hotkey: {}", hotkey);
    *state.hotkey_str.lock().unwrap_or_else(|e| e.into_inner()) = hotkey.clone();
    let annotation = state
        .annotation_hotkey_str
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    // Re-register both so the annotation shortcut survives a record-hotkey
    // change (collision guard inside).
    register_record_and_annotation_hotkeys(&app, &hotkey, &annotation);
    tracing::info!("Hotkey active: {}", hotkey);
    Ok(())
}

/// Runtime re-registration for the annotation hotkey (persist via
/// `save_config`, which stores the whole `Config`). Additive; `reload_hotkey`
/// keeps its signature. Collisions with the record hotkey are rejected.
#[tauri::command]
fn reload_annotation_hotkey(
    app: AppHandle,
    hotkey: String,
    state: tauri::State<Arc<RecorderState>>,
) -> Result<(), String> {
    let record = state
        .hotkey_str
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if hotkeys_equal(&hotkey, &record) {
        return Err(format!(
            "Annotation hotkey '{}' must differ from the Start/Stop hotkey '{}'.",
            hotkey, record
        ));
    }
    tracing::info!("Reloading annotation hotkey: {}", hotkey);
    *state
        .annotation_hotkey_str
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = hotkey.clone();
    register_record_and_annotation_hotkeys(&app, &record, &hotkey);
    tracing::info!("Annotation hotkey active: {}", hotkey);
    Ok(())
}

/// Restart the settings level-meter previews from the CURRENT UI selections
/// (not the saved config): changing the mic dropdown or the system-audio
/// checkbox otherwise leaves the meters on the old device until the next
/// save + reopen. No-op while recording (meters belong to settings).
#[tauri::command]
fn restart_audio_previews(
    app: AppHandle,
    state: tauri::State<Arc<RecorderState>>,
    record_system_audio: bool,
    record_mic: bool,
    microphone_name: String,
) -> Result<(), String> {
    if state.is_recording.load(Ordering::SeqCst) {
        return Ok(());
    }
    let mut config = Config::load();
    config.record_system_audio = record_system_audio;
    config.record_mic = record_mic;
    config.microphone_name = microphone_name;
    spawn_audio_previews(&state, &config, &app);
    Ok(())
}

#[tauri::command]
fn hide_settings(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.hide();
    }
    Ok(())
}

#[tauri::command]
fn get_display_info() -> Result<serde_json::Value, String> {
    #[cfg(target_os = "windows")]
    {
        extern "system" {
            fn GetSystemMetrics(nIndex: i32) -> i32;
        }
        unsafe {
            let w = GetSystemMetrics(0);
            let h = GetSystemMetrics(1);
            if w > 0 && h > 0 {
                return Ok(serde_json::json!({ "width": w, "height": h, "refreshRate": 60 }));
            }
        }
    }
    Ok(serde_json::json!({ "width": 1920, "height": 1080, "refreshRate": 60 }))
}

// ---------------------------------------------------------------------------
// App entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Persistent app log: stdout is invisible in the installed app, so every
    // `tracing` line (stop stages, offsets, mux outcome, validation) is also
    // appended to %TEMP%\dr-record-app.log. Rotated by delete above 2 MB.
    let log_path = std::env::temp_dir().join("dr-record-app.log");
    if std::fs::metadata(&log_path).map(|m| m.len()).unwrap_or(0) > 2_000_000 {
        let _ = std::fs::remove_file(&log_path);
    }
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path);
    match log_file {
        Ok(f) => {
            tracing_subscriber::fmt()
                .with_env_filter("dr_record=info")
                .with_ansi(false)
                .with_writer(std::sync::Mutex::new(f))
                .init();
        }
        Err(_) => {
            tracing_subscriber::fmt()
                .with_env_filter("dr_record=info")
                .init();
        }
    }

    let recorder_state = Arc::new(RecorderState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        // One app only: a second launch (double-click while running) focuses
        // the existing settings window instead of spawning a rival recorder
        // that fights over hotkeys, audio devices, and the overlay.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_settings(app);
        }))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        // Route by plugin id, parsed from the same config
                        // string used at registration: the pressed combo can
                        // never be misread as the other action, no matter how
                        // modifiers are ordered/cased. A misroute here used to
                        // send the annotation key into start/stop.
                        let pressed_id = shortcut.id();
                        let ann_id = app
                            .try_state::<Arc<RecorderState>>()
                            .and_then(|s| {
                                let ann = s
                                    .annotation_hotkey_str
                                    .lock()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .clone();
                                hotkey_id(&ann)
                            });
                        let is_annotation = ann_id.is_some_and(|id| id == pressed_id);
                        // Global Esc grab (registered only while armed): wipe
                        // the layer and exit draw mode from anywhere. Explicit
                        // disarm, never toggle: a stale Esc must not re-arm.
                        let is_esc = hotkey_id(annotation_esc_shortcut())
                            .is_some_and(|id| id == pressed_id);
                        tracing::info!(
                            "hotkey pressed: {} (annotation={} esc={})",
                            shortcut.into_string(),
                            is_annotation,
                            is_esc
                        );
                        if is_esc {
                            // Esc wipes the layer and exits draw mode, but
                            // ONLY while armed: a stray Esc while disarmed
                            // must never touch the canvas.
                            if let Some(state) = app.try_state::<Arc<RecorderState>>() {
                                if state.annotation_armed.load(Ordering::SeqCst) {
                                    let owned: Arc<RecorderState> = Arc::clone(&*state);
                                    let _ = app.emit(
                                        "annotation-clear",
                                        serde_json::json!({}),
                                    );
                                    match set_annotation_armed(app, &owned, false, "esc") {
                                        Ok(_) => tracing::info!("annotation esc: wiped and disarmed"),
                                        Err(e) => tracing::info!("annotation esc ignored: {}", e),
                                    }
                                }
                            }
                        } else if is_annotation {
                            handle_annotation_hotkey(app);
                        } else {
                            handle_hotkey(app);
                        }
                    }
                })
                .build(),
        )
        .manage(recorder_state)
        .setup(|app| {
            let app_handle = app.handle();

            let settings_item = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&settings_item)
                .separator()
                .item(&quit_item)
                .build()?;

            let mut tray = TrayIconBuilder::new().tooltip("Dr. Record").menu(&menu);
            if let Some(icon) = app.default_window_icon().cloned() {
                tray = tray.icon(icon);
            }
            tray.on_menu_event(move |app, event| match event.id().as_ref() {
                "settings" => show_settings(app),
                "quit" => {
                    let state = app.state::<Arc<RecorderState>>();
                    if state.is_recording.load(Ordering::SeqCst) {
                        let _ = stop_recording(&state);
                        close_overlay(app);
                        close_annotation_window(app);
                    }
                    // A pending save dialog is already journaled at finalize
                    // time; quit keeps the journal so next launch re-offers
                    // the dialog instead of orphaning temps.
                    app.exit(0);
                }
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    show_settings(tray.app_handle());
                }
            })
            .build(app)?;

            let config = Config::load();
            tracing::info!("Hotkey from config: {}", config.hotkey);
            #[cfg(target_os = "windows")]
            set_auto_start(config.auto_start);

            let args: Vec<String> = std::env::args().collect();
            if !args.contains(&"--autostart".to_string()) {
                create_settings_window(app_handle)?;
            }

            // Pre-create the save dialog hidden (creation at stop time
            // wedges; the stop path only shows/focuses it).
            if let Err(e) = create_save_dialog_window(app_handle) {
                tracing::warn!("save dialog pre-create failed (settings modal fallback): {}", e);
            }

            // Register initial hotkeys (record + annotation, collision-guarded)
            let hk = config.hotkey.clone();
            let annotation_hk = config.annotation_hotkey.clone();
            register_record_and_annotation_hotkeys(app_handle, &hk, &annotation_hk);
            let state = app_handle.state::<Arc<RecorderState>>();
            *state.hotkey_str.lock().unwrap_or_else(|e| e.into_inner()) = hk.clone();
            *state
                .annotation_hotkey_str
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = annotation_hk.clone();

            // Crash/quit recovery: journaled pending takes re-offer the
            // dialog instead of leaving silent orphans.
            let pending = list_pending_takes(&config.output_dir);
            if !pending.is_empty() {
                tracing::warn!("{} unresolved take(s) from previous session", pending.len());
                if let Some(first) = pending.into_iter().next() {
                    *state.pending_take.lock().unwrap_or_else(|e| e.into_inner()) =
                        Some(first.clone());
                    state.pending_dialog.store(true, Ordering::SeqCst);
                    let result = StopResult {
                        take_id: first.take_id.clone(),
                        default_path: first.default_path.clone(),
                        final_path: first.final_path.clone(),
                        valid: first.valid,
                        validation_reason: if first.reason.is_empty() {
                            "recovered pending take".to_string()
                        } else {
                            first.reason.clone()
                        },
                        offsets_ms: recorder_av_offsets(&state),
                        warnings: Vec::new(),
                        retained: Vec::new(),
                    };
                    open_save_dialog(app_handle, &result);
                }
            }

            start_audio_previews(&state, &config, app_handle);

            // Background loopback-latency calibration (the user's "no manual
            // delay" requirement): measures the default output device once
            // per device with a short two-tone blip and stores the result in
            // `system_delay_ms`, which the mux applies automatically.
            // Never blocks startup, never breaks recording: any failure
            // keeps the previous value.
            std::thread::spawn(|| {
                let current = audio::default_output_name();
                let stored = Config::load();
                let needs = match &current {
                    // New device (or never calibrated): measure once.
                    Some(name) => *name != stored.system_delay_device,
                    // No output device: nothing to calibrate.
                    None => false,
                };
                if !needs {
                    return;
                }
                tracing::info!("calibrating output latency (short blip)...");
                match audio::calibrate_output_latency() {
                    Some((ms, device)) => {
                        let mut cfg = Config::load();
                        cfg.system_delay_ms = ms;
                        cfg.system_delay_device = device.clone();
                        cfg.save();
                        tracing::info!(
                            "output latency calibrated: {}ms on '{}' (auto-applied)",
                            ms,
                            device
                        );
                    }
                    None => tracing::warn!(
                        "output latency calibration failed; keeping {}ms",
                        stored.system_delay_ms
                    ),
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            enum_sources,
            get_thumbnail,
            start_rec,
            stop_rec,
            stop_rec_ex,
            retry_finalize,
            retry_mux,
            validate_take,
            rename_take,
            delete_take,
            discard_take,
            get_pending_take,
            get_save_info,
            resolve_pending_take,
            toggle_annotation,
            annotation_show,
            annotation_hide,
            annotation_toggle,
            annotation_clear,
            set_annotation_tool,
            get_annotation_state,
            get_av_offsets,
            get_status,
            get_elapsed_secs,
            load_config,
            save_config,
            get_output_path,
            open_settings_window,
            reload_hotkey,
            reload_annotation_hotkey,
            restart_audio_previews,
            hide_settings,
            get_display_info,
            get_microphones,
            hide_save_dialog,
        ])
        .build(tauri::generate_context!())
        .expect("error building tauri application")
        .run(|_app_handle, event| {
            if let RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotkey_compare_ignores_modifier_order_and_case() {
        assert!(hotkeys_equal("Ctrl+Shift+Alt+A", "ctrl+alt+shift+a"));
        assert!(hotkeys_equal("Ctrl+Shift+Alt+R", "Ctrl+Shift+Alt+R"));
    }

    #[test]
    fn hotkey_compare_rejects_different_bindings() {
        assert!(!hotkeys_equal("Ctrl+Shift+Alt+A", "Ctrl+Shift+Alt+R"));
        assert!(!hotkeys_equal("Ctrl+Shift+A", "Ctrl+Shift+Alt+A"));
        assert!(!hotkeys_equal("Ctrl+A", "Alt+A"));
    }

    #[test]
    fn hotkey_compare_unifies_win_meta_super() {
        assert!(hotkeys_equal("Win+Shift+S", "meta+shift+s"));
        assert!(hotkeys_equal("Super+Shift+S", "command+shift+s"));
    }
}
