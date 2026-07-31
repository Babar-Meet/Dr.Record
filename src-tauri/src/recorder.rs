#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use chrono::Local;
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing;
use xcap::{Monitor, Window};
use crate::audio::AudioRecorder;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct RecorderState {
    pub is_recording: AtomicBool,
    pub process: Mutex<Option<Child>>,
    pub stdin: Mutex<Option<ChildStdin>>,
    pub output_path: Mutex<Option<String>>,
    pub hotkey_str: Mutex<String>,
    pub watch_hwnd: Mutex<usize>,
    pub sys_audio: Mutex<Option<AudioRecorder>>,
    pub mic_audio: Mutex<Option<AudioRecorder>>,
    pub sys_audio_path: Mutex<Option<String>>,
    pub mic_audio_path: Mutex<Option<String>>,
    pub start_time: Mutex<Option<Instant>>,
    pub preview_sys_audio: Mutex<Option<AudioRecorder>>,
    pub preview_mic_audio: Mutex<Option<AudioRecorder>>,
}

impl RecorderState {
    pub fn new() -> Self {
        Self {
            is_recording: AtomicBool::new(false),
            process: Mutex::new(None),
            stdin: Mutex::new(None),
            output_path: Mutex::new(None),
            hotkey_str: Mutex::new("Ctrl+Shift+R".to_string()),
            watch_hwnd: Mutex::new(0),
            sys_audio: Mutex::new(None),
            mic_audio: Mutex::new(None),
            sys_audio_path: Mutex::new(None),
            mic_audio_path: Mutex::new(None),
            start_time: Mutex::new(None),
            preview_sys_audio: Mutex::new(None),
            preview_mic_audio: Mutex::new(None),
        }
    }
}

fn source_label(source: &str) -> &str {
    if source.starts_with("monitor:") {
        "Monitor"
    } else if source.starts_with("window:") {
        "Window"
    } else {
        "Screen"
    }
}

pub fn generate_output_path(output_dir: &str, source: &str) -> String {
    let now = Local::now();
    let timestamp = now.format("%Y-%m-%d_%H-%M-%S");
    let label = source_label(source);
    format!(
        "{}\\DrRecord_{}_{}.mp4",
        output_dir.trim_end_matches('\\').trim_end_matches('/'),
        label,
        timestamp
    )
}

pub fn quality_to_crf(quality: &str) -> u32 {
    match quality {
        "lossless" => 0,
        "high" => 18,
        "low" => 28,
        _ => 23,
    }
}

/// Build FFmpeg args for gdigrab.
///
/// Sources:
///   "all"              – full virtual desktop
///   "monitor:<device>" – single monitor (e.g. "monitor:\\.\DISPLAY1"; legacy
///                        "monitor:N" indices also accepted), offset + video_size
///   "window:HWND"      – `title=<title>` input
fn build_ffmpeg_args(
    output_path: &str,
    source: &str,
    framerate: u32,
    quality: &str,
    monitors: &[MonitorInfo],
) -> Vec<String> {
    let crf = quality_to_crf(quality);
    let fps = if framerate == 0 { 60 } else { framerate };

    let mut args = vec![
        "-y".to_string(),
        "-f".to_string(), "gdigrab".to_string(),
        "-framerate".to_string(), fps.to_string(),
    ];

    if source.starts_with("monitor:") {
        if let Some(mon) = resolve_monitor(source, monitors) {
            let mut w = mon.width;
            let mut h = mon.height;
            if w % 2 != 0 { w -= 1; }
            if h % 2 != 0 { h -= 1; }
            
            args.extend_from_slice(&[
                "-offset_x".to_string(), mon.x.to_string(),
                "-offset_y".to_string(), mon.y.to_string(),
                "-video_size".to_string(), format!("{}x{}", w, h),
                "-i".to_string(), "desktop".to_string(),
            ]);
        } else {
            args.extend_from_slice(&["-i".to_string(), "desktop".to_string()]);
        }
    } else {
        // "all"
        args.extend_from_slice(&["-i".to_string(), "desktop".to_string()]);
    }

    args.extend_from_slice(&[
        "-c:v".to_string(), "libx264".to_string(),
        "-preset".to_string(), "ultrafast".to_string(),
        "-crf".to_string(), crf.to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        output_path.to_string(),
    ]);

    args
}

fn find_ffmpeg() -> String {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();

    let candidates = [
        exe_dir.join("ffmpeg.exe"),
        exe_dir.join("resources").join("ffmpeg.exe"),
        exe_dir.join("..").join("resources").join("ffmpeg.exe"),
    ];

    for path in &candidates {
        if path.exists() {
            tracing::info!("FFmpeg at: {:?}", path);
            return path.to_string_lossy().to_string();
        }
    }

    tracing::info!("No bundled FFmpeg, falling back to PATH");
    "ffmpeg".to_string()
}

fn ffmpeg_on_path() -> bool {
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    cmd.spawn().is_ok()
}

// ---------------------------------------------------------------------------
// Source enumeration types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
pub struct MonitorInfo {
    pub index: usize,
    /// Stable per-monitor identity. On Windows this is the device string
    /// (szDevice from MONITORINFOEXW), e.g. `\\.\DISPLAY1`. All consumers
    /// (dropdown, preview, capture) resolve the same physical monitor from
    /// this id so they can never disagree.
    pub device_id: String,
    pub label: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_primary: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WindowInfo {
    pub hwnd: usize,
    pub title: String,
}

/// Convert a Windows wide-char buffer to a Rust String, truncating at the
/// first null terminator.
#[cfg(target_os = "windows")]
fn device_string(dev: &[u16]) -> String {
    let len = dev.iter().position(|&c| c == 0).unwrap_or(dev.len());
    String::from_utf16_lossy(&dev[..len])
}

/// Resolve a `monitor:...` source string to the physical `MonitorInfo` it
/// refers to, using `monitors` (the primary-first enumeration) as the map.
///
/// New-style values carry the stable device id (`monitor:\\.\DISPLAY1`) and are
/// matched by identity. Legacy values carry a positional index
/// (`monitor:0`) and are treated as an index into the same enumeration, which
/// is the order the dropdown always displayed them in.
fn resolve_monitor<'a>(source: &str, monitors: &'a [MonitorInfo]) -> Option<&'a MonitorInfo> {
    let key = source.strip_prefix("monitor:")?;
    if let Some(m) = monitors.iter().find(|m| m.device_id == key) {
        return Some(m);
    }
    if let Ok(idx) = key.parse::<usize>() {
        return monitors.get(idx);
    }
    None
}

// ---------------------------------------------------------------------------
// Windows API source enumeration
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub fn enum_monitors() -> Vec<MonitorInfo> {
    use std::ffi::c_void;
    use std::mem;
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, MONITORINFOEXW,
    };

    // MONITORINFOF_PRIMARY is 0x00000001 per MSDN — not re-exported by windows-sys
    const MONITORINFOF_PRIMARY: u32 = 0x00000001;

    struct Collector {
        monitors: Vec<MonitorInfo>,
    }

    unsafe extern "system" fn callback(
        hmonitor: *mut c_void,
        _hdc: *mut c_void,
        _lprect: *mut RECT,
        lparam: isize,
    ) -> i32 {
        let col = &mut *(lparam as *mut Collector);
        let idx = col.monitors.len();

        let mut mi: MONITORINFOEXW = mem::zeroed();
        mi.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;

        if GetMonitorInfoW(hmonitor, &mut mi as *mut _ as *mut _) != 0 {
            let rc = mi.monitorInfo.rcMonitor;
            let is_primary = (mi.monitorInfo.dwFlags & MONITORINFOF_PRIMARY) != 0;
            let device_id = device_string(&mi.szDevice);
            let label = if is_primary {
                format!("Monitor {} (Primary)", idx + 1)
            } else {
                format!("Monitor {}", idx + 1)
            };
            col.monitors.push(MonitorInfo {
                index: idx,
                device_id,
                label,
                x: rc.left,
                y: rc.top,
                width: (rc.right - rc.left) as u32,
                height: (rc.bottom - rc.top) as u32,
                is_primary,
            });
        }
        1
    }

    let mut col = Collector { monitors: Vec::new() };
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(callback),
            &mut col as *mut _ as isize,
        );
    }
    // Primary first, then stable index
    col.monitors.sort_by(|a, b| b.is_primary.cmp(&a.is_primary));
    for (i, m) in col.monitors.iter_mut().enumerate() {
        m.index = i;
        if m.is_primary {
            m.label = format!("Monitor {} (Primary) [{}]", i + 1, m.device_id);
        } else {
            m.label = format!("Monitor {} [{}]", i + 1, m.device_id);
        }
    }
    col.monitors
}

#[cfg(not(target_os = "windows"))]
pub fn enum_monitors() -> Vec<MonitorInfo> {
    vec![MonitorInfo {
        index: 0,
        device_id: "default".to_string(),
        label: "Monitor 1 (Primary)".to_string(),
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        is_primary: true,
    }]
}



fn capture_monitor_png(monitor: &Monitor) -> Option<String> {
    let image = monitor.capture_image().ok()?;
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    image.write_to(&mut cursor, image::ImageFormat::Png).ok()?;
    use base64::Engine;
    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&buf)
    ))
}

pub fn get_thumbnail(source: &str) -> Option<String> {
    if let Some(key) = source.strip_prefix("monitor:") {
        let monitors = Monitor::all().ok()?;
        // Canonical identity: resolve the source to a device id via the same
        // primary-first enumeration the capture path uses, then find the xcap
        // monitor whose device name matches it. Legacy "monitor:<index>"
        // values resolve to a device id here too, so the preview always shows
        // the physical monitor that would be recorded.
        let known = enum_monitors();
        if let Some(info) = resolve_monitor(source, &known) {
            if let Some(m) = monitors
                .iter()
                .find(|m| m.name().ok().as_deref() == Some(info.device_id.as_str()))
            {
                return capture_monitor_png(m);
            }
        }
        // Defensive fallback for platforms/edge cases where device names can't
        // be matched (e.g. non-Windows stubs): keep the old positional-index
        // behavior so a preview is still produced.
        if let Ok(idx) = key.parse::<usize>() {
            if let Some(m) = monitors.get(idx) {
                return capture_monitor_png(m);
            }
        }
    } else if let Some(key) = source.strip_prefix("window:") {
        let hwnd: usize = key.parse().unwrap_or(0);
        let windows = Window::all().ok()?;
        if let Some(w) = windows.iter().find(|w| w.id().unwrap_or(0) as usize == hwnd) {
            let image = w.capture_image().ok()?;
            let mut buf = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut buf);
            image.write_to(&mut cursor, image::ImageFormat::Png).ok()?;
            use base64::Engine;
            return Some(format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&buf)));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// start / stop recording
// ---------------------------------------------------------------------------

pub fn start_recording(
    state: &Arc<RecorderState>,
    config: &crate::config::Config,
    app_handle: &tauri::AppHandle,
) -> Result<String, String> {
    if state.is_recording.load(Ordering::SeqCst) {
        return Err("Already recording".to_string());
    }

    // Stop previews before starting recording
    stop_audio_previews(state);

    let ffmpeg_path = find_ffmpeg();
    let ff_exists = if ffmpeg_path == "ffmpeg" {
        ffmpeg_on_path()
    } else {
        std::path::Path::new(&ffmpeg_path).exists()
    };
    if !ff_exists {
        return Err("FFmpeg not found. Ensure ffmpeg.exe is bundled or on PATH.".to_string());
    }

    let monitors = enum_monitors();

    std::fs::create_dir_all(&config.output_dir)
        .unwrap_or_else(|e| tracing::warn!("Failed to create output dir: {}", e));

    let output_path = generate_output_path(&config.output_dir, &config.recording_source);
    let video_path = output_path.replace(".mp4", "_video.mp4"); // write temp video file
    
    let args = build_ffmpeg_args(
        &video_path,
        &config.recording_source,
        config.framerate,
        &config.quality,
        &monitors,
    );
    tracing::info!("FFmpeg args: {:?}", args);

    let mut cmd = Command::new(&ffmpeg_path);
    cmd.args(&args).stdin(Stdio::piped()).stdout(Stdio::null());

    #[cfg(target_os = "windows")]
    {
        let log_path = std::env::temp_dir().join("dr-record-ffmpeg.log");
        if let Ok(f) = std::fs::File::create(&log_path) {
            cmd.stderr(std::process::Stdio::from(f));
        } else {
            cmd.stderr(Stdio::null());
        }
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(target_os = "windows"))]
    cmd.stderr(Stdio::null());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed to start ffmpeg: {}", e))?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to capture ffmpeg stdin".to_string())?;

    // Audio Setup
    if config.record_system_audio {
        let mut sys_recorder = AudioRecorder::new();
        let sys_path = output_path.replace(".mp4", "_sys.wav");
        if let Err(e) = sys_recorder.start(app_handle.clone(), true, None, Some(sys_path.clone()), "system".to_string()) {
            tracing::warn!("Failed to start system audio: {}", e);
        } else {
            *lock(&state.sys_audio) = Some(sys_recorder);
            *lock(&state.sys_audio_path) = Some(sys_path);
        }
    }
    
    if config.microphone_name != "None" {
        let mut mic_recorder = AudioRecorder::new();
        let mic_path = output_path.replace(".mp4", "_mic.wav");
        if let Err(e) = mic_recorder.start(app_handle.clone(), false, Some(config.microphone_name.clone()), Some(mic_path.clone()), "mic".to_string()) {
            tracing::warn!("Failed to start mic audio: {}", e);
        } else {
            *lock(&state.mic_audio) = Some(mic_recorder);
            *lock(&state.mic_audio_path) = Some(mic_path);
        }
    }

    state.is_recording.store(true, Ordering::SeqCst);
    *lock(&state.process) = Some(child);
    *lock(&state.stdin) = Some(stdin);
    *lock(&state.start_time) = Some(Instant::now());
    *lock(&state.output_path) = Some(output_path.clone());

    // FFmpeg Watchdog thread
    let state_clone_watchdog = Arc::clone(state);
    let app_clone_watchdog = app_handle.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if !state_clone_watchdog.is_recording.load(Ordering::SeqCst) {
                break; // Stopped normally
            }

            let mut exited = false;
            if let Some(ref mut child) = *lock(&state_clone_watchdog.process) {
                if let Ok(Some(_status)) = child.try_wait() {
                    exited = true;
                }
            }

            if exited {
                tracing::error!("FFmpeg process exited unexpectedly!");
                
                let mut error_msg = "FFmpeg crashed unexpectedly. Please check your settings.".to_string();
                #[cfg(target_os = "windows")]
                {
                    let log_path = std::env::temp_dir().join("dr-record-ffmpeg.log");
                    if let Ok(content) = std::fs::read_to_string(&log_path) {
                        let lines: Vec<&str> = content.lines().collect();
                        let tail = lines.iter().rev().take(15).rev().copied().collect::<Vec<&str>>().join("\n");
                        if !tail.trim().is_empty() {
                            error_msg = format!("FFmpeg Crash Log:\n{}", tail);
                        }
                    }
                }

                state_clone_watchdog.is_recording.store(false, Ordering::SeqCst);
                *lock(&state_clone_watchdog.start_time) = None;
                *lock(&state_clone_watchdog.process) = None;

                use tauri::Emitter;
                let _ = app_clone_watchdog.emit("recording-crashed", error_msg);
                let _ = app_clone_watchdog.emit("status-changed", "stopped");
                break;
            }
        }
    });

    tracing::info!("Recording started: {}", output_path);
    Ok(output_path)
}

pub fn stop_recording(state: &RecorderState) -> Result<Option<String>, String> {
    if !state.is_recording.load(Ordering::SeqCst) {
        return Err("Not recording".to_string());
    }

    state.is_recording.store(false, Ordering::SeqCst);
    tracing::info!("Stopping recording...");

    // Tell FFmpeg to finish cleanly
    if let Some(mut stdin) = lock(&state.stdin).take() {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
        std::thread::sleep(std::time::Duration::from_millis(400));
        drop(stdin);
    }

    // Wait for FFmpeg with a timeout; kill if needed
    if let Some(mut child) = lock(&state.process).take() {
        match child.try_wait() {
            Ok(Some(_)) => {
                tracing::info!("FFmpeg already exited");
            }
            _ => {
                let deadline = Instant::now() + std::time::Duration::from_secs(3);
                loop {
                    if let Ok(Some(_)) = child.try_wait() {
                        break;
                    }
                    if Instant::now() >= deadline {
                        tracing::warn!("FFmpeg did not exit in time, killing");
                        let _ = child.kill();
                        let _ = child.wait();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                tracing::info!("FFmpeg exited");
            }
        }
    }

    *lock(&state.start_time) = None;
    *lock(&state.start_time) = None;
    *lock(&state.watch_hwnd) = 0;

    if let Some(mut sys) = lock(&state.sys_audio).take() {
        sys.stop();
    }
    if let Some(mut mic) = lock(&state.mic_audio).take() {
        mic.stop();
    }

    let sys_path = lock(&state.sys_audio_path).take();
    let mic_path = lock(&state.mic_audio_path).take();
    let path = lock(&state.output_path).take();

    if let Some(final_path) = &path {
        let video_path = final_path.replace(".mp4", "_video.mp4");
        let ffmpeg_path = find_ffmpeg();
        let mut mux_args = vec!["-y".to_string(), "-i".to_string(), video_path.clone()];

        let mut audio_inputs = 0;
        if let Some(sp) = &sys_path {
            if std::path::Path::new(sp).exists() {
                mux_args.extend_from_slice(&["-i".to_string(), sp.clone()]);
                audio_inputs += 1;
            }
        }
        if let Some(mp) = &mic_path {
            if std::path::Path::new(mp).exists() {
                mux_args.extend_from_slice(&["-i".to_string(), mp.clone()]);
                audio_inputs += 1;
            }
        }

        if audio_inputs > 0 {
            if audio_inputs == 1 {
                // One audio source, just map it
                mux_args.extend_from_slice(&[
                    "-c:v".to_string(), "copy".to_string(),
                    "-c:a".to_string(), "aac".to_string(),
                    "-map".to_string(), "0:v:0".to_string(),
                    "-map".to_string(), "1:a:0".to_string(),
                    final_path.clone()
                ]);
            } else {
                // Mix two audio sources
                mux_args.extend_from_slice(&[
                    "-filter_complex".to_string(), "[1:a][2:a]amix=inputs=2[a]".to_string(),
                    "-map".to_string(), "0:v".to_string(),
                    "-map".to_string(), "[a]".to_string(),
                    "-c:v".to_string(), "copy".to_string(),
                    "-c:a".to_string(), "aac".to_string(),
                    final_path.clone()
                ]);
            }
        } else {
            // No audio, just copy video to final path
            mux_args.extend_from_slice(&[
                "-c".to_string(), "copy".to_string(),
                final_path.clone()
            ]);
        }

        tracing::info!("Muxing with args: {:?}", mux_args);
        
        let mut cmd = Command::new(&ffmpeg_path);
        cmd.args(&mux_args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);
        
        if let Ok(mut mux_proc) = cmd.spawn() {
            let _ = mux_proc.wait();
        }

        // Clean up temp files
        let _ = std::fs::remove_file(&video_path);
        if let Some(sp) = sys_path { let _ = std::fs::remove_file(sp); }
        if let Some(mp) = mic_path { let _ = std::fs::remove_file(mp); }
    }

    tracing::info!("Recording stopped: {:?}", path);

    Ok(path)
}

pub fn get_elapsed(state: &RecorderState) -> u64 {
    if let Some(start) = *lock(&state.start_time) {
        start.elapsed().as_secs()
    } else {
        0
    }
}

pub fn start_audio_previews(
    state: &Arc<RecorderState>,
    config: &crate::config::Config,
    app_handle: &tauri::AppHandle,
) {
    stop_audio_previews(state);

    if config.record_system_audio {
        let mut sys_recorder = AudioRecorder::new();
        if let Err(e) = sys_recorder.start(app_handle.clone(), true, None, None, "system".to_string()) {
            tracing::warn!("Failed to start system audio preview: {}", e);
        } else {
            *lock(&state.preview_sys_audio) = Some(sys_recorder);
        }
    }

    if config.microphone_name != "None" {
        let mut mic_recorder = AudioRecorder::new();
        if let Err(e) = mic_recorder.start(app_handle.clone(), false, Some(config.microphone_name.clone()), None, "mic".to_string()) {
            tracing::warn!("Failed to start mic audio preview: {}", e);
        } else {
            *lock(&state.preview_mic_audio) = Some(mic_recorder);
        }
    }
}

pub fn stop_audio_previews(state: &RecorderState) {
    if let Some(mut sys) = lock(&state.preview_sys_audio).take() {
        sys.stop();
    }
    if let Some(mut mic) = lock(&state.preview_mic_audio).take() {
        mic.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_monitors() -> Vec<MonitorInfo> {
        vec![
            MonitorInfo {
                index: 0,
                device_id: "\\\\.\\DISPLAY1".to_string(),
                label: "Monitor 1 (Primary) [\\\\.\\DISPLAY1]".to_string(),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                is_primary: true,
            },
            MonitorInfo {
                index: 1,
                device_id: "\\\\.\\DISPLAY2".to_string(),
                label: "Monitor 2 [\\\\.\\DISPLAY2]".to_string(),
                x: -1920,
                y: 0,
                width: 1920,
                height: 1080,
                is_primary: false,
            },
            MonitorInfo {
                index: 2,
                device_id: "\\\\.\\DISPLAY3".to_string(),
                label: "Monitor 3 [\\\\.\\DISPLAY3]".to_string(),
                x: 0,
                y: -1080,
                width: 2560,
                height: 1440,
                is_primary: false,
            },
        ]
    }

    #[test]
    fn resolves_monitor_by_stable_device_id() {
        let ms = fake_monitors();
        let m = resolve_monitor("monitor:\\\\.\\DISPLAY2", &ms).expect("found");
        assert_eq!(m.device_id, "\\\\.\\DISPLAY2");
        assert_eq!(m.x, -1920);
        assert_eq!(m.width, 1920);
    }

    #[test]
    fn resolves_legacy_positional_index_to_same_monitor() {
        let ms = fake_monitors();
        // index 0 -> primary, index 1 -> secondary (left of primary, negative x)
        let m0 = resolve_monitor("monitor:0", &ms).expect("found");
        assert_eq!(m0.device_id, "\\\\.\\DISPLAY1");
        assert!(m0.is_primary);
        let m1 = resolve_monitor("monitor:1", &ms).expect("found");
        assert_eq!(m1.device_id, "\\\\.\\DISPLAY2");
        assert_eq!(m1.x, -1920);
    }

    #[test]
    fn index_and_device_id_disagree_is_resolved_by_device_id() {
        let ms = fake_monitors();
        // Same physical monitor whichever style is stored.
        assert_eq!(
            resolve_monitor("monitor:2", &ms).unwrap().device_id,
            "\\\\.\\DISPLAY3"
        );
        assert_eq!(
            resolve_monitor("monitor:\\\\.\\DISPLAY3", &ms).unwrap().device_id,
            "\\\\.\\DISPLAY3"
        );
    }

    #[test]
    fn unknown_or_non_monitor_sources_resolve_to_none() {
        let ms = fake_monitors();
        assert!(resolve_monitor("monitor:\\\\.\\DISPLAY99", &ms).is_none());
        assert!(resolve_monitor("monitor:42", &ms).is_none());
        assert!(resolve_monitor("all", &ms).is_none());
        assert!(resolve_monitor("window:123", &ms).is_none());
        assert!(resolve_monitor("", &ms).is_none());
    }

    #[test]
    fn build_ffmpeg_args_uses_device_id_and_negative_offsets() {
        let ms = fake_monitors();
        let args = build_ffmpeg_args("out.mp4", "monitor:\\\\.\\DISPLAY2", 30, "high", &ms);
        assert!(args.contains(&"-offset_x".to_string()));
        let ox = args[args.iter().position(|a| a == "-offset_x").unwrap() + 1].clone();
        let oy = args[args.iter().position(|a| a == "-offset_y").unwrap() + 1].clone();
        let vs = args[args.iter().position(|a| a == "-video_size").unwrap() + 1].clone();
        assert_eq!(ox, "-1920");
        assert_eq!(oy, "0");
        assert_eq!(vs, "1920x1080");
    }

    #[test]
    fn build_ffmpeg_args_legacy_index_matches_device_id() {
        let ms = fake_monitors();
        let by_index = build_ffmpeg_args("out.mp4", "monitor:1", 30, "high", &ms);
        let by_device = build_ffmpeg_args("out.mp4", "monitor:\\\\.\\DISPLAY2", 30, "high", &ms);
        assert_eq!(by_index, by_device);
    }

    /// On real hardware, every device id exposed by enum_monitors() must map to
    /// exactly one xcap monitor via the same name lookup get_thumbnail() uses.
    /// This guarantees the preview always targets the monitor that capture
    /// would record, regardless of how either enumeration orders monitors.
    #[cfg(target_os = "windows")]
    #[test]
    fn every_enumerated_monitor_maps_to_one_xcap_monitor_by_device_id() {
        let known = enum_monitors();
        if known.is_empty() {
            return;
        }
        let xcap_monitors = match Monitor::all() {
            Ok(ms) => ms,
            Err(_) => return, // capture API unavailable in this session
        };
        let device_ids: std::collections::HashSet<&str> =
            known.iter().map(|m| m.device_id.as_str()).collect();
        assert_eq!(
            device_ids.len(),
            known.len(),
            "device ids must be unique"
        );
        for info in &known {
            let matches: Vec<_> = xcap_monitors
                .iter()
                .filter(|m| m.name().ok().as_deref() == Some(info.device_id.as_str()))
                .collect();
            assert_eq!(
                matches.len(),
                1,
                "device id {} must map to exactly one xcap monitor",
                info.device_id
            );
        }
    }
}
