use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tracing;

pub fn create_overlay_window(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("overlay").is_some() {
        return Ok(());
    }

    let monitor = match app.get_webview_window("settings") {
        Some(w) => w.current_monitor().ok().flatten(),
        None => None,
    };

    let (window_width, _window_height) = match &monitor {
        Some(m) => {
            let size = m.size();
            (size.width as f64, size.height as f64)
        }
        None => (1920.0, 1080.0),
    };

    let overlay_width = 180.0;
    let overlay_height = 44.0;
    let x = (window_width - overlay_width - 20.0).max(0.0);
    let y = 20.0;

    let window = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
        .title("Dr. Record Overlay")
        .inner_size(overlay_width, overlay_height)
        .position(x, y)
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

    tracing::info!("Overlay window created at ({}, {})", x, y);
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

    let window = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("index.html".into()))
        .title("Dr. Record Settings")
        .inner_size(520.0, 480.0)
        .resizable(false)
        .center()
        .build()
        .map_err(|e| format!("Failed to create settings: {}", e))?;

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

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        let _ = create_settings_window(app);
    }
}
