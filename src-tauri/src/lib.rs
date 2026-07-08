mod config;
mod overlay;
mod recorder;

use config::Config;
use overlay::{close_overlay, create_overlay_window, create_settings_window, show_settings};
use recorder::{get_elapsed, start_recording, stop_recording, RecorderState};
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[cfg(target_os = "windows")]
fn set_auto_start(enabled: bool) {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let key = r"Software\Microsoft\Windows\CurrentVersion\Run";
    if let Ok(run) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(key, winreg::enums::KEY_SET_VALUE) {
        if enabled {
            if let Ok(exe) = std::env::current_exe() {
                let _ = run.set_value("Dr.Record", &format!("\"{}\" --autostart", exe.to_string_lossy()));
            }
        } else {
            let _ = run.delete_value("Dr.Record");
        }
    }
}
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, RunEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing;

fn handle_hotkey(app: &AppHandle) {
    let state = app.state::<Arc<RecorderState>>();
    let config = Config::load();
    let rec = |s: &RecorderState| -> bool { s.is_recording.load(Ordering::SeqCst) };

    if rec(&state) {
        match stop_recording(&state) {
            Ok(path) => {
                tracing::info!("Saved: {:?}", path);
                let _ = app.emit("recording-stopped", path);
                close_overlay(app);
                let _ = app.emit("status-changed", "stopped");
            }
            Err(e) => tracing::error!("Stop error: {}", e),
        }
    } else {
        match start_recording(&state, &config.output_dir, &config.recording_mode, config.framerate, &config.quality) {
            Ok(path) => {
                tracing::info!("Started: {}", path);
                let _ = app.emit("recording-started", &path);
                if config.show_overlay {
                    let _ = create_overlay_window(app);
                }
                let _ = app.emit("status-changed", "recording");
            }
            Err(e) => {
                tracing::error!("Start error: {}", e);
                let _ = app.emit("recording-error", &e);
            }
        }
    }
}

#[tauri::command]
fn start_rec(app: AppHandle, state: tauri::State<Arc<RecorderState>>) -> Result<String, String> {
    let config = Config::load();
    if state.is_recording.load(Ordering::SeqCst) {
        return Err("Already recording".to_string());
    }
    let result = start_recording(&state, &config.output_dir, &config.recording_mode, config.framerate, &config.quality)?;
    if config.show_overlay {
        create_overlay_window(&app)?;
    }
    let _ = app.emit("status-changed", "recording");
    Ok(result)
}

#[tauri::command]
fn stop_rec(app: AppHandle, state: tauri::State<Arc<RecorderState>>) -> Result<Option<String>, String> {
    if !state.is_recording.load(Ordering::SeqCst) {
        return Err("Not recording".to_string());
    }
    let result = stop_recording(&state)?;
    close_overlay(&app);
    let _ = app.emit("status-changed", "stopped");
    Ok(result)
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
fn save_config(config: Config) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    set_auto_start(config.auto_start);
    config.save();
    Ok(())
}

#[tauri::command]
fn get_output_path(state: tauri::State<Arc<RecorderState>>) -> Result<Option<String>, String> {
    Ok(state.output_path.lock().unwrap_or_else(|e| e.into_inner()).clone())
}

#[tauri::command]
fn open_settings_window(app: AppHandle) -> Result<(), String> {
    show_settings(&app);
    Ok(())
}

#[tauri::command]
fn reload_hotkey(app: AppHandle, hotkey: String, state: tauri::State<Arc<RecorderState>>) -> Result<(), String> {
    tracing::info!("Reloading hotkey: {}", hotkey);
    let _ = app.global_shortcut().unregister_all();
    app.global_shortcut().register(hotkey.as_str())
        .map_err(|e| format!("Failed to register '{}': {}", hotkey, e))?;
    *state.hotkey_str.lock().unwrap_or_else(|e| e.into_inner()) = hotkey.clone();
    tracing::info!("Hotkey active: {}", hotkey);
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter("dr_record=info")
        .init();

    let recorder_state = Arc::new(RecorderState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new()
            .with_handler(|app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    handle_hotkey(app);
                }
            })
            .build())
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
            tray.on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "settings" => show_settings(app),
                        "quit" => {
                            let state = app.state::<Arc<RecorderState>>();
                            if state.is_recording.load(Ordering::SeqCst) {
                                let _ = stop_recording(&state);
                                close_overlay(app);
                            }
                            app.exit(0);
                        }
                        _ => {}
                    }
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

            // Register initial hotkey
            let hk = config.hotkey.clone();
            app_handle.global_shortcut().register(hk.as_str())
                .map_err(|e| format!("Failed to register hotkey '{}': {}", hk, e))?;
            let state = app_handle.state::<Arc<RecorderState>>();
            *state.hotkey_str.lock().unwrap_or_else(|e| e.into_inner()) = hk.clone();
            tracing::info!("Hotkey registered: {}", hk);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_rec,
            stop_rec,
            get_status,
            get_elapsed_secs,
            load_config,
            save_config,
            get_output_path,
            open_settings_window,
            reload_hotkey,
            hide_settings,
            get_display_info,
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
