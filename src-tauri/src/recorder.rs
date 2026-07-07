use chrono::Local;
use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tracing;

pub struct RecorderState {
    pub is_recording: AtomicBool,
    pub process: Mutex<Option<Child>>,
    pub stdin: Mutex<Option<ChildStdin>>,
    pub start_time: Mutex<Option<Instant>>,
    pub output_path: Mutex<Option<String>>,
}

impl RecorderState {
    pub fn new() -> Self {
        Self {
            is_recording: AtomicBool::new(false),
            process: Mutex::new(None),
            stdin: Mutex::new(None),
            start_time: Mutex::new(None),
            output_path: Mutex::new(None),
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
    format!(
        "{}\\DrRecord_{}_{}.mp4",
        output_dir.trim_end_matches('\\').trim_end_matches('/'),
        mode_label,
        timestamp
    )
}

fn build_ffmpeg_args(output_path: &str, mode: &str, framerate: u32) -> Vec<String> {
    let mut args = vec![
        "-y".to_string(),
        "-f".to_string(), "gdigrab".to_string(),
        "-framerate".to_string(), framerate.to_string(),
    ];

    match mode {
        "multimonitor" => {
            args.extend_from_slice(&[
                "-i".to_string(), "desktop".to_string(),
            ]);
        }
        "window" => {
            args.extend_from_slice(&[
                "-i".to_string(), "desktop".to_string(),
            ]);
        }
        _ => {
            args.extend_from_slice(&[
                "-offset_x".to_string(), "0".to_string(),
                "-offset_y".to_string(), "0".to_string(),
                "-i".to_string(), "desktop".to_string(),
            ]);
        }
    }

    args.extend_from_slice(&[
        "-c:v".to_string(), "libx264".to_string(),
        "-preset".to_string(), "ultrafast".to_string(),
        "-crf".to_string(), "23".to_string(),
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

    let local_path = exe_dir.join("ffmpeg.exe");
    if local_path.exists() {
        tracing::info!("Using bundled FFmpeg at: {:?}", local_path);
        return local_path.to_string_lossy().to_string();
    }

    tracing::info!("No bundled FFmpeg found, falling back to PATH");
    "ffmpeg".to_string()
}

pub fn start_recording(state: &RecorderState, output_dir: &str, mode: &str, framerate: u32) -> Result<String, String> {
    if state.is_recording.load(Ordering::SeqCst) {
        return Err("Already recording".to_string());
    }

    let output_path = generate_output_path(output_dir, mode);

    let args = build_ffmpeg_args(&output_path, mode, framerate);
    tracing::info!("Starting FFmpeg with args: {:?}", args);

    let ffmpeg_path = find_ffmpeg();
    let mut child = Command::new(&ffmpeg_path)
        .args(&args)
        .stdin(Stdio::piped())
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start ffmpeg: {}. Is ffmpeg installed?", e))?;

    let stdin = child.stdin.take()
        .ok_or_else(|| "Failed to capture ffmpeg stdin".to_string())?;

    state.is_recording.store(true, Ordering::SeqCst);
    *state.process.lock().unwrap() = Some(child);
    *state.stdin.lock().unwrap() = Some(stdin);
    *state.start_time.lock().unwrap() = Some(Instant::now());
    *state.output_path.lock().unwrap() = Some(output_path.clone());

    tracing::info!("Recording started: {}", output_path);
    Ok(output_path)
}

pub fn stop_recording(state: &RecorderState) -> Result<Option<String>, String> {
    if !state.is_recording.load(Ordering::SeqCst) {
        return Err("Not recording".to_string());
    }

    tracing::info!("Stopping recording...");

    if let Some(mut stdin) = state.stdin.lock().unwrap().take() {
        let _ = stdin.write_all(b"q\n");
        let _ = stdin.flush();
        std::thread::sleep(std::time::Duration::from_millis(500));
        drop(stdin);
    }

    let mut process_guard = state.process.lock().unwrap();
    if let Some(mut child) = process_guard.take() {
        let _ = child.wait();
        tracing::info!("FFmpeg process exited");
    }
    drop(process_guard);

    state.is_recording.store(false, Ordering::SeqCst);
    *state.start_time.lock().unwrap() = None;

    let path = state.output_path.lock().unwrap().take();
    tracing::info!("Recording stopped: {:?}", path);

    Ok(path)
}

pub fn get_elapsed(state: &RecorderState) -> u64 {
    if let Some(start) = *state.start_time.lock().unwrap() {
        start.elapsed().as_secs()
    } else {
        0
    }
}
