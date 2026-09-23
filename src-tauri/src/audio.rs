use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use hound::{WavSpec, WavWriter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

pub struct AudioRecorder {
    stream: Option<Stream>,
    is_running: Arc<AtomicBool>,
    writer_thread: Option<thread::JoinHandle<()>>,
    first_sample_offset_ms: Arc<Mutex<Option<i64>>>,
    offset_source: Arc<Mutex<String>>,
    /// True once any non-empty device buffer arrived. Distinguishes a live
    /// but silent room (callbacks flow, track kept) from a dead device
    /// (nothing ever arrived: padded file must NOT masquerade as data).
    received_input: Arc<AtomicBool>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            is_running: Arc::new(AtomicBool::new(false)),
            writer_thread: None,
            first_sample_offset_ms: Arc::new(Mutex::new(None)),
            offset_source: Arc::new(Mutex::new("none".to_string())),
            received_input: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Signed per-track offset (ms, positive = audio late) measured from the
    /// shared recording start to this track's first captured sample.
    pub fn first_sample_offset_ms(&self) -> Option<i64> {
        *self
            .first_sample_offset_ms
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// How the offset was measured: "arrival" (arrival-time fallback) or
    /// "none" (no clock / no samples yet).
    pub fn offset_source(&self) -> String {
        self.offset_source
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Whether any real device buffer arrived during this recording.
    /// False + empty file = dead device (exclude with a named warning);
    /// true = live device, even if the room was silent.
    pub fn has_received_input(&self) -> bool {
        self.received_input.load(Ordering::SeqCst)
    }

    pub fn start(
        &mut self,
        app: AppHandle,
        is_system: bool,
        device_name: Option<String>,
        output_path: Option<String>,
        source_name: String,
    ) -> Result<(), String> {
        self.start_with_clock(app, is_system, device_name, output_path, source_name, None)
    }

    /// Start the capture stream, measuring the first-sample offset against
    /// the shared `common_start` clock (set BEFORE ffmpeg spawn + audio
    /// start in `recorder.rs`). `None` preserves the preview path (no clock,
    /// offset stays `None` / source `"none"`).
    ///
    /// The cpal device timestamp (`info.timestamp()`) is read on every
    /// callback, but cpal 0.18 stream clocks are per-stream (origins are not
    /// comparable to `std::time::Instant`), so the reported offset uses the
    /// arrival time `Instant::now() - common_start` and is marked
    /// `offset_source = "arrival"`.
    pub fn start_with_clock(
        &mut self,
        app: AppHandle,
        is_system: bool,
        device_name: Option<String>,
        output_path: Option<String>,
        source_name: String,
        common_start: Option<Instant>,
    ) -> Result<(), String> {
        let host = cpal::default_host();
        let device = if is_system {
            host.default_output_device()
                .ok_or_else(|| "No default output device available".to_string())?
        } else {
            if let Some(name) = device_name {
                let mut found_device = None;
                if let Ok(devices) = host.input_devices() {
                    for d in devices {
                        let d_name = d.to_string();
                        if d_name == name {
                            found_device = Some(d);
                            break;
                        }
                    }
                }
                found_device.ok_or_else(|| format!("Could not find microphone '{}'", name))?
            } else {
                host.default_input_device()
                    .ok_or_else(|| "No default input device available".to_string())?
            }
        };

        let supported_config = if is_system {
            device
                .default_output_config()
                .map_err(|e| format!("Failed to get default output config: {}", e))?
        } else {
            device
                .default_input_config()
                .map_err(|e| format!("Failed to get default input config: {}", e))?
        };

        let sample_format = supported_config.sample_format();
        let config: cpal::StreamConfig = supported_config.into();

        let sample_rate = config.sample_rate;
        let channels = config.channels;

        let spec = WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let is_running = self.is_running.clone();

        // First-sample offset capture against the shared recording clock.
        // `common_start` is `Some` on the record path (set BEFORE spawns)
        // and `None` on the preview path. The device timestamp is read on
        // every callback; the reported offset uses arrival time because
        // cpal stream clocks are per-stream and not comparable to `Instant`.
        let first_offset = self.first_sample_offset_ms.clone();
        let offset_src = self.offset_source.clone();
        let got_input = self.received_input.clone();
        *first_offset.lock().unwrap_or_else(|e| e.into_inner()) = None;
        *offset_src.lock().unwrap_or_else(|e| e.into_inner()) = "none".to_string();
        got_input.store(false, Ordering::SeqCst);
        let mark_first_sample = move |info: &cpal::InputCallbackInfo| {
            let _device_ts = info.timestamp();
            // Any callback carrying samples proves a live device, even when
            // the room is silent (zeros are data).
            got_input.store(true, Ordering::SeqCst);
            if first_offset
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
            {
                return;
            }
            if let Some(t0) = common_start {
                let ms = Instant::now().saturating_duration_since(t0).as_millis();
                // Arrival is never before the shared start, so this is
                // >= 0 (audio late); early offsets (< 0) arise only when a
                // device timestamp predates the clock and are handled by the
                // mux-time `-itsoffset` path via `av_sync::offset_ms`.
                let ms = ms.min(i64::MAX as u128) as i64;
                *first_offset.lock().unwrap_or_else(|e| e.into_inner()) = Some(ms);
                *offset_src.lock().unwrap_or_else(|e| e.into_inner()) = "arrival".to_string();
            }
        };

        let track_label = if is_system { "system" } else { "mic" }.to_string();
        let err_fn = move |err| {
            tracing::error!("an error occurred on {} stream: {}", track_label, err);
        };

        let event_name = format!("audio-level-{}", source_name);

        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], info: &cpal::InputCallbackInfo| {
                    if !data.is_empty() {
                        mark_first_sample(info);
                    }
                    let mut sum_sq = 0.0;
                    let mut samples = Vec::with_capacity(data.len());
                    for &sample in data {
                        sum_sq += sample * sample;
                        samples.push(sample);
                    }
                    let _ = tx.send(samples);

                    if !data.is_empty() {
                        let rms = (sum_sq / data.len() as f32).sqrt();
                        let _ = app.emit(&event_name, rms);
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], info: &cpal::InputCallbackInfo| {
                    if !data.is_empty() {
                        mark_first_sample(info);
                    }
                    let mut sum_sq = 0.0;
                    let mut samples = Vec::with_capacity(data.len());
                    for &sample in data {
                        let f = sample as f32 / i16::MAX as f32;
                        sum_sq += f * f;
                        samples.push(f);
                    }
                    let _ = tx.send(samples);

                    if !data.is_empty() {
                        let rms = (sum_sq / data.len() as f32).sqrt();
                        let _ = app.emit(&event_name, rms);
                    }
                },
                err_fn,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                config,
                move |data: &[u16], info: &cpal::InputCallbackInfo| {
                    if !data.is_empty() {
                        mark_first_sample(info);
                    }
                    let mut sum_sq = 0.0;
                    let mut samples = Vec::with_capacity(data.len());
                    for &sample in data {
                        let f = (sample as f32 - 32768.0) / 32768.0;
                        sum_sq += f * f;
                        samples.push(f);
                    }
                    let _ = tx.send(samples);

                    if !data.is_empty() {
                        let rms = (sum_sq / data.len() as f32).sqrt();
                        let _ = app.emit(&event_name, rms);
                    }
                },
                err_fn,
                None,
            ),
            _ => return Err("Unsupported sample format".to_string()),
        }
        .map_err(|e| format!("Failed to build input stream: {}", e))?;

        is_running.store(true, Ordering::SeqCst);

        if let Some(path) = output_path {
            let is_running_writer = is_running.clone();
            let expected_rate = sample_rate as f64 * channels as f64;
            self.writer_thread = Some(thread::spawn(move || {
                run_wav_writer(rx, path, spec, is_running_writer, expected_rate);
            }));
        }

        stream.play().map_err(|e| {
            is_running.store(false, Ordering::SeqCst);
            format!("Failed to play stream: {}", e)
        })?;

        self.stream = Some(stream);

        Ok(())
    }

    pub fn stop(&mut self) {
        self.is_running.store(false, Ordering::SeqCst);
        if let Some(stream) = self.stream.take() {
            let _ = stream.pause();
        }
        if let Some(handle) = self.writer_thread.take() {
            let _ = handle.join();
        }
    }
}

fn run_wav_writer(
    rx: mpsc::Receiver<Vec<f32>>,
    path: String,
    spec: WavSpec,
    is_running: Arc<AtomicBool>,
    expected_rate: f64,
) {
    let mut writer = match WavWriter::create(&path, spec) {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to create WavWriter: {}", e);
            return;
        }
    };

    // Gap clock runs from thread spawn (≈ recording start): the WAV stays
    // wall-clock aligned through device gaps — leading silence, mid-take
    // stalls, wake-up delays. Because every track shares this alignment,
    // the mux must NOT additionally shift tracks (`-itsoffset` would apply
    // the delay twice: once as padding, once as shift). A device that never
    // delivers yields a padded-but-contentless file, which the caller
    // excludes via `has_received_input` (never silently muxed).
    let start_time = std::time::Instant::now();
    let mut samples_written: u64 = 0;

    while is_running.load(Ordering::SeqCst) {
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(samples) => {
                let count = samples.len() as u64;
                for sample in samples {
                    let _ = writer.write_sample(sample);
                }
                samples_written += count;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let elapsed = start_time.elapsed().as_secs_f64();
                let expected = (elapsed * expected_rate) as u64;
                if expected > samples_written {
                    let missing = expected - samples_written;
                    for _ in 0..missing {
                        let _ = writer.write_sample(0.0f32);
                    }
                    samples_written += missing;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    // Drain remaining samples
    while let Ok(samples) = rx.try_recv() {
        for sample in samples {
            let _ = writer.write_sample(sample);
        }
    }
    let _ = writer.finalize();
    tracing::info!("Audio writer for {} finished.", path);
}

pub fn get_microphones() -> Vec<String> {
    let mut mics = Vec::new();
    if let Ok(host) = cpal::default_host().input_devices() {
        for device in host {
            mics.push(device.to_string());
        }
    }
    mics
}

/// Friendly name of the default output device (loopback source), if any.
/// Uses Display (same string the mic picker matches on), since cpal 0.18
/// exposes no fallible name() on Device.
pub fn default_output_name() -> Option<String> {
    cpal::default_host()
        .default_output_device()
        .map(|d| d.to_string())
}

/// Goertzel tone check: is `freq_hz` dominant in these mono-normalized
/// samples? Pure function so the detector is unit-testable with synthetic
/// tones (no hardware). A pure tone's bin magnitude scales as N/2 vs
/// broadband, so the bar is `1.5*sqrt(N)` (floor 20): amplitude- and
/// buffer-size independent down to ~5ms device buffers, while noise
/// (~sqrt(N)/2) and off-tone music stay far below.
pub fn tone_present(samples: &[f32], sample_rate: u32, freq_hz: f32) -> bool {
    if samples.len() < 64 || sample_rate == 0 {
        return false;
    }
    let n = samples.len() as f32;
    // Nearest DFT bin (exact for the 150ms calibration tones at 48k).
    let k = (freq_hz * n / sample_rate as f32).round();
    let omega = 2.0 * std::f32::consts::PI * k / n;
    let coeff = 2.0 * omega.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    let mut energy = 0.0f32;
    for &v in samples {
        let s0 = v + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
        energy += v * v;
    }
    if energy <= 1e-9 {
        return false;
    }
    let mag = (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0).sqrt();
    mag > (1.5 * n.sqrt()).max(20.0)
}

/// Measure default-output loopback latency end to end: play a two-tone
/// blip (660Hz then 880Hz, 150ms each) on the default output while
/// listening on its loopback, and time the first tone's arrival.
/// Returns (latency_ms, device_name), or None when anything fails
/// (no device, busy/exclusive device, no onset within timeout, insane
/// value). Best-effort: callers keep the previous value on None.
/// The two-tone pattern (not one) keeps stray music/speech from
/// faking an onset: both tones must arrive ~150ms apart.
pub fn calibrate_output_latency() -> Option<(u32, String)> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    let host = cpal::default_host();
    let device = match host.default_output_device() {
        Some(d) => d,
        None => {
            tracing::warn!("calibration: no default output device");
            return None;
        }
    };
    let device_name = device.to_string();
    let supported = match device.default_output_config() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("calibration: no output config: {}", e);
            return None;
        }
    };
    let sample_format = supported.sample_format();
    let config: cpal::StreamConfig = supported.into();
    let rate = config.sample_rate as f32;

    let a_onset: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let b_onset: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let play_start: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    // Diagnostics: how many loopback buffers arrived + peak level. Silence
    // here means the tap hears nothing at all (not a detector problem).
    let cb_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let cb_peak = Arc::new(Mutex::new(0.0f32));

    // --- loopback listener -------------------------------------------------
    let mk_detect = |a: Arc<Mutex<Option<Instant>>>,
                     b: Arc<Mutex<Option<Instant>>>,
                     channels: u16,
                     sr: u32,
                     count: Arc<std::sync::atomic::AtomicU64>,
                     peak: Arc<Mutex<f32>>| {
        move |data: Vec<f32>| {
            count.fetch_add(1, Ordering::SeqCst);
            let mut top: f32 = 0.0;
            for &v in &data {
                let a = v.abs();
                if a > top {
                    top = a;
                }
            }
            {
                let mut p = peak.lock().unwrap_or_else(|e| e.into_inner());
                if top > *p {
                    *p = top;
                }
            }
            // Downmix to mono for the detector.
            let mono: Vec<f32> = if channels <= 1 {
                data
            } else {
                data.chunks(channels as usize)
                    .map(|f| f.iter().sum::<f32>() / f.len() as f32)
                    .collect()
            };
            let now = Instant::now();
            if a.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
                if tone_present(&mono, sr, 660.0) {
                    *a.lock().unwrap_or_else(|e| e.into_inner()) = Some(now);
                }
            } else if b.lock().unwrap_or_else(|e| e.into_inner()).is_none() {
                if tone_present(&mono, sr, 880.0) {
                    *b.lock().unwrap_or_else(|e| e.into_inner()) = Some(now);
                }
            }
        }
    };
    let err_in = |err| tracing::warn!("calibration loopback stream error: {}", err);
    let in_stream = match sample_format {
        SampleFormat::F32 => device.build_input_stream(
            config.clone(),
            {
                let a = a_onset.clone();
                let b = b_onset.clone();
                let ch = config.channels;
                let detect = mk_detect(a, b, ch, rate as u32, cb_count.clone(), cb_peak.clone());
                move |data: &[f32], _: &_| detect(data.to_vec())
            },
            err_in,
            None,
        ),
        SampleFormat::I16 => device.build_input_stream(
            config.clone(),
            {
                let a = a_onset.clone();
                let b = b_onset.clone();
                let ch = config.channels;
                let detect = mk_detect(a, b, ch, rate as u32, cb_count.clone(), cb_peak.clone());
                move |data: &[i16], _: &_| {
                    detect(data.iter().map(|&s| s as f32 / i16::MAX as f32).collect())
                }
            },
            err_in,
            None,
        ),
        SampleFormat::U16 => device.build_input_stream(
            config.clone(),
            {
                let a = a_onset.clone();
                let b = b_onset.clone();
                let ch = config.channels;
                let detect = mk_detect(a, b, ch, rate as u32, cb_count.clone(), cb_peak.clone());
                move |data: &[u16], _: &_| {
                    detect(
                        data.iter()
                            .map(|&s| (s as f32 - 32768.0) / 32768.0)
                            .collect(),
                    )
                }
            },
            err_in,
            None,
        ),
        _ => return None,
    }
    .ok()?;

    // --- tone player: 800ms 440Hz wake-up (swallowed if the device slept,
    // ignored by the detector), then 660Hz 200ms + 880Hz 200ms pattern ----
    const LEADER_S: f32 = 0.8;
    const TONE_S: f32 = 0.2;
    let played = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let started = play_start.clone();
    let begun = Arc::new(AtomicBool::new(false));
    let mk_tone = |played: Arc<std::sync::atomic::AtomicU64>,
                   started: Arc<Mutex<Option<Instant>>>,
                   begun: Arc<AtomicBool>,
                   channels: usize| {
        move |n: u64| -> f32 {
            if begun
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                *started.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
            }
            let i = played.fetch_add(n, Ordering::SeqCst);
            // n counts SAMPLES across all channels; divide out channels to
            // get frame time, or every tone plays channels× too fast.
            let t = (i as f32) / (rate * channels.max(1) as f32);
            let freq = if t < LEADER_S {
                440.0
            } else if t < LEADER_S + TONE_S {
                660.0
            } else if t < LEADER_S + 2.0 * TONE_S {
                880.0
            } else {
                0.0
            };
            if freq == 0.0 {
                0.0
            } else {
                (t * freq * std::f32::consts::TAU).sin() * 0.25
            }
        }
    };
    let err_out = |err| tracing::warn!("calibration playout stream error: {}", err);
    let out_stream = match sample_format {
        SampleFormat::F32 => device.build_output_stream(
            config.clone(),
            {
                let tone = mk_tone(played.clone(), started.clone(), begun.clone(), config.channels as usize);
                move |data: &mut [f32], _: &_| {
                    for s in data.iter_mut() {
                        *s = tone(1);
                    }
                }
            },
            err_out,
            None,
        ),
        SampleFormat::I16 => device.build_output_stream(
            config.clone(),
            {
                let tone = mk_tone(played.clone(), started.clone(), begun.clone(), config.channels as usize);
                move |data: &mut [i16], _: &_| {
                    for s in data.iter_mut() {
                        *s = (tone(1) * i16::MAX as f32) as i16;
                    }
                }
            },
            err_out,
            None,
        ),
        SampleFormat::U16 => device.build_output_stream(
            config.clone(),
            {
                let tone = mk_tone(played.clone(), started.clone(), begun.clone(), config.channels as usize);
                move |data: &mut [u16], _: &_| {
                    for s in data.iter_mut() {
                        *s = ((tone(1) + 1.0) * 32768.0) as u16;
                    }
                }
            },
            err_out,
            None,
        ),
        _ => return None,
    }
    .ok()?;

    in_stream.play().ok()?;
    out_stream.play().ok()?;
    tracing::info!(
        "calibration: streams playing on '{}' ({}ch)",
        device_name,
        config.channels
    );
    // Wait for the B onset (bounded); the pattern check rejects fakes.
    let deadline = Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let done = b_onset
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some();
        if done || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    let _ = out_stream.pause();
    let _ = in_stream.pause();
    tracing::info!(
        "calibration: loopback delivered {} buffers, peak {:.3}",
        cb_count.load(Ordering::SeqCst),
        cb_peak.lock().unwrap_or_else(|e| e.into_inner())
    );

    let (a, b, t0) = (
        a_onset.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        b_onset.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        play_start.lock().unwrap_or_else(|e| e.into_inner()).clone(),
    );
    let (a, b, t0) = match (a, b, t0) {
        (Some(a), Some(b), Some(t0)) => (a, b, t0),
        _ => {
            tracing::warn!("calibration: tone pattern not heard; keeping previous value");
            return None;
        }
    };
    // Pattern sanity: B must follow A by roughly the 150ms tone spacing.
    let gap = b.saturating_duration_since(a).as_millis();
    if !(80..=600).contains(&gap) {
        tracing::warn!("calibration: tone gap {}ms implausible; keeping previous value", gap);
        return None;
    }
    let latency = a
        .saturating_duration_since(t0)
        .as_millis()
        .saturating_sub((LEADER_S * 1000.0) as u128);
    if !(20..=2500).contains(&latency) {
        tracing::warn!("calibration: latency {}ms implausible; keeping previous value", latency);
        return None;
    }
    tracing::info!(
        "calibration: output '{}' loopback latency {}ms (tone gap {}ms)",
        device_name,
        latency,
        gap
    );
    Some((latency as u32, device_name))
}

/// Data-chunk payload bytes of a WAV file (0 when missing/unparseable).
/// Used by the mux planner + missing-track warnings so an enabled track
/// with no data is named, never silently dropped.
pub fn wav_data_bytes(path: &str) -> u64 {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return 0,
    };
    if let Some(pos) = bytes.windows(4).position(|w| w == b"data") {
        if bytes.len() >= pos + 8 {
            return u32::from_le_bytes([
                bytes[pos + 4],
                bytes[pos + 5],
                bytes[pos + 6],
                bytes[pos + 7],
            ]) as u64;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_finalizes_before_join_returns() {
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let is_running = Arc::new(AtomicBool::new(true));
        let path =
            std::env::temp_dir().join(format!("dr-record-writer-test-{}.wav", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let spec = WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let path_clone = path.clone();
        let is_running_writer = is_running.clone();
        let handle = thread::spawn(move || {
            run_wav_writer(
                rx,
                path_clone.to_string_lossy().into_owned(),
                spec,
                is_running_writer,
                96000.0,
            );
        });

        let _ = tx.send(vec![0.5f32; 1000]);

        // Simulate stop(): flip the flag, then join. join() must not return
        // until the writer has finalize()d the WAV header.
        is_running.store(false, Ordering::SeqCst);
        handle.join().expect("writer thread panicked");

        let bytes = std::fs::read(&path).expect("WAV file must exist after join");
        assert!(bytes.len() >= 44, "WAV must have a complete header");
        let data_chunk = bytes
            .windows(4)
            .position(|w| w == b"data")
            .expect("WAV must contain a data chunk");
        let data_size = u32::from_le_bytes([
            bytes[data_chunk + 4],
            bytes[data_chunk + 5],
            bytes[data_chunk + 6],
            bytes[data_chunk + 7],
        ]);
        assert_eq!(
            data_size as usize,
            1000 * 4,
            "finalized WAV must contain every buffered sample"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn writer_pads_leading_gap_for_wall_alignment() {
        // Device wakes ~300ms late (loopback warm-up): the gap must be
        // zero-filled so later content lands at its true wall position.
        // Without the pad, content would sit at file-start and play early.
        // (Lower bound only: scheduling can only add MORE pad time, and the
        // trailing stop is immediate so almost no tail pad accrues.)
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let is_running = Arc::new(AtomicBool::new(true));
        let path =
            std::env::temp_dir().join(format!("dr-record-pad-test-{}.wav", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let spec = WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let path_clone = path.clone();
        let is_running_writer = is_running.clone();
        let handle = thread::spawn(move || {
            run_wav_writer(
                rx,
                path_clone.to_string_lossy().into_owned(),
                spec,
                is_running_writer,
                48000.0,
            );
        });

        // No data for 300ms (slow device): gap must be padded.
        std::thread::sleep(std::time::Duration::from_millis(300));
        let _ = tx.send(vec![0.5f32; 4800]);

        // Stop at once: the drain picks up the sent vec deterministically.
        is_running.store(false, Ordering::SeqCst);
        handle.join().expect("writer thread panicked");

        let bytes = std::fs::read(&path).expect("WAV file must exist after join");
        let data_chunk = bytes
            .windows(4)
            .position(|w| w == b"data")
            .expect("WAV must contain a data chunk");
        let data_size = u32::from_le_bytes([
            bytes[data_chunk + 4],
            bytes[data_chunk + 5],
            bytes[data_chunk + 6],
            bytes[data_chunk + 7],
        ]);
        // ≥ 200ms of pad + the 4800 delivered samples (4 bytes each).
        assert!(
            data_size as usize >= 200 * 48 * 4 + 4800 * 4,
            "leading device gap must be zero-padded for alignment (got {} bytes)",
            data_size
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn tone_detector_finds_calibration_tone_and_rejects_noise() {
        // Synthetic 880Hz tone at 48k: must detect. Deterministic
        // pseudo-noise (no rand crate): must not detect. Music-like
        // broadband at similar level: must not detect.
        let sr = 48000u32;
        let tone: Vec<f32> = (0..4800)
            .map(|i| ((i as f32 / sr as f32) * 880.0 * std::f32::consts::TAU).sin() * 0.25)
            .collect();
        assert!(tone_present(&tone, sr, 880.0));
        assert!(!tone_present(&tone, sr, 660.0));
        // Deterministic hash noise in [-1, 1].
        let mut x = 0x12345678u32;
        let noise: Vec<f32> = (0..4800)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x as f32 / u32::MAX as f32) * 2.0 - 1.0
            })
            .collect();
        assert!(!tone_present(&noise, sr, 880.0));
        // Loud broadband (chord-ish sum): tone ratio must still reject.
        let chord: Vec<f32> = (0..4800)
            .map(|i| {
                let t = i as f32 / sr as f32;
                ((t * 220.0 * std::f32::consts::TAU).sin()
                    + (t * 331.0 * std::f32::consts::TAU).sin()
                    + (t * 553.0 * std::f32::consts::TAU).sin())
                    * 0.2
            })
            .collect();
        assert!(!tone_present(&chord, sr, 880.0));
        // Degenerate inputs never panic.
        assert!(!tone_present(&[], sr, 880.0));
        assert!(!tone_present(&tone, 0, 880.0));
    }

    #[test]
    fn real_system_loopback_capture_produces_valid_wav() {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            return; // No audio hardware, nothing to test
        };
        let Ok(supported) = device.default_output_config() else {
            return;
        };
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        let spec = WavSpec {
            channels: config.channels,
            sample_rate: config.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let path = std::env::temp_dir().join(format!(
            "dr-record-loopback-test-{}.wav",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let is_running = Arc::new(AtomicBool::new(true));
        let is_running_writer = is_running.clone();
        let path_clone = path.clone();
        let expected_rate = config.sample_rate as f64 * config.channels as f64;
        let writer = thread::spawn(move || {
            run_wav_writer(
                rx,
                path_clone.to_string_lossy().into_owned(),
                spec,
                is_running_writer,
                expected_rate,
            );
        });

        let err_fn = |err| tracing::error!("stream error during loopback test: {}", err);
        let stream = match sample_format {
            SampleFormat::F32 => device
                .build_input_stream(
                    config,
                    move |data: &[f32], _: &_| {
                        let _ = tx.send(data.to_vec());
                    },
                    err_fn,
                    None,
                )
                .ok(),
            SampleFormat::I16 => device
                .build_input_stream(
                    config,
                    move |data: &[i16], _: &_| {
                        let _ = tx.send(data.iter().map(|&s| s as f32 / i16::MAX as f32).collect());
                    },
                    err_fn,
                    None,
                )
                .ok(),
            SampleFormat::U16 => device
                .build_input_stream(
                    config,
                    move |data: &[u16], _: &_| {
                        let _ = tx.send(
                            data.iter()
                                .map(|&s| (s as f32 - 32768.0) / 32768.0)
                                .collect(),
                        );
                    },
                    err_fn,
                    None,
                )
                .ok(),
            _ => None,
        };
        let Some(stream) = stream else {
            return; // Loopback stream could not be built; skip
        };
        let _ = stream.play();

        let capture_duration = std::time::Duration::from_millis(1200);
        std::thread::sleep(capture_duration);

        let _ = stream.pause();
        is_running.store(false, Ordering::SeqCst);
        writer.join().expect("loopback writer thread panicked");

        let bytes = std::fs::read(&path).expect("loopback WAV must exist after join");
        let data_chunk = bytes
            .windows(4)
            .position(|w| w == b"data")
            .expect("loopback WAV must contain a data chunk");
        let data_size = u32::from_le_bytes([
            bytes[data_chunk + 4],
            bytes[data_chunk + 5],
            bytes[data_chunk + 6],
            bytes[data_chunk + 7],
        ]) as f64;

        let expected = capture_duration.as_secs_f64() * expected_rate * 4.0;
        assert!(
            data_size > 0.0 && data_size >= expected * 0.5 && data_size <= expected * 1.5,
            "loopback WAV must contain roughly the captured duration (got {} bytes, expected ~{})",
            data_size,
            expected
        );

        let _ = std::fs::remove_file(&path);
    }
}
