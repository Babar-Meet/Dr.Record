use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};
use hound::{WavSpec, WavWriter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use tauri::{AppHandle, Emitter};

pub struct AudioRecorder {
    stream: Option<Stream>,
    is_running: Arc<AtomicBool>,
    writer_thread: Option<thread::JoinHandle<()>>,
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            is_running: Arc::new(AtomicBool::new(false)),
            writer_thread: None,
        }
    }

    pub fn start(
        &mut self,
        app: AppHandle,
        is_system: bool,
        device_name: Option<String>,
        output_path: Option<String>,
        source_name: String,
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

        let err_fn = move |err| {
            tracing::error!("an error occurred on stream: {}", err);
        };

        let event_name = format!("audio-level-{}", source_name);
        
        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                config.clone(),
                move |data: &[f32], _: &_| {
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
                config.clone(),
                move |data: &[i16], _: &_| {
                    let mut sum_sq = 0.0;
                    let mut samples = Vec::with_capacity(data.len());
                    for &sample in data {
                        let f = sample as f32 / std::i16::MAX as f32;
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
                config.clone(),
                move |data: &[u16], _: &_| {
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

        stream
            .play()
            .map_err(|e| {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_finalizes_before_join_returns() {
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let is_running = Arc::new(AtomicBool::new(true));
        let path = std::env::temp_dir().join(format!(
            "dr-record-writer-test-{}.wav",
            std::process::id()
        ));
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
        let data_size = u32::from_le_bytes(
            [bytes[data_chunk + 4], bytes[data_chunk + 5], bytes[data_chunk + 6], bytes[data_chunk + 7]],
        );
        assert_eq!(
            data_size as usize,
            1000 * 4,
            "finalized WAV must contain every buffered sample"
        );

        let _ = std::fs::remove_file(&path);
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
                    config.clone(),
                    move |data: &[f32], _: &_| {
                        let _ = tx.send(data.to_vec());
                    },
                    err_fn,
                    None,
                )
                .ok(),
            SampleFormat::I16 => device
                .build_input_stream(
                    config.clone(),
                    move |data: &[i16], _: &_| {
                        let _ = tx.send(
                            data.iter().map(|&s| s as f32 / std::i16::MAX as f32).collect(),
                        );
                    },
                    err_fn,
                    None,
                )
                .ok(),
            SampleFormat::U16 => device
                .build_input_stream(
                    config.clone(),
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
