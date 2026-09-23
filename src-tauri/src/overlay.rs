use tauri::{
    AppHandle, LogicalPosition, Manager, Position, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};
use tauri_plugin_global_shortcut::Shortcut;

pub fn create_overlay_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("overlay").is_some() {
        return Ok(());
    }

    let overlay_width = 180.0;
    let overlay_height = 44.0;
    let padding = 0.0;

    let window = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
        .title("Dr. Record Overlay")
        .inner_size(overlay_width, overlay_height)
        .always_on_top(true)
        .transparent(true)
        .decorations(false)
        .skip_taskbar(true)
        .focusable(false)
        .resizable(false)
        .shadow(false)
        .build()
        .map_err(|e| format!("Failed to create overlay: {}", e))?;

    window.set_cursor_visible(false).ok();

    let monitor = match app.get_webview_window("settings") {
        Some(w) => w.current_monitor().ok().flatten(),
        None => app.primary_monitor().ok().flatten(),
    };

    let pos = monitor_position(monitor.as_ref(), padding, |logical_w, logical_h| {
        let x = logical_w - overlay_width - padding;
        let y = logical_h - overlay_height - padding;
        (x, y)
    });
    let _ = window.set_position(Position::Logical(LogicalPosition { x: pos.0, y: pos.1 }));

    tracing::info!("Overlay window created at ({}, {})", pos.0, pos.1);
    Ok(())
}

pub fn close_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.close();
        tracing::info!("Overlay window closed");
    }
}

// ---------------------------------------------------------------------------
// Annotation overlay (Feature 2): transparent fullscreen burn-in layer.
// Separate from the REC pill above; closing/disarming it never touches the
// recording. gdigrab captures its pixels automatically (burn-in).
// ---------------------------------------------------------------------------

/// Fullscreen transparent annotation window (`annotation.html` + canvas 2D).
/// Fullscreen covers the virtual-desktop union enumerated by
/// `recorder::enum_monitors` (negative origins included); per-monitor
/// captures built by `resolve_monitor`/`build_ffmpeg_args` clip outside
/// pixels, so strokes are never offset or mirrored. Canvas DPR scaling
/// mirrors the `monitor_position` size/scale math so cursor and mark agree.
///
/// Monitor-origin note: the union origin may be negative (monitor left of /
/// above primary). Cursor→capture offsets derive from
/// `recorder::monitor_origin_for(source)` via
/// `annotation::map_to_capture(pt, origin, dpr)`; client coords inside this
/// union-spanning window are already union-relative, so no per-monitor
/// window reposition is applied here.
/// Known limitation (documented, not repositioned): a single fullscreen
/// layer cannot be moved per monitor without risking burn-in misalignment
/// with the gdigrab offset/video_size rect, so strokes outside the recorded
/// monitor are clipped, never remapped. The toolbar (Player drawbar look)
/// stays visible for the whole armed session by user demand and hides with
/// the window on disarm; marks already captured persist in the video track
/// regardless.
pub fn create_annotation_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("annotation").is_some() {
        return Ok(());
    }

    let window =
        WebviewWindowBuilder::new(app, "annotation", WebviewUrl::App("annotation.html".into()))
            .title("Dr. Record Annotation")
            .transparent(true)
            .decorations(false)
            .always_on_top(true)
            .skip_taskbar(true)
            // Focusable (unlike the REC pill) so Esc exits draw mode and the
            // text-tool input receives keys once armed. Not focused at
            // creation so starting a recording never steals the user's
            // focus; `set_annotation_clickthrough` focuses on arm.
            //
            // Hidden at birth (`visible(false)`): a transparent fullscreen
            // WebView flashes white on some GPUs/drivers until first paint,
            // which reads as a "white strip" over the recording. The layer
            // is shown only while armed and hidden on every disarm, so a
            // disarmed recorder is pixel-silent (nothing on screen except
            // the user's own strokes while drawing).
            .focusable(true)
            .focused(false)
            .visible(false)
            .resizable(false)
            .shadow(false)
            .fullscreen(true)
            .build()
            .map_err(|e| format!("Failed to create annotation window: {}", e))?;

    // Disarmed by default: mouse passes through to windows below.
    let _ = window.set_ignore_cursor_events(true);

    tracing::info!("Annotation window created (disarmed, click-through)");
    Ok(())
}

pub fn close_annotation_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("annotation") {
        let _ = window.close();
        tracing::info!("Annotation window closed (marks already captured persist)");
    }
}

/// Post-stop save dialog (`save-dialog.html`): created ONCE at startup,
/// hidden, and only shown/hidden per take. Window CREATION from the stop
/// path (hotkey-event thread) wedges `WebviewWindowBuilder::build()`
/// forever, so creation lives here in the setup context and the stop path
/// only calls show/focus (non-blocking).
pub fn create_save_dialog_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("save-dialog").is_some() {
        return Ok(());
    }
    use tauri::{WebviewUrl, WebviewWindowBuilder};
    WebviewWindowBuilder::new(
        app,
        "save-dialog",
        WebviewUrl::App("save-dialog.html".into()),
    )
    .title("Save recording")
    .inner_size(440.0, 320.0)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .build()
    .map(|_| ())
    .map_err(|e| format!("Failed to create save dialog: {}", e))?;
    tracing::info!("Save dialog window created (hidden until a take finalizes)");
    Ok(())
}

pub fn hide_save_dialog_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("save-dialog") {
        let _ = window.hide();
    }
}

/// Canonical global Esc shortcut string: whichever spelling the hotkey
/// parser accepts. Registration happens once at startup (proven context);
/// NEVER register/unregister from inside the hotkey-event handler — the
/// blocking main-thread dispatch deadlocks there and wedges the whole stop.
pub fn annotation_esc_shortcut() -> &'static str {
    use std::str::FromStr;
    if Shortcut::from_str("Escape").is_ok() {
        "Escape"
    } else {
        "Esc"
    }
}

/// Disarmed (`ignore=true`) = passthrough AND hidden; armed
/// (`ignore=false`) = drawn + shown + focused. The layer is visible only
/// while the user is actively annotating, so a disarmed recorder never
/// flashes a white strip over the capture. Never touches
/// `RecorderState::is_recording`.
///
/// NOTE: no shortcut (un)registration here — see `annotation_esc_shortcut`.
/// Each step is logged so a wedged window call can never hide silently.
pub fn set_annotation_clickthrough(app: &AppHandle, ignore: bool) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("annotation") {
        tracing::info!("annotation clickthrough -> ignore={}", ignore);
        window
            .set_ignore_cursor_events(ignore)
            .map_err(|e| format!("Failed to set annotation passthrough: {}", e))?;
        tracing::info!("annotation ignore-cursor-events ok");
        if !ignore {
            tracing::info!("annotation showing window...");
            let _ = window.show();
            tracing::info!("annotation window shown, focusing...");
            let _ = window.set_focus();
            tracing::info!("annotation armed and visible");
        } else {
            tracing::info!("annotation hiding window...");
            let _ = window.hide();
            tracing::info!("annotation hidden");
        }
    }
    Ok(())
}

pub fn create_settings_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("settings").is_some() {
        return Ok(());
    }

    let win_w = 560.0;
    let win_h = 720.0;
    let padding = 20.0;

    let window = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("Dr. Record Settings")
        .inner_size(win_w, win_h)
        .min_inner_size(480.0, 560.0)
        .resizable(true)
        .build()
        .map_err(|e| format!("Failed to create settings: {}", e))?;

    let monitor = app.primary_monitor().ok().flatten();
    let pos = monitor_position(monitor.as_ref(), padding, |logical_w, logical_h| {
        let x = (logical_w - win_w) / 2.0;
        let y = (logical_h - win_h) / 2.0;
        (x, y)
    });
    let _ = window.set_position(Position::Logical(LogicalPosition { x: pos.0, y: pos.1 }));

    let w = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            let _ = w.hide();
            api.prevent_close();
        }
    });

    tracing::info!("Settings window created");
    Ok(())
}

fn monitor_position(
    monitor: Option<&tauri::Monitor>,
    padding: f64,
    calc: impl Fn(f64, f64) -> (f64, f64),
) -> (f64, f64) {
    match monitor {
        Some(m) => {
            let size = m.size();
            let scale = m.scale_factor();
            let origin = m.position();
            let logical_ox = origin.x as f64 / scale;
            let logical_oy = origin.y as f64 / scale;
            let logical_w = size.width as f64 / scale;
            let logical_h = size.height as f64 / scale;
            let (x, y) = calc(logical_w, logical_h);
            ((logical_ox + x).max(0.0), (logical_oy + y).max(0.0))
        }
        None => (padding, padding),
    }
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        let _ = create_settings_window(app);
    }
}
