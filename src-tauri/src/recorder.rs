#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

use chrono::Local;
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tracing;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub struct RecorderState {
    pub is_recording: AtomicBool,
    pub process: Mutex<Option<Child>>,
    pub stdin: Mutex<Option<ChildStdin>>,
    pub start_time: Mutex<Option<Instant>>,
    pub output_path: Mutex<Option<String>>,
    pub hotkey_str: Mutex<String>,
}

impl RecorderState {
    pub fn new() -> Self {
        Self {
            is_recording: AtomicBool::new(false),
            process: Mutex::new(None),
            stdin: Mutex::new(None),
            start_time: Mutex::new(None),
            output_path: Mutex::new(None),
            hotkey_str: Mutex::new("Ctrl+Shift+R".to_string()),
        }
    }
}

pub fn generate_output_path(output_dir: &str, mode: &str) -> String {
    let now = Local::now();
    let timestamp = now.format("%Y-%m-%d_%H-%M-%S");
    let mode_label = match mode {
        "window" => "Window",
        "multimonitor" => "Multi",
        _ => "Screen",
    };
    let sep = std::path::MAIN_SEPARATOR;
    format!(
        "{}{sep}DrRecord_{}_{}.mp4",
        output_dir.trim_end_matches('\\').trim_end_matches('/'),
        mode_label,
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

#[allow(unused_variables)]
fn build_ffmpeg_args(output_path: &str, mode: &str, framerate: u32, quality: &str) -> Vec<String> {
    let crf = quality_to_crf(quality);
    let fps = if framerate == 0 { 60 } else { framerate };

    let mut args = vec![
        "-y".to_string(),
    ];

    #[cfg(target_os = "macos")]
    {
        args.extend_from_slice(&[
            "-f".to_string(), "avfoundation".to_string(),
            "-framerate".to_string(), fps.to_string(),
            "-i".to_string(), "1".to_string(),
            "-video_size".to_string(), get_display_size(),
        ]);
    }

    #[cfg(target_os = "windows")]
    {
        args.extend_from_slice(&[
            "-f".to_string(), "gdigrab".to_string(),
            "-framerate".to_string(), fps.to_string(),
        ]);

        match mode {
            "multimonitor" | "window" => {
                args.extend_from_slice(&["-i".to_string(), "desktop".to_string()]);
            }
            _ => {
                args.extend_from_slice(&[
                    "-offset_x".to_string(), "0".to_string(),
                    "-offset_y".to_string(), "0".to_string(),
                    "-i".to_string(), "desktop".to_string(),
                ]);
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        args.extend_from_slice(&[
            "-f".to_string(), "x11grab".to_string(),
            "-framerate".to_string(), fps.to_string(),
            "-i".to_string(), ":0.0".to_string(),
        ]);
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

#[cfg(target_os = "macos")]
fn get_display_size() -> String {
    use std::process::Command;
    if let Ok(out) = Command::new("system_profiler")
        .args(["SPDisplaysDataType"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            if line.contains("Resolution") {
                if let Some(res) = line.split(':').nth(1) {
                    let dims: Vec<&str> = res.trim().split_whitespace().collect();
                    if dims.len() >= 2 {
                        return format!("{}x{}", dims[0], dims[2].trim_end_matches(','));
                    }
                }
            }
        }
    }
    "1920x1080".to_string()
}

fn find_ffmpeg() -> String {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_default();

    #[cfg(target_os = "windows")]
    let ffmpeg_name = "ffmpeg.exe";
    #[cfg(not(target_os = "windows"))]
    let ffmpeg_name = "ffmpeg";

    let candidates = [
        exe_dir.join(ffmpeg_name),
        exe_dir.join("resources").join(ffmpeg_name),
        exe_dir.join("..").join("resources").join(ffmpeg_name),
    ];

    for path in &candidates {
        if path.exists() {
            tracing::info!("FFmpeg at: {:?}", path);
            return path.to_string_lossy().to_string();
        }
    }

    tracing::info!("No bundled FFmpeg, falling back to PATH");
    ffmpeg_name.to_string()
}

fn ffmpeg_on_path() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

pub fn start_recording(
    state: &RecorderState,
    output_dir: &str,
    mode: &str,
    framerate: u32,
    quality: &str,
) -> Result<String, String> {
    if state.is_recording.load(Ordering::SeqCst) {
        return Err("Already recording".to_string());
    }

    let ffmpeg_path = find_ffmpeg();
    let ff_exists = if ffmpeg_path == "ffmpeg" {
        ffmpeg_on_path()
    } else {
        std::path::Path::new(&ffmpeg_path).exists()
    };
    if !ff_exists {
        return Err("FFmpeg not found. Ensure ffmpeg.exe is bundled or on PATH.".to_string());
    }

    let output_path = generate_output_path(output_dir, mode);
    let args = build_ffmpeg_args(&output_path, mode, framerate, quality);
    tracing::info!("FFmpeg args: {:?}", args);

    let mut cmd = Command::new(&ffmpeg_path);
    cmd.args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null());

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

    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to start ffmpeg: {}", e))?;

    let stdin = child.stdin.take()
        .ok_or_else(|| "Failed to capture ffmpeg stdin".to_string())?;

    state.is_recording.store(true, Ordering::SeqCst);
    *lock(&state.process) = Some(child);
    *lock(&state.stdin) = Some(stdin);
    *lock(&state.start_time) = Some(Instant::now());
    *lock(&state.output_path) = Some(output_path.clone());

    tracing::info!("Recording started: {}", output_path);
    Ok(output_path)
}

pub fn stop_recording(state: &RecorderState) -> Result<Option<String>, String> {
    if !state.is_recording.load(Ordering::SeqCst) {
        return Err("Not recording".to_string());
    }

    tracing::info!("Stopping recording...");

    if let Some(mut stdin) = lock(&state.stdin).take() {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(stdin);
    }

    if let Some(mut child) = lock(&state.process).take() {
        let _ = child.wait();
        tracing::info!("FFmpeg exited");
    }

    state.is_recording.store(false, Ordering::SeqCst);
    *lock(&state.start_time) = None;

    let path = lock(&state.output_path).take();
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
