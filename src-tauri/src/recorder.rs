#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use chrono::Local;
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

use crate::audio::{wav_data_bytes, AudioRecorder};
use crate::av_sync::{
    missing_track_warnings, mux_succeeded, plan_mux, validate_final_file,
    FinalFileVerdict, RecordingClock,
};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};
use tauri::Emitter;
use xcap::{Monitor, Window};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Open a capture stream, retrying a few times. WASAPI releases a device
/// asynchronously, so immediately reopening a device that was just stopped
/// (e.g. a mic preview stream) can fail transiently; a short backoff resolves
/// it without silently dropping the track.
fn start_audio_recorder_with_retry(
    app: &tauri::AppHandle,
    is_system: bool,
    device_name: Option<String>,
    output_path: Option<String>,
    source_name: String,
    common_start: Option<Instant>,
) -> Result<AudioRecorder, String> {
    let attempts = 4;
    let mut last_err = String::new();
    for i in 0..attempts {
        let mut recorder = AudioRecorder::new();
        match recorder.start_with_clock(
            app.clone(),
            is_system,
            device_name.clone(),
            output_path.clone(),
            source_name.clone(),
            common_start,
        ) {
            Ok(()) => return Ok(recorder),
            Err(e) => {
                tracing::warn!(
                    "audio-start retry {}/{} for {} failed: {}",
                    i + 1,
                    attempts,
                    source_name,
                    e
                );
                last_err = e;
                if i + 1 < attempts {
                    std::thread::sleep(std::time::Duration::from_millis(200 * (i + 1)));
                }
            }
        }
    }
    Err(last_err)
}

/// Preview retry path: preserves the original 5-arg `AudioRecorder::start`
/// (no shared clock; offsets stay `None` / `"none"`).
fn start_audio_preview_with_retry(
    app: &tauri::AppHandle,
    is_system: bool,
    device_name: Option<String>,
    source_name: String,
) -> Result<AudioRecorder, String> {
    let attempts = 4;
    let mut last_err = String::new();
    for i in 0..attempts {
        let mut recorder = AudioRecorder::new();
        match recorder.start(
            app.clone(),
            is_system,
            device_name.clone(),
            None,
            source_name.clone(),
        ) {
            Ok(()) => return Ok(recorder),
            Err(e) => {
                last_err = e;
                if i + 1 < attempts {
                    std::thread::sleep(std::time::Duration::from_millis(200 * (i + 1)));
                }
            }
        }
    }
    Err(last_err)
}

// ---------------------------------------------------------------------------
// Feature 1/3 runtime types (additive; pure logic lives in av_sync/save_dialog)
// ---------------------------------------------------------------------------

/// Per-track first-sample offsets (ms, signed: positive = audio late).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AvOffsets {
    pub system: Option<i64>,
    pub mic: Option<i64>,
    pub system_source: String,
    pub mic_source: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackWarningPayload {
    pub track: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StopResult {
    pub take_id: String,
    pub default_path: String,
    pub final_path: Option<String>,
    pub valid: bool,
    pub validation_reason: String,
    pub offsets_ms: AvOffsets,
    pub warnings: Vec<TrackWarningPayload>,
    pub retained: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PendingTake {
    pub take_id: String,
    pub default_path: String,
    pub final_path: Option<String>,
    pub video_path: String,
    pub sys_path: Option<String>,
    pub mic_path: Option<String>,
    pub valid: bool,
    /// Human-readable finalization outcome (mux exit / validation detail).
    /// Shown in the save dialog so a failed merge is diagnosable instead of
    /// "for some reason". `#[serde(default)]` keeps pre-2.1 journals readable.
    #[serde(default)]
    pub reason: String,
    /// Whether any real device buffer arrived (vs padded zeros only).
    /// `None` = journal predates the flag → fall back to size>0.
    #[serde(default)]
    pub sys_had_input: Option<bool>,
    #[serde(default)]
    pub mic_had_input: Option<bool>,
}

fn take_id_for(default_path: &str) -> String {
    let ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let stem = std::path::Path::new(default_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "take".to_string());
    format!("{stem}_{ms}")
}

fn pending_journal_path(output_dir: &str, take_id: &str) -> std::path::PathBuf {
    std::path::Path::new(output_dir)
        .join(".dr-record-pending")
        .join(format!("{take_id}.json"))
}

fn write_pending_journal(output_dir: &str, pending: &PendingTake) {
    let path = pending_journal_path(output_dir, &pending.take_id);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(pending) {
        let _ = std::fs::write(&path, json);
    }
}

fn clear_pending_journal(output_dir: &str, take_id: &str) {
    let _ = std::fs::remove_file(pending_journal_path(output_dir, take_id));
}

/// Pending takes left behind by a quit/crash with an unresolved dialog.
pub fn list_pending_takes(output_dir: &str) -> Vec<PendingTake> {
    let dir = std::path::Path::new(output_dir).join(".dr-record-pending");
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(pending) = serde_json::from_str::<PendingTake>(&content) {
                    out.push(pending);
                }
            }
        }
    }
    out
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
    /// Why the system/mic capture never started (Some) when a take's WAV is
    /// missing. Surfaced in the take warnings so a silent track names its
    /// cause (busy device, missing mic, exclusive mode) instead of silence.
    pub sys_start_error: Mutex<Option<String>>,
    pub mic_start_error: Mutex<Option<String>>,
    pub start_time: Mutex<Option<Instant>>,
    pub preview_sys_audio: Mutex<Option<AudioRecorder>>,
    pub preview_mic_audio: Mutex<Option<AudioRecorder>>,
    /// Common-start wall clock for logs (Instant lives in `start_time`).
    pub common_start_wall: Mutex<Option<SystemTime>>,
    /// Same origin in the ms domain (`av_sync::RecordingClock`) so overlay
    /// elapsed and mux metadata share one observable start moment.
    pub common_clock: Mutex<Option<RecordingClock>>,
    /// Mux/save in progress: double-stop is a no-op, start-during-mux is rejected.
    pub finalizing: AtomicBool,
    /// Save dialog open for a take: new recordings wait for resolution.
    pub pending_dialog: AtomicBool,
    pub last_offsets: Mutex<AvOffsets>,
    pub pending_take: Mutex<Option<PendingTake>>,
    /// Annotation arm state (Feature 2). Toggling never touches recording.
    pub annotation_armed: AtomicBool,
    pub annotation_tool: Mutex<String>,
    /// Second global shortcut string (Feature 2 annotation toggle).
    pub annotation_hotkey_str: Mutex<String>,
    /// Last annotation toggle time: `toggle_annotation` is idempotent
    /// within a 300ms window so hotkey/button double-fire toggles once.
    /// Mirrors the tested `annotation::AnnotationState::debounced_toggle`
    /// semantics with wall-clock time.
    pub last_annotation_toggle: Mutex<Option<Instant>>,
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
            sys_start_error: Mutex::new(None),
            mic_start_error: Mutex::new(None),
            start_time: Mutex::new(None),
            preview_sys_audio: Mutex::new(None),
            preview_mic_audio: Mutex::new(None),
            common_start_wall: Mutex::new(None),
            common_clock: Mutex::new(None),
            finalizing: AtomicBool::new(false),
            pending_dialog: AtomicBool::new(false),
            last_offsets: Mutex::new(AvOffsets::default()),
            pending_take: Mutex::new(None),
            annotation_armed: AtomicBool::new(false),
            annotation_tool: Mutex::new("pen".to_string()),
            annotation_hotkey_str: Mutex::new("Ctrl+Shift+Alt+A".to_string()),
            last_annotation_toggle: Mutex::new(None),
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
        "-f".to_string(),
        "gdigrab".to_string(),
        "-framerate".to_string(),
        fps.to_string(),
    ];

    if source.starts_with("monitor:") {
        if let Some(mon) = resolve_monitor(source, monitors) {
            let mut w = mon.width;
            let mut h = mon.height;
            if w % 2 != 0 {
                w -= 1;
            }
            if h % 2 != 0 {
                h -= 1;
            }

            args.extend_from_slice(&[
                "-offset_x".to_string(),
                mon.x.to_string(),
                "-offset_y".to_string(),
                mon.y.to_string(),
                "-video_size".to_string(),
                format!("{}x{}", w, h),
                "-i".to_string(),
                "desktop".to_string(),
            ]);
        } else {
            args.extend_from_slice(&["-i".to_string(), "desktop".to_string()]);
        }
    } else {
        // "all"
        args.extend_from_slice(&["-i".to_string(), "desktop".to_string()]);
    }

    args.extend_from_slice(&[
        "-c:v".to_string(),
        "libx264".to_string(),
        "-preset".to_string(),
        "ultrafast".to_string(),
        "-crf".to_string(),
        crf.to_string(),
        "-pix_fmt".to_string(),
        "yuv420p".to_string(),
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

/// Bundled ffprobe (ships next to ffmpeg in resources). Used for the
/// save-dialog file info (resolution/duration); best-effort only.
/// Checks existence like find_ffmpeg and falls back to PATH, so old
/// installs without the bundled binary degrade instead of breaking.
fn find_ffprobe() -> String {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();
    for path in [
        exe_dir.join("ffprobe.exe"),
        exe_dir.join("resources").join("ffprobe.exe"),
        exe_dir.join("..").join("resources").join("ffprobe.exe"),
    ] {
        if path.exists() {
            return path.to_string_lossy().into_owned();
        }
    }
    "ffprobe".to_string()
}

/// Video dims + fps + duration for the save popup, via a header-only
/// ffprobe read (bounded 10 s, never blocks saving). All `None` when the
/// probe binary or file is missing — the dialog falls back to config values.
pub fn probe_video_info(path: &str) -> (Option<u32>, Option<u32>, Option<String>, Option<f64>) {
    let none = (None, None, None, None);
    let mut cmd = Command::new(find_ffprobe());
    cmd.args([
        "-v",
        "error",
        "-select_streams",
        "v:0",
        "-show_entries",
        "stream=width,height,avg_frame_rate",
        "-show_entries",
        "format=duration",
        "-of",
        "default=noprint_wrappers=1",
        path,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(_) => return none,
    };
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    let out: Option<std::process::Output>;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                out = child.wait_with_output().ok();
                break;
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return none;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => return none,
        }
    }
    let text = out
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let mut w = None;
    let mut h = None;
    let mut fps = None;
    let mut dur = None;
    for line in text.lines() {
        let (k, v) = match line.split_once('=') {
            Some(p) => p,
            None => continue,
        };
        match k.trim() {
            "width" => w = v.trim().parse::<u32>().ok(),
            "height" => h = v.trim().parse::<u32>().ok(),
            "avg_frame_rate" => {
                // "30/1" -> "30 fps", "30000/1001" -> "29.97 fps".
                let mut parts = v.trim().split('/');
                if let (Some(n), Some(d)) = (
                    parts.next().and_then(|x| x.parse::<f64>().ok()),
                    parts.next().and_then(|x| x.parse::<f64>().ok()),
                ) {
                    if d != 0.0 {
                        let f = n / d;
                        fps = Some(if (f - f.round()).abs() < 0.01 {
                            format!("{} fps", f.round() as u64)
                        } else {
                            format!("{:.2} fps", f)
                        });
                    }
                }
            }
            "duration" => dur = v.trim().parse::<f64>().ok(),
            _ => {}
        }
    }
    (w, h, fps, dur)
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

/// Virtual-desktop origin of a capture source (may be negative for
/// left/above-primary monitors). Source of truth for canvas cursor→capture
/// offset math (`annotation::map_to_capture`); the fullscreen annotation
/// window itself stays union-spanning (see `overlay.rs`), so this is used
/// for logging/geometry today, not for window repositioning.
pub fn monitor_origin_for(source: &str, monitors: &[MonitorInfo]) -> (i32, i32) {
    resolve_monitor(source, monitors)
        .map(|m| (m.x, m.y))
        .unwrap_or((0, 0))
}

// ---------------------------------------------------------------------------
// Windows API source enumeration
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub fn enum_monitors() -> Vec<MonitorInfo> {
    use std::ffi::c_void;
    use std::mem;
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, MONITORINFOEXW};

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

    let mut col = Collector {
        monitors: Vec::new(),
    };
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(callback),
            &mut col as *mut _ as isize,
        );
    }
    // Primary first, then stable index
    col.monitors
        .sort_by_key(|b| std::cmp::Reverse(b.is_primary));
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
        if let Some(w) = windows
            .iter()
            .find(|w| w.id().unwrap_or(0) as usize == hwnd)
        {
            let image = w.capture_image().ok()?;
            let mut buf = Vec::new();
            let mut cursor = std::io::Cursor::new(&mut buf);
            image.write_to(&mut cursor, image::ImageFormat::Png).ok()?;
            use base64::Engine;
            return Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&buf)
            ));
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
    if state.finalizing.load(Ordering::SeqCst) {
        return Err("Still finalizing the previous take; try again in a moment.".to_string());
    }
    if state.pending_dialog.load(Ordering::SeqCst) {
        return Err("Resolve the save dialog first.".to_string());
    }

    // Stop previews before starting recording
    stop_audio_previews(state);

    // Common-start clock: one observable origin shared by the video track
    // and every enabled audio track. Set BEFORE spawning FFmpeg or audio
    // so overlay elapsed + mux offsets derive from the same origin.
    let common_start = Instant::now();
    let common_wall = SystemTime::now();
    *lock(&state.start_time) = Some(common_start);
    *lock(&state.common_start_wall) = Some(common_wall);
    let origin_ms = common_wall
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    *lock(&state.common_clock) = Some(RecordingClock::new(origin_ms));
    *lock(&state.last_offsets) = AvOffsets::default();
    state.finalizing.store(false, Ordering::SeqCst);
    // Annotation always starts disarmed; prior toggles never leak state.
    state.annotation_armed.store(false, Ordering::SeqCst);
    tracing::info!("recording common-start: {:?}", common_wall);

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
    // Log the virtual-desktop origin for this source (negative on
    // left/above-primary monitors): the annotation canvas offset math
    // (`annotation::map_to_capture`) derives from this origin.
    tracing::info!(
        "capture source {:?} origin: {:?}",
        config.recording_source,
        monitor_origin_for(&config.recording_source, &monitors)
    );

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

    // Audio Setup (shares the common-start clock for first-sample offsets)
    *lock(&state.sys_start_error) = None;
    *lock(&state.mic_start_error) = None;
    if config.record_system_audio {
        let sys_path = output_path.replace(".mp4", "_sys.wav");
        *lock(&state.sys_audio_path) = Some(sys_path.clone());
        match start_audio_recorder_with_retry(
            app_handle,
            true,
            None,
            Some(sys_path.clone()),
            "system".to_string(),
            Some(common_start),
        ) {
            Ok(rec) => *lock(&state.sys_audio) = Some(rec),
            Err(e) => {
                tracing::error!("Failed to start system audio: {}", e);
                *lock(&state.sys_start_error) = Some(e.clone());
                let _ = app_handle.emit("audio-error", format!("System audio failed: {}", e));
            }
        }
    }

    if config.record_mic && config.microphone_name != "None" {
        let mic_path = output_path.replace(".mp4", "_mic.wav");
        *lock(&state.mic_audio_path) = Some(mic_path.clone());
        match start_audio_recorder_with_retry(
            app_handle,
            false,
            Some(config.microphone_name.clone()),
            Some(mic_path.clone()),
            "mic".to_string(),
            Some(common_start),
        ) {
            Ok(rec) => *lock(&state.mic_audio) = Some(rec),
            Err(e) => {
                tracing::error!("Failed to start mic audio: {}", e);
                *lock(&state.mic_start_error) = Some(e.clone());
                let _ = app_handle.emit("audio-error", format!("Microphone failed: {}", e));
            }
        }
    }

    state.is_recording.store(true, Ordering::SeqCst);
    *lock(&state.process) = Some(child);
    *lock(&state.stdin) = Some(stdin);
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

                let mut error_msg =
                    "FFmpeg crashed unexpectedly. Please check your settings.".to_string();
                #[cfg(target_os = "windows")]
                {
                    let log_path = std::env::temp_dir().join("dr-record-ffmpeg.log");
                    if let Ok(content) = std::fs::read_to_string(&log_path) {
                        let lines: Vec<&str> = content.lines().collect();
                        let tail = lines
                            .iter()
                            .rev()
                            .take(15)
                            .rev()
                            .copied()
                            .collect::<Vec<&str>>()
                            .join("\n");
                        if !tail.trim().is_empty() {
                            error_msg = format!("FFmpeg Crash Log:\n{}", tail);
                        }
                    }
                }

                state_clone_watchdog
                    .is_recording
                    .store(false, Ordering::SeqCst);
                *lock(&state_clone_watchdog.start_time) = None;
                *lock(&state_clone_watchdog.process) = None;
                let _ = app_clone_watchdog.emit("recording-crashed", error_msg);
                let _ = app_clone_watchdog.emit("status-changed", "stopped");
                break;
            }
        }
    });

    tracing::info!("Recording started: {}", output_path);
    Ok(output_path)
}

/// Mux argument builder extracted from `stop_recording`.
///
/// Mix policy: mic-only gets `volume=2.0`, two-track uses `amix
/// normalize=0` with mic boost.
///
/// Sync model (read carefully): every WAV is wall-clock aligned by
/// construction — the writer zero-pads device gaps from recording start —
/// so the mux applies NO automatic time shift. Shifting an aligned track
/// was the desync: pad + shift applied the delay twice and pushed whole
/// tracks seconds late. Measured offsets remain as telemetry
/// (`StopResult.offsets_ms`) plus a late-device notice, never as mux args.
///
/// The ONE exception is manual delay compensation (`sys_delay_ms` /
/// `mic_delay_ms` settings, OBS "Sync Offset" equivalent): some setups
/// (virtual audio drivers, Bluetooth) deliver every buffer with a large
/// CONSTANT driver latency no app can auto-measure. For those, `-ss N`
/// skips the first N ms of that track's input. Default 0 = untouched.
///
/// Every audio leg runs `aresample=48000,aformat=channel_layouts=stereo`
/// before mixing/encoding: loopback/microphone devices can deliver MONO
/// (or other layouts) and the AAC encoder hard-fails on those
/// (`Unsupported channel layout`, exit -22) despite perfect temps —
/// normalization upmixes mono to stereo and leaves stereo untouched.
/// Deliberately NO `aresample=async` time warp: it stretches/squeezes
/// audio to the video clock, audibly eating silences and making sync
/// randomly better or worse per take.
///
/// `-fflags +genpts` on the video input regenerates presentation
/// timestamps for the `-c:v copy` mux: gdigrab capture-clock timestamps
/// can be non-monotonic under load (dropped/duplicated frames), and a
/// copy-mux aborts on those ("Non-monotonous DTS") leaving no final file
/// despite perfect temps. Regenerating PTS keeps the pixels identical
/// while making the mux robust.
pub fn build_mux_args(
    final_path: &str,
    video_path: &str,
    sys_path: Option<&str>,
    mic_path: Option<&str>,
    sys_delay_ms: u32,
    mic_delay_ms: u32,
) -> Result<Vec<String>, String> {
    let video_bytes = std::fs::metadata(video_path).map(|m| m.len()).unwrap_or(0);
    let sys_data = sys_path.map(wav_data_bytes).unwrap_or(0);
    let mic_data = mic_path.map(wav_data_bytes).unwrap_or(0);
    let plan = plan_mux(
        video_bytes,
        sys_path.is_some(),
        sys_data,
        mic_path.is_some(),
        mic_data,
    )
    .map_err(|e| format!("No recordable inputs: {:?}", e))?;

    let mut args = vec![
        "-y".to_string(),
        "-fflags".to_string(),
        "+genpts".to_string(),
        "-i".to_string(),
        video_path.to_string(),
    ];
    if plan.video_only {
        args.extend_from_slice(&["-c".to_string(), "copy".to_string(), final_path.to_string()]);
        return Ok(args);
    }
    if plan.include_sys && plan.include_mic {
        if sys_delay_ms > 0 {
            args.extend_from_slice(&[
                "-ss".to_string(),
                format!("{:.3}", sys_delay_ms as f64 / 1000.0),
            ]);
        }
        args.extend_from_slice(&["-i".to_string(), sys_path.unwrap_or_default().to_string()]);
        if mic_delay_ms > 0 {
            args.extend_from_slice(&[
                "-ss".to_string(),
                format!("{:.3}", mic_delay_ms as f64 / 1000.0),
            ]);
        }
        args.extend_from_slice(&["-i".to_string(), mic_path.unwrap_or_default().to_string()]);
        // Inputs: 0=video, 1=system, 2=microphone. Plain resample +
        // layout normalize per leg (mono-safe for AAC); amix without
        // normalization + mic boost preserves speech audibility.
        args.extend_from_slice(&[
            "-filter_complex".to_string(),
            "[1:a]aresample=48000,aformat=channel_layouts=stereo,volume=1.0[a1];[2:a]aresample=48000,aformat=channel_layouts=stereo,volume=2.0[a2];[a1][a2]amix=inputs=2:normalize=0[a]".to_string(),
            "-map".to_string(),
            "0:v".to_string(),
            "-map".to_string(),
            "[a]".to_string(),
            "-c:v".to_string(),
            "copy".to_string(),
            "-c:a".to_string(),
            "aac".to_string(),
            "-ar".to_string(),
            "48000".to_string(),
            final_path.to_string(),
        ]);
        return Ok(args);
    }
    // Single audio leg.
    let (audio_path, mic_only) = if plan.include_mic {
        (mic_path.unwrap_or_default(), true)
    } else {
        (sys_path.unwrap_or_default(), false)
    };
    let delay_ms = if mic_only { mic_delay_ms } else { sys_delay_ms };
    if delay_ms > 0 {
        args.extend_from_slice(&["-ss".to_string(), format!("{:.3}", delay_ms as f64 / 1000.0)]);
    }
    args.extend_from_slice(&["-i".to_string(), audio_path.to_string()]);
    let af = if mic_only {
        "aresample=48000,aformat=channel_layouts=stereo,volume=2.0"
    } else {
        "aresample=48000,aformat=channel_layouts=stereo"
    };
    args.extend_from_slice(&[
        "-c:v".to_string(),
        "copy".to_string(),
        "-af".to_string(),
        af.to_string(),
        "-c:a".to_string(),
        "aac".to_string(),
        "-ar".to_string(),
        "48000".to_string(),
        "-map".to_string(),
        "0:v:0".to_string(),
        "-map".to_string(),
        "1:a:0".to_string(),
        final_path.to_string(),
    ]);
    Ok(args)
}

/// Validate a muxed final file: file-level tiers first (exists + size +
/// `moov` scan via `av_sync::validate_final_file`), then a bundled-FFmpeg
/// null-decode (`ffmpeg -v error -i <final> -f null -`) when available.
pub fn validate_output(final_path: &str) -> (FinalFileVerdict, String) {
    let verdict = validate_final_file(std::path::Path::new(final_path));
    if verdict != FinalFileVerdict::Valid {
        return (verdict, format!("file check failed: {:?}", verdict));
    }
    let ffmpeg_path = find_ffmpeg();
    let mut cmd = Command::new(&ffmpeg_path);
    cmd.arg("-v")
        .arg("error")
        .arg("-i")
        .arg(final_path)
        .arg("-f")
        .arg("null")
        .arg("-")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);
    // Bounded decode check (60s): a corrupt/oversized file must fail into
    // the retry path, never block saving forever.
    match cmd.spawn() {
        Ok(mut p) => {
            let deadline = Instant::now() + std::time::Duration::from_secs(60);
            let mut status = None;
            loop {
                match p.try_wait() {
                    Ok(Some(s)) => {
                        status = Some(s);
                        break;
                    }
                    Ok(None) => {
                        if Instant::now() >= deadline {
                            tracing::warn!("validation decode timed out, killing validator");
                            let _ = p.kill();
                            let _ = p.wait();
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    Err(e) => {
                        tracing::warn!("validation wait failed: {}", e);
                        break;
                    }
                }
            }
            match status {
                Some(s) if s.success() => (FinalFileVerdict::Valid, "ok".to_string()),
                Some(_) => (
                    FinalFileVerdict::MoovMissing,
                    "decode check failed (truncated moov?)".to_string(),
                ),
                None => (
                    FinalFileVerdict::MoovMissing,
                    "decode check timed out; retry without re-recording".to_string(),
                ),
            }
        }
        Err(e) => {
            // Decode tier inconclusive: the validator binary is missing, so
            // a `Valid` verdict cannot be honestly reported (SPEC: no corrupt
            // file ever reported as success). Return a retryable non-Valid
            // verdict and keep temps; retry can succeed without re-recording
            // once FFmpeg is available.
            tracing::warn!(
                "decode-tier validator unavailable ({}); final NOT reported as valid (retryable)",
                e
            );
            (
                FinalFileVerdict::MoovMissing,
                format!(
                    "decode check inconclusive: validator binary missing ({}); retry without re-recording once ffmpeg is available",
                    e
                ),
            )
        }
    }
}

struct CapturedTake {
    take_id: String,
    default_path: String,
    video_path: String,
    sys_path: Option<String>,
    mic_path: Option<String>,
    sys_requested: bool,
    mic_requested: bool,
    sys_data_bytes: u64,
    mic_data_bytes: u64,
    /// Any real device buffer arrived (vs pure padded zeros from a device
    /// that never delivered). Decides inclusion, never the file size.
    sys_had_input: bool,
    mic_had_input: bool,
    offsets: AvOffsets,
}

/// Quit FFmpeg, join audio writers, collect artifacts + offsets.
/// No mux here; reusable by stop and by crash-recovery paths.
/// Stage timings are logged so a slow SAVING names its stage.
fn stop_capture(state: &RecorderState) -> Result<CapturedTake, String> {
    state.is_recording.store(false, Ordering::SeqCst);
    tracing::info!("Stopping recording...");
    let stop_t0 = Instant::now();

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
    tracing::info!("stop stage: ffmpeg-quit took {} ms", stop_t0.elapsed().as_millis());

    *lock(&state.start_time) = None;
    *lock(&state.watch_hwnd) = 0;
    // Disarm annotation only; captured marks persist in the video track.
    state.annotation_armed.store(false, Ordering::SeqCst);

    let mut sys_offset = None;
    let mut sys_source = "none".to_string();
    let mut mic_offset = None;
    let mut mic_source = "none".to_string();
    let mut sys_had_input = false;
    let mut mic_had_input = false;
    let audio_t0 = Instant::now();
    if let Some(mut sys) = lock(&state.sys_audio).take() {
        sys_offset = sys.first_sample_offset_ms();
        sys_source = sys.offset_source();
        sys_had_input = sys.has_received_input();
        sys.stop();
    }
    if let Some(mut mic) = lock(&state.mic_audio).take() {
        mic_offset = mic.first_sample_offset_ms();
        mic_source = mic.offset_source();
        mic_had_input = mic.has_received_input();
        mic.stop();
    }
    tracing::info!("stop stage: audio-stop took {} ms", audio_t0.elapsed().as_millis());

    // Signed-offset convention (positive = audio late) matches
    // `av_sync::offset_ms`; arrival-time offsets here are >= 0 by
    // construction (Instant::now() after the shared start).
    let offsets = AvOffsets {
        system: sys_offset,
        mic: mic_offset,
        system_source: sys_source,
        mic_source,
    };
    *lock(&state.last_offsets) = offsets.clone();

    let sys_path = lock(&state.sys_audio_path).take();
    let mic_path = lock(&state.mic_audio_path).take();
    let path = lock(&state.output_path).take();
    let default_path = path.ok_or_else(|| "No active recording path".to_string())?;
    let video_path = default_path.replace(".mp4", "_video.mp4");

    let sys_requested = sys_path.is_some();
    let mic_requested = mic_path.is_some();
    tracing::info!("stop stage: paths collected, reading wav sizes...");
    let sys_data_bytes = sys_path.as_deref().map(wav_data_bytes).unwrap_or(0);
    let mic_data_bytes = mic_path.as_deref().map(wav_data_bytes).unwrap_or(0);
    tracing::info!(
        "stop stage: wav sizes sys={} mic={}",
        sys_data_bytes,
        mic_data_bytes
    );

    // Clone state out of the locks BEFORE logging: no guard is held across
    // the format call, so a slow/poisoned mutex can never wedge the stop.
    let last = lock(&state.last_offsets).clone();
    let sys_err = lock(&state.sys_start_error).clone();
    let mic_err = lock(&state.mic_start_error).clone();
    tracing::info!(
        "capture done: sys_offset={:?} ({}) mic_offset={:?} ({}) sys_bytes={} mic_bytes={} sys_input={} mic_input={} sys_start_err={:?} mic_start_err={:?}",
        sys_offset,
        last.system_source,
        mic_offset,
        last.mic_source,
        sys_data_bytes,
        mic_data_bytes,
        sys_had_input,
        mic_had_input,
        sys_err,
        mic_err,
    );
    // Cross-check the shared origin: Instant-domain stop vs ms-clock.
    if let Some(clock) = *lock(&state.common_clock) {
        let now_ms = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        tracing::info!(
            "take duration (shared clock): {} ms",
            clock.elapsed_ms(now_ms)
        );
    }

    Ok(CapturedTake {
        take_id: take_id_for(&default_path),
        default_path,
        video_path,
        sys_path,
        mic_path,
        sys_requested,
        mic_requested,
        sys_data_bytes,
        mic_data_bytes,
        sys_had_input,
        mic_had_input,
        offsets,
    })
}

fn output_dir_of(default_path: &str) -> String {
    std::path::Path::new(default_path)
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_string())
}

/// Last `max_lines` of a log file for user-facing failure detail.
/// Mux stderr is the only witness when the merge fails despite healthy
/// temps, so the tail is surfaced (not just logged).
fn read_tail_lines(path: &std::path::Path, max_lines: usize) -> String {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(max_lines);
    lines[start..].join("\n")
}

fn existing_temps(captured: &CapturedTake) -> Vec<String> {
    let mut out = Vec::new();
    for p in std::iter::once(&captured.video_path)
        .chain(captured.sys_path.iter())
        .chain(captured.mic_path.iter())
    {
        if std::path::Path::new(p).exists() {
            out.push(p.clone());
        }
    }
    out
}

fn sweep_temps_for(default_path: &str, sys_path: &Option<String>, mic_path: &Option<String>) {
    let video_path = default_path.replace(".mp4", "_video.mp4");
    let _ = std::fs::remove_file(&video_path);
    if let Some(sp) = sys_path {
        let _ = std::fs::remove_file(sp);
    }
    if let Some(mp) = mic_path {
        let _ = std::fs::remove_file(mp);
    }
}

/// Mux + validate a captured take. Retains temps on failure for `retry`.
///
/// Sync model: every WAV is wall-clock aligned by construction (the writer
/// zero-pads device gaps from recording start), so tracks mux WITHOUT any
/// time shift — shifting an already-aligned track is what used to push
/// whole tracks seconds late. Measured offsets stay as telemetry
/// (`offsets_ms`) plus a notice when a device woke seconds late.
/// Inclusion rule: a requested track with no real device input is excluded
/// with a named warning, even if its file holds padded zeros.
fn finalize_captured(state: &RecorderState, captured: CapturedTake) -> StopResult {
    let sys_enabled = captured.sys_requested;
    let mic_enabled = captured.mic_requested;
    // Dead-device exclusion: padded zeros must not masquerade as content.
    let sys_data_bytes = if captured.sys_had_input {
        captured.sys_data_bytes
    } else {
        0
    };
    let mic_data_bytes = if captured.mic_had_input {
        captured.mic_data_bytes
    } else {
        0
    };
    // Late-device notice (>2 s to first sample): alignment holds via
    // padding, but the user should know the start was silence.
    let mut late_notices: Vec<TrackWarningPayload> = Vec::new();
    for (track, raw) in [
        ("system", captured.offsets.system),
        ("mic", captured.offsets.mic),
    ] {
        if let Some(r) = raw {
            if r.abs() > crate::av_sync::LATE_START_NOTICE_MS {
                late_notices.push(TrackWarningPayload {
                    track: track.to_string(),
                    reason: format!(
                        "{} audio showed up {:.1} s late (device waking up?); silence kept its place so sync holds",
                        track,
                        r as f64 / 1000.0,
                    ),
                });
            }
        }
    }
    // Manual delay compensation comes from saved settings (same source the
    // recording itself used); 0 = untouched.
    let delay_cfg = crate::config::Config::load();
    let mux_args = build_mux_args(
        &captured.default_path,
        &captured.video_path,
        captured.sys_path.as_deref(),
        captured.mic_path.as_deref(),
        delay_cfg.system_delay_ms,
        delay_cfg.mic_delay_ms,
    );

    let mut mux_exit = -1;
    let mux_args = match mux_args {
        Ok(a) => a,
        Err(e) => {
            tracing::error!("mux plan failed: {}", e);
            let warnings = missing_track_warnings(
                sys_enabled,
                sys_data_bytes,
                mic_enabled,
                mic_data_bytes,
            );
            let retained = existing_temps(&captured);
            let plan_reason = format!("mux plan failed: {}", e);
            let pending = PendingTake {
                take_id: captured.take_id.clone(),
                default_path: captured.default_path.clone(),
                final_path: None,
                video_path: captured.video_path.clone(),
                sys_path: captured.sys_path.clone(),
                mic_path: captured.mic_path.clone(),
                valid: false,
                reason: plan_reason.clone(),
                sys_had_input: Some(captured.sys_had_input),
                mic_had_input: Some(captured.mic_had_input),
            };
            *lock(&state.pending_take) = Some(pending.clone());
            write_pending_journal(&output_dir_of(&captured.default_path), &pending);
            state.pending_dialog.store(true, Ordering::SeqCst);
            return StopResult {
                take_id: captured.take_id,
                default_path: captured.default_path,
                final_path: None,
                valid: false,
                validation_reason: plan_reason,
                offsets_ms: captured.offsets,
                warnings: warnings
                    .iter()
                    .map(|w| TrackWarningPayload {
                        track: w.track.to_string(),
                        reason: w.reason.clone(),
                    })
                    .collect(),
                retained,
            };
        }
    };

    tracing::info!("Muxing with args: {:?}", mux_args);    let ffmpeg_path = find_ffmpeg();
    // Mux stderr goes to a per-take log (not null): when the merge fails
    // despite healthy temps, the ffmpeg tail is the only witness, and it
    // is surfaced in the failure reason below instead of "for some reason".
    let mux_log_path =
        std::env::temp_dir().join(format!("dr-record-mux-{}.log", captured.take_id));
    let mux_log_file = std::fs::File::create(&mux_log_path).ok();
    let mut cmd = Command::new(&ffmpeg_path);
    cmd.args(&mux_args)
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    if let Some(f) = mux_log_file {
        cmd.stderr(std::process::Stdio::from(f));
    } else {
        cmd.stderr(Stdio::null());
    }

    #[cfg(target_os = "windows")]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mux_t0 = Instant::now();
    if let Ok(mut mux_proc) = cmd.spawn() {
        // Bounded wait: a stuck mux previously left the UI on SAVING with no
        // exit path. 90s is far beyond a normal copy-mux; on timeout the
        // temps are retained and the user gets the retry path, never a hang.
        let deadline = Instant::now() + std::time::Duration::from_secs(90);
        loop {
            match mux_proc.try_wait() {
                Ok(Some(status)) => {
                    mux_exit = status.code().unwrap_or(-1);
                    break;
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        tracing::error!("mux timed out after 90s, killing ffmpeg");
                        let _ = mux_proc.kill();
                        let _ = mux_proc.wait();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    tracing::error!("mux wait failed: {}", e);
                    break;
                }
            }
        }
    } else {
        tracing::error!("Failed to spawn mux FFmpeg");
    }
    let output_bytes = std::fs::metadata(&captured.default_path)
        .map(|m| m.len())
        .unwrap_or(0);
    tracing::info!(
        "mux exited {} with {} output bytes (took {} ms)",
        mux_exit,
        output_bytes,
        mux_t0.elapsed().as_millis()
    );

    let mux_ok = mux_succeeded(mux_exit, output_bytes);
    let validate_t0 = Instant::now();
    let (verdict, vreason) = validate_output(&captured.default_path);
    tracing::info!(
        "validate took {} ms",
        validate_t0.elapsed().as_millis()
    );
    let valid = mux_ok && verdict == FinalFileVerdict::Valid;
    let reason = if !mux_ok {
        let tail = read_tail_lines(&mux_log_path, 15);
        if tail.trim().is_empty() {
            format!(
                "mux failed (exit {}, {} output bytes) with no ffmpeg detail; temps kept for retry",
                mux_exit, output_bytes
            )
        } else {
            format!("mux failed (exit {}): {}", mux_exit, tail)
        }
    } else {
        vreason
    };
    tracing::info!("validation: {:?} ({})", verdict, reason);

    // Named-track warnings: an enabled track with no data is never silent.
    // (Sizes are input-gated above: a padded-but-contentless file counts as
    // empty here, so dead devices are named instead of muxed as silence.)
    let warnings = missing_track_warnings(
        sys_enabled,
        sys_data_bytes,
        mic_enabled,
        mic_data_bytes,
    );
    for w in &warnings {
        tracing::warn!("audio track missing: {} ({})", w.track, w.reason);
    }
    // Keep video-only honest: surface which track is missing on the result.
    // When a WAV is empty AND its capture never started, the recorded start
    // error names the cause (busy device, missing mic) instead of silence.
    let mut warnings: Vec<TrackWarningPayload> = warnings
        .iter()
        .map(|w| TrackWarningPayload {
            track: w.track.to_string(),
            reason: w.reason.clone(),
        })
        .collect();
    if sys_data_bytes == 0 {
        if let Some(err) = lock(&state.sys_start_error).clone() {
            warnings.push(TrackWarningPayload {
                track: "system".to_string(),
                reason: format!("system capture never started: {}", err),
            });
        }
    }
    if mic_data_bytes == 0 {
        if let Some(err) = lock(&state.mic_start_error).clone() {
            warnings.push(TrackWarningPayload {
                track: "mic".to_string(),
                reason: format!("mic capture never started: {}", err),
            });
        }
    }
    // Late-device notices from above: the user must see them, not just logs.
    warnings.extend(late_notices);

    if valid {
        sweep_temps_for(
            &captured.default_path,
            &captured.sys_path,
            &captured.mic_path,
        );
        let pending = PendingTake {
            take_id: captured.take_id.clone(),
            default_path: captured.default_path.clone(),
            final_path: Some(captured.default_path.clone()),
            video_path: captured.video_path.clone(),
            sys_path: captured.sys_path.clone(),
            mic_path: captured.mic_path.clone(),
            valid: true,
            reason: reason.clone(),
            sys_had_input: Some(captured.sys_had_input),
            mic_had_input: Some(captured.mic_had_input),
        };
        *lock(&state.pending_take) = Some(pending.clone());
        write_pending_journal(&output_dir_of(&captured.default_path), &pending);
        state.pending_dialog.store(true, Ordering::SeqCst);
        tracing::info!("Recording stopped: {:?}", captured.default_path);
        StopResult {
            take_id: captured.take_id,
            default_path: captured.default_path.clone(),
            final_path: Some(captured.default_path),
            valid: true,
            validation_reason: reason,
            offsets_ms: captured.offsets,
            warnings,
            retained: Vec::new(),
        }
    } else {
        // Temps are retained for retry on every failed verdict (valid-looking
        // partial output after a failed mux included: the mux exit decides,
        // never the file scan alone).
        let retained = existing_temps(&captured);
        // Also retain the partial final file (if any) for inspection; the
        // retry path re-muxes from temps and overwrites it.
        let pending = PendingTake {
            take_id: captured.take_id.clone(),
            default_path: captured.default_path.clone(),
            final_path: None,
            video_path: captured.video_path.clone(),
            sys_path: captured.sys_path.clone(),
            mic_path: captured.mic_path.clone(),
            valid: false,
            reason: reason.clone(),
            sys_had_input: Some(captured.sys_had_input),
            mic_had_input: Some(captured.mic_had_input),
        };
        *lock(&state.pending_take) = Some(pending.clone());
        write_pending_journal(&output_dir_of(&captured.default_path), &pending);
        state.pending_dialog.store(true, Ordering::SeqCst);
        StopResult {
            take_id: captured.take_id,
            default_path: captured.default_path,
            final_path: None,
            valid: false,
            validation_reason: reason,
            offsets_ms: captured.offsets,
            warnings,
            retained,
        }
    }
}

/// Rich stop: capture + mux + validate + warnings + retained temps.
pub fn stop_recording_ex(state: &RecorderState) -> Result<StopResult, String> {
    if !state.is_recording.load(Ordering::SeqCst) {
        return Err("Not recording".to_string());
    }
    if state.finalizing.load(Ordering::SeqCst) {
        return Err("Already finalizing".to_string());
    }
    state.finalizing.store(true, Ordering::SeqCst);
    let captured = stop_capture(state)?;
    let result = finalize_captured(state, captured);
    state.finalizing.store(false, Ordering::SeqCst);
    Ok(result)
}

/// Re-attempt finalization from retained temp artifacts (no re-record).
pub fn retry_finalize(state: &RecorderState, take_id: &str) -> Result<StopResult, String> {
    if state.finalizing.load(Ordering::SeqCst) {
        return Err("Already finalizing".to_string());
    }
    let pending = lock(&state.pending_take)
        .clone()
        .ok_or_else(|| "No pending take to retry".to_string())?;
    if pending.take_id != take_id {
        return Err("Take id mismatch".to_string());
    }
    state.finalizing.store(true, Ordering::SeqCst);
    let sys_data_bytes = pending.sys_path.as_deref().map(wav_data_bytes).unwrap_or(0);
    let mic_data_bytes = pending.mic_path.as_deref().map(wav_data_bytes).unwrap_or(0);
    // Old journals predate the input flag: fall back to size>0 (a padded
    // file counted as data back then, so retry keeps old semantics).
    let captured = CapturedTake {
        take_id: pending.take_id.clone(),
        default_path: pending.default_path.clone(),
        video_path: pending.video_path.clone(),
        sys_path: pending.sys_path.clone(),
        mic_path: pending.mic_path.clone(),
        sys_requested: pending.sys_path.is_some(),
        mic_requested: pending.mic_path.is_some(),
        sys_data_bytes,
        mic_data_bytes,
        sys_had_input: pending.sys_had_input.unwrap_or(sys_data_bytes > 0),
        mic_had_input: pending.mic_had_input.unwrap_or(mic_data_bytes > 0),
        offsets: lock(&state.last_offsets).clone(),
    };
    let result = finalize_captured(state, captured);
    state.finalizing.store(false, Ordering::SeqCst);
    Ok(result)
}

/// Last-take per-track offsets for UI/logs.
pub fn get_av_offsets(state: &RecorderState) -> AvOffsets {
    lock(&state.last_offsets).clone()
}

/// Target-path rule shared with `save_dialog::DialogSession::target_for`
/// (".mp4" appended unless already present, case-insensitive). Single source
/// of truth so backend rename and dialog-session save never disagree on the
/// final name; keep both in sync when this rule changes.
fn target_for(dir: &str, sanitized: &str) -> std::path::PathBuf {
    if sanitized.to_lowercase().ends_with(".mp4") {
        std::path::Path::new(dir).join(sanitized)
    } else {
        std::path::Path::new(dir).join(format!("{}.mp4", sanitized))
    }
}

/// Rename a finalized take inside the output dir (backend authoritative:
/// `save_dialog::sanitize_filename`, the same validator used by
/// `DialogSession::confirm_save`, so validation can never drift).
/// Outcome mapping to `DialogSession` semantics (surfaced as the Err
/// strings the UI already handles, no behavior change):
/// - `ValidationError` ⇔ `Err("Invalid filename: ...")`
/// - `NeedsOverwriteConfirm` ⇔ `Err("Collision: ... already exists")`
///   (UI confirms, then retries with `overwrite = true`)
/// - `Saved` ⇔ `Ok(target)` (temps swept, journal cleared, dialog flags reset)
/// - `AlreadyResolved` ⇔ take-token gone (`"No pending take"` /
///   `"Take id mismatch"`): the pending slot is cleared on resolution, so a
///   double-clicked Save executes exactly once.
pub fn rename_take_file(
    state: &RecorderState,
    take_id: &str,
    new_name: &str,
    overwrite: bool,
) -> Result<String, String> {
    let pending = lock(&state.pending_take)
        .clone()
        .ok_or_else(|| "No pending take".to_string())?;
    if pending.take_id != take_id {
        return Err("Take id mismatch".to_string());
    }
    let current = pending
        .final_path
        .clone()
        .unwrap_or_else(|| pending.default_path.clone());
    let sanitized = crate::save_dialog::sanitize_filename(new_name)
        .map_err(|e| format!("Invalid filename: {:?}", e))?;
    let dir = output_dir_of(&pending.default_path);
    let target = target_for(&dir, &sanitized);
    let target_str = target.to_string_lossy().into_owned();
    if target.exists() && target.to_string_lossy() != current && !overwrite {
        return Err(format!("Collision: {} already exists", target_str));
    }
    if target.to_string_lossy() != current && std::path::Path::new(&current).exists() {
        std::fs::rename(&current, &target)
            .or_else(|_| {
                std::fs::copy(&current, &target).map(|_| ()).and_then(|_| {
                    std::fs::remove_file(&current).map_err(|e| std::io::Error::other(e.to_string()))
                })
            })
            .map_err(|e| format!("Rename failed: {}", e))?;
    }
    sweep_temps_for(&pending.default_path, &pending.sys_path, &pending.mic_path);
    clear_pending_journal(&dir, take_id);
    state.pending_dialog.store(false, Ordering::SeqCst);
    *lock(&state.pending_take) = None;
    tracing::info!("take renamed: {} -> {}", current, target_str);
    Ok(target_str)
}

/// Permanently delete a take: final + all temp artifacts + journal.
/// Outcome mapping to `save_dialog::DeleteOutcome` (same Err strings the
/// UI already handles): `Deleted` ⇔ `Ok(())`; `MissingArtifact` ⇔
/// `Err("Final file missing: ...")`; a locked file ⇔
/// `Err("Delete failed (file may be locked): ...")` with retry offered by
/// the UI, which never claims a deletion that did not happen.
/// `AlreadyResolved` ⇔ take-token gone (`"No pending take"` /
/// `"Take id mismatch"`), so double-clicked Delete executes exactly once.
pub fn delete_take_files(state: &RecorderState, take_id: &str) -> Result<(), String> {
    let pending = lock(&state.pending_take)
        .clone()
        .ok_or_else(|| "No pending take".to_string())?;
    if pending.take_id != take_id {
        return Err("Take id mismatch".to_string());
    }
    let final_path = pending
        .final_path
        .clone()
        .unwrap_or_else(|| pending.default_path.clone());
    let mut removed = 0;
    if std::path::Path::new(&final_path).exists() {
        std::fs::remove_file(&final_path)
            .map_err(|e| format!("Delete failed (file may be locked): {}", e))?;
        removed += 1;
    } else if pending.valid {
        return Err(format!("Final file missing: {}", final_path));
    }
    sweep_temps_for(&pending.default_path, &pending.sys_path, &pending.mic_path);
    let dir = output_dir_of(&pending.default_path);
    clear_pending_journal(&dir, take_id);
    state.pending_dialog.store(false, Ordering::SeqCst);
    *lock(&state.pending_take) = None;
    tracing::info!("take deleted: {} ({} files)", final_path, removed);
    Ok(())
}

/// Discard retained temps after a failed take (explicit user discard).
pub fn discard_take_files(state: &RecorderState, take_id: &str) -> Result<(), String> {
    let pending = lock(&state.pending_take)
        .clone()
        .ok_or_else(|| "No pending take".to_string())?;
    if pending.take_id != take_id {
        return Err("Take id mismatch".to_string());
    }
    sweep_temps_for(&pending.default_path, &pending.sys_path, &pending.mic_path);
    if !pending.valid {
        if let Some(fp) = &pending.final_path {
            let _ = std::fs::remove_file(fp);
        }
    }
    let dir = output_dir_of(&pending.default_path);
    clear_pending_journal(&dir, take_id);
    state.pending_dialog.store(false, Ordering::SeqCst);
    *lock(&state.pending_take) = None;
    Ok(())
}

/// Cancel path: keep the file under its default name, clear temps + journal.
pub fn cancel_pending_take(state: &RecorderState) -> Option<String> {
    let pending = lock(&state.pending_take).clone()?;
    sweep_temps_for(&pending.default_path, &pending.sys_path, &pending.mic_path);
    let dir = output_dir_of(&pending.default_path);
    clear_pending_journal(&dir, &pending.take_id);
    state.pending_dialog.store(false, Ordering::SeqCst);
    *lock(&state.pending_take) = None;
    Some(
        pending
            .final_path
            .unwrap_or_else(|| pending.default_path.clone()),
    )
}

pub fn stop_recording(state: &RecorderState) -> Result<Option<String>, String> {
    stop_recording_ex(state).map(|r| r.final_path)
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
        match start_audio_preview_with_retry(app_handle, true, None, "system".to_string()) {
            Ok(rec) => *lock(&state.preview_sys_audio) = Some(rec),
            Err(e) => tracing::warn!("Failed to start system audio preview: {}", e),
        }
    }

    if config.record_mic && config.microphone_name != "None" {
        match start_audio_preview_with_retry(
            app_handle,
            false,
            Some(config.microphone_name.clone()),
            "mic".to_string(),
        ) {
            Ok(rec) => *lock(&state.preview_mic_audio) = Some(rec),
            Err(e) => tracing::warn!("Failed to start mic audio preview: {}", e),
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
            resolve_monitor("monitor:\\\\.\\DISPLAY3", &ms)
                .unwrap()
                .device_id,
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
        assert_eq!(device_ids.len(), known.len(), "device ids must be unique");
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

    /// Bundled-ffmpeg binary for integration tests (repo resources). Skips
    /// when absent (checkout without binaries), like the hardware-gated
    /// tests above.
    #[cfg(test)]
    fn bundled_ffmpeg_for_test() -> Option<std::path::PathBuf> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("resources")
            .join("ffmpeg")
            .join("ffmpeg.exe");
        if p.exists() {
            Some(p)
        } else {
            None
        }
    }

    /// Manual delay compensation wires `-ss` ONLY in front of the configured
    /// leg(s); default 0 leaves args untouched (record-everything default).
    #[test]
    fn delay_compensation_seeks_only_configured_legs() {
        let dir =
            std::env::temp_dir().join(format!("dr-record-delay-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let video = dir.join("v.mp4");
        let sys = dir.join("s.wav");
        let out = dir.join("o.mp4");
        std::fs::write(&video, vec![1u8; 256]).unwrap();
        {
            let spec = hound::WavSpec {
                channels: 2,
                sample_rate: 48000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w = hound::WavWriter::create(&sys, spec).unwrap();
            for _ in 0..480 {
                w.write_sample(0.0f32).unwrap();
            }
            w.finalize().unwrap();
        }
        let (o, v, s) = (
            out.to_string_lossy().into_owned(),
            video.to_string_lossy().into_owned(),
            sys.to_string_lossy().into_owned(),
        );
        let plain = build_mux_args(&o, &v, Some(&s), None, 0, 0).unwrap();
        assert!(
            !plain.iter().any(|a| a == "-ss"),
            "zero compensation must not seek"
        );
        let comp = build_mux_args(&o, &v, Some(&s), None, 600, 0).unwrap();
        let ss = comp.iter().position(|a| a == "-ss").expect("-ss present");
        assert_eq!(comp[ss + 1], "0.600");
        assert_eq!(comp[ss + 2], "-i");
        assert_eq!(comp[ss + 3], s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// REGRESSION (field failure, exit -22): a MONO system loopback WAV fed
    /// to `-c:a aac` died with `Unsupported channel layout "1 channels
    /// (FL)"` despite perfect temps. The production mux args must normalize
    /// the layout, and this test runs the REAL bundled ffmpeg end to end to
    /// prove it — no mocks, no fixture-only assertions.
    #[test]
    fn mono_system_track_muxes_to_valid_aac_output() {
        let Some(ffmpeg) = bundled_ffmpeg_for_test() else {
            return; // no bundled binary in this checkout; skip
        };
        let dir =
            std::env::temp_dir().join(format!("dr-record-mono-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let video = dir.join("v.mp4");
        let sys = dir.join("s.wav");
        let out = dir.join("final.mp4");

        // 1s test video (real encode, no mocks).
        let st = std::process::Command::new(&ffmpeg)
            .args([
                "-y",
                "-f",
                "lavfi",
                "-i",
                "testsrc=duration=1:size=320x240:rate=30",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&video)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        assert!(
            st.map(|s| s.success()).unwrap_or(false),
            "fixture video must encode"
        );

        // MONO float WAV, exactly like a 1-channel loopback capture.
        {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 48000,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            };
            let mut w =
                hound::WavWriter::create(&sys, spec).expect("fixture wav must create");
            for i in 0..48000 {
                let t = i as f32 / 48000.0;
                w.write_sample((t * 440.0 * std::f32::consts::TAU).sin() * 0.5)
                    .expect("sample");
            }
            w.finalize().expect("fixture wav must finalize");
        }

        // The EXACT production arg builder (single sys leg).
        let (out_s, video_s, sys_s) = (
            out.to_string_lossy().into_owned(),
            video.to_string_lossy().into_owned(),
            sys.to_string_lossy().into_owned(),
        );
        let args = build_mux_args(&out_s, &video_s, Some(&sys_s), None, 0, 0)
            .expect("mux plan must accept mono input");
        let status = std::process::Command::new(&ffmpeg)
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("mux ffmpeg must spawn");
        assert!(
            status.success(),
            "mono mux must exit 0 (was exit -22 before aformat)"
        );
        let (verdict, reason) = validate_output(&out_s);
        assert_eq!(
            verdict,
            FinalFileVerdict::Valid,
            "mono mux output must validate: {}",
            reason
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
