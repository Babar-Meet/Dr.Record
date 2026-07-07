mod config;
mod overlay;
mod recorder;

use config::Config;
use overlay::{close_overlay, create_overlay_window, create_settings_window, show_settings};
use recorder::{get_elapsed, start_recording, stop_recording, RecorderState};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, RunEvent,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use tracing;

fn setup_shortcut(app: &AppHandle, hotkey: &str) -> Result<(), String> {
    let parts: Vec<&str> = hotkey.split('+').collect();
    if parts.is_empty() {
        return Err("Invalid hotkey".to_string());
    }

    let key_part = parts.last().unwrap().trim();
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut meta = false;

    for p in &parts[..parts.len() - 1] {
        match p.trim().to_lowercase().as_str() {
            "ctrl" | "control" => ctrl = true,
            "alt" => alt = true,
            "shift" => shift = true,
            "win" | "cmd" | "meta" | "super" => meta = true,
            _ => {}
        }
    }

    let mut modifiers = Modifiers::empty();
    if ctrl { modifiers |= Modifiers::CONTROL; }
    if alt { modifiers |= Modifiers::ALT; }
    if shift { modifiers |= Modifiers::SHIFT; }
    if meta { modifiers |= Modifiers::META; }

    let code = match key_part.to_lowercase().as_str() {
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "q" => Code::KeyQ,
        "f1" => Code::F1,
        "f2" => Code::F2,
        "f3" => Code::F3,
        "f4" => Code::F4,
        "f5" => Code::F5,
        "f6" => Code::F6,
        "f7" => Code::F7,
        "f8" => Code::F8,
        "f9" => Code::F9,
        "f10" => Code::F10,
        "f11" => Code::F11,
        "f12" => Code::F12,
        "space" => Code::Space,
        "escape" | "esc" => Code::Escape,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "delete" => Code::Delete,
        "backspace" => Code::Backspace,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" => Code::PageUp,
        "pagedown" => Code::PageDown,
        "insert" => Code::Insert,
        "pause" => Code::Pause,
        "scrolllock" => Code::ScrollLock,
        "capslock" => Code::CapsLock,
        "numlock" => Code::NumLock,
        "printscreen" => Code::PrintScreen,
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        _ => return Err(format!("Unknown key: {}", key_part)),
    };

    let shortcut = Shortcut::new(Some(modifiers), code);

    let app_handle = app.clone();
    app.global_shortcut().on_shortcut(shortcut.clone(), move |_app, _event, _seq| {
        let state = app_handle.state::<Arc<RecorderState>>();
        let config = Config::load();

        if state.is_recording.load(Ordering::SeqCst) {
            match stop_recording(&state) {
                Ok(path) => {
                    tracing::info!("Recording saved to: {:?}", path);
                    let _ = app_handle.emit("recording-stopped", path);
                    close_overlay(&app_handle);
                    let _ = app_handle.emit("status-changed", "stopped");
                }
                Err(e) => {
                    tracing::error!("Error stopping: {}", e);
                }
            }
        } else {
            match start_recording(&state, &config.output_dir, &config.recording_mode, config.framerate) {
                Ok(path) => {
                    tracing::info!("Recording started: {}", path);
                    let _ = app_handle.emit("recording-started", &path);
                    if config.show_overlay {
                        let _ = create_overlay_window(&app_handle);
                    }
                    let _ = app_handle.emit("status-changed", "recording");
                }
                Err(e) => {
                    tracing::error!("Error starting: {}", e);
                    let _ = app_handle.emit("recording-error", &e);
                }
            }
        }
    }).map_err(|e| format!("Failed to register shortcut: {}", e))?;

    tracing::info!("Shortcut registered: {}", hotkey);
    Ok(())
}

#[tauri::command]
fn start_rec(app: AppHandle, state: tauri::State<Arc<RecorderState>>) -> Result<String, String> {
    let config = Config::load();
    if state.is_recording.load(Ordering::SeqCst) {
        return Err("Already recording".to_string());
    }

    let result = start_recording(&state, &config.output_dir, &config.recording_mode, config.framerate)?;
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
    let mut cfg = config;
    if !cfg.first_run {
        let old = Config::load();
        cfg.first_run = old.first_run;
    }
    cfg.save();
    Ok(())
}

#[tauri::command]
fn get_output_path(state: tauri::State<Arc<RecorderState>>) -> Result<Option<String>, String> {
    Ok(state.output_path.lock().unwrap().clone())
}

#[tauri::command]
fn setup_first_run(app: AppHandle) -> Result<(), String> {
    let mut config = Config::load();
    config.first_run = false;
    config.save();

    let hotkey = config.hotkey.clone();
    setup_shortcut(&app, &hotkey)?;
    Ok(())
}

#[tauri::command]
fn open_settings_window(app: AppHandle) -> Result<(), String> {
    show_settings(&app);
    Ok(())
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
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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
            TrayIconBuilder::new()
                .tooltip("Dr. Record")
                .icon_as_template(true)
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "settings" => {
                            show_settings(app);
                        }
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
                        let app = tray.app_handle();
                        show_settings(&app);
                    }
                })
                .build(app)?;

            let config = Config::load();
            tracing::info!("Config loaded. first_run: {}, hotkey: {}, path: {:?}", config.first_run, config.hotkey, Config::config_path());

            if config.first_run {
                create_settings_window(&app_handle)?;
            } else {
                let hotkey = config.hotkey.clone();
                if let Err(e) = setup_shortcut(&app_handle, &hotkey) {
                    tracing::error!("Failed to setup shortcut: {}", e);
                }
            }

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
            setup_first_run,
            open_settings_window,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let RunEvent::ExitRequested { .. } = event {
            }
        });
}
