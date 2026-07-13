use tauri::{AppHandle, LogicalPosition, Manager, Position, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tracing;

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
    let _ = window.set_position(Position::Logical(LogicalPosition {
        x: pos.0,
        y: pos.1,
    }));

    tracing::info!("Overlay window created at ({}, {})", pos.0, pos.1);
    Ok(())
}

pub fn close_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("overlay") {
        let _ = window.close();
        tracing::info!("Overlay window closed");
    }
}

pub fn create_settings_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("settings").is_some() {
        return Ok(());
    }

    let win_w = 520.0;
    let win_h = 600.0;
    let padding = 20.0;

    let window = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("Dr. Record Settings")
        .inner_size(win_w, win_h)
        .resizable(false)
        .build()
        .map_err(|e| format!("Failed to create settings: {}", e))?;

    let monitor = app.primary_monitor().ok().flatten();
    let pos = monitor_position(monitor.as_ref(), padding, |logical_w, logical_h| {
        let x = (logical_w - win_w) / 2.0;
        let y = (logical_h - win_h) / 2.0;
        (x, y)
    });
    let _ = window.set_position(Position::Logical(LogicalPosition {
        x: pos.0,
        y: pos.1,
    }));

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
