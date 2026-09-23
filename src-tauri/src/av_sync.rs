//! Feature 1 — Audio/Video sync + corruption fix (pure logic).
//!
//! Source of truth: SPEC.md "Feature 1". This module holds the unit-testable
//! core: shared start clock, signed offsets, drift-exact frame math,
//! final-file validation, mux planning, missing-track warnings, and the
//! single-finalizer guard. I/O lives at the edges (recorder.rs); this file
//! stays dependency-free (std only).

use std::fmt;
use std::path::Path;

/// One observable recording start shared by video + every audio track.
///
/// All elapsed readouts derive from this origin, not per-track starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingClock {
    origin_ms: i64,
}

impl RecordingClock {
    pub fn new(origin_ms: i64) -> Self {
        Self { origin_ms }
    }

    /// Elapsed ms since the shared origin. Pre-start queries clamp to 0 so
    /// the overlay timer never shows negatives.
    pub fn elapsed_ms(&self, now_ms: i64) -> i64 {
        (now_ms - self.origin_ms).max(0)
    }
}

/// Signed per-track offset in ms: positive = audio late, negative = early.
pub fn offset_ms(common_start_ms: i64, first_sample_ms: i64) -> i64 {
    first_sample_ms - common_start_ms
}

/// Exact audio frames for `video_frames` at `fps`/`sample_rate`.
///
/// Integer-exact (`frames * rate / fps`) so rounding never accumulates
/// into drift over long recordings.
pub fn compensated_audio_frames(video_frames: u64, fps: u32, sample_rate: u32) -> u64 {
    if fps == 0 {
        return 0;
    }
    video_frames * sample_rate as u64 / fps as u64
}

/// Frame-scale drift bound (one frame interval at 30 FPS).
pub fn max_allowed_drift_ms() -> u64 {
    33
}

/// First-sample lateness above which the take gets a "device woke late"
/// notice (informational only — alignment holds via zero-padding, nothing
/// is shifted, so this never affects the mux).
pub const LATE_START_NOTICE_MS: i64 = 2000;

/// Final-file validation verdict. Anything but `Valid` is a graceful error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalFileVerdict {
    Valid,
    Missing,
    ZeroByte,
    MoovMissing,
}

/// Validate a muxed final file. Never reports corrupt output as success.
///
/// Tiers: exists + size > 0, then `moov` presence (unplayable without it).
pub fn validate_final_file(path: &Path) -> FinalFileVerdict {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return FinalFileVerdict::Missing,
    };
    if meta.len() == 0 {
        return FinalFileVerdict::ZeroByte;
    }
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return FinalFileVerdict::Missing,
    };
    if bytes.is_empty() {
        return FinalFileVerdict::ZeroByte;
    }
    if contains_moov(&bytes) {
        FinalFileVerdict::Valid
    } else {
        FinalFileVerdict::MoovMissing
    }
}

fn contains_moov(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    bytes.windows(4).any(|w| w == b"moov")
}

/// Mux outcome: nonzero exit or zero output bytes is never success.
pub fn mux_succeeded(exit_code: i32, output_bytes: u64) -> bool {
    exit_code == 0 && output_bytes > 0
}

/// Temps are retained for retry on every failed verdict.
pub fn should_retain_temps(verdict: &FinalFileVerdict) -> bool {
    !matches!(verdict, FinalFileVerdict::Valid)
}

/// Mux input plan derived from retained artifact sizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MuxPlan {
    pub video_only: bool,
    pub include_sys: bool,
    pub include_mic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MuxInputsError {
    NoVideoInput,
    NoInputs,
}

/// Plan the mux from artifact sizes. Video is mandatory; audio legs with
/// no data are excluded (and flagged separately via warnings, never silent).
pub fn plan_mux(
    video_bytes: u64,
    sys_enabled: bool,
    sys_bytes: u64,
    mic_enabled: bool,
    mic_bytes: u64,
) -> Result<MuxPlan, MuxInputsError> {
    if video_bytes == 0 {
        if !sys_enabled && !mic_enabled {
            return Err(MuxInputsError::NoInputs);
        }
        return Err(MuxInputsError::NoVideoInput);
    }
    let include_sys = sys_enabled && sys_bytes > 0;
    let include_mic = mic_enabled && mic_bytes > 0;
    Ok(MuxPlan {
        video_only: !include_sys && !include_mic,
        include_sys,
        include_mic,
    })
}

/// Enabled audio track identity for warnings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioTrack {
    System,
    Mic,
}

impl fmt::Display for AudioTrack {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioTrack::System => write!(f, "system"),
            AudioTrack::Mic => write!(f, "mic"),
        }
    }
}

/// A missing enabled track: named track + human-readable reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackWarning {
    pub track: AudioTrack,
    pub reason: String,
}

/// Every enabled track with no data produces a named warning (never silent).
pub fn missing_track_warnings(
    sys_enabled: bool,
    sys_samples: u64,
    mic_enabled: bool,
    mic_samples: u64,
) -> Vec<TrackWarning> {
    let mut out = Vec::new();
    if sys_enabled && sys_samples == 0 {
        out.push(TrackWarning {
            track: AudioTrack::System,
            reason: "system audio produced no data (device lost, stream error, or empty file)"
                .to_string(),
        });
    }
    if mic_enabled && mic_samples == 0 {
        out.push(TrackWarning {
            track: AudioTrack::Mic,
            reason: "microphone produced no data (device lost, stream error, or empty file)"
                .to_string(),
        });
    }
    out
}

/// Guards finalization so double-stop / watchdog races yield one outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinalizerPhase {
    Idle,
    Finalizing,
    Done,
}

#[derive(Debug)]
pub struct FinalizerGuard {
    phase: FinalizerPhase,
}

impl FinalizerGuard {
    pub fn new() -> Self {
        Self {
            phase: FinalizerPhase::Idle,
        }
    }

    /// First caller begins finalization; concurrent seconds are no-ops.
    pub fn try_begin_finalize(&mut self) -> bool {
        if self.phase == FinalizerPhase::Idle {
            self.phase = FinalizerPhase::Finalizing;
            true
        } else {
            false
        }
    }

    pub fn complete(&mut self) {
        self.phase = FinalizerPhase::Done;
    }

    pub fn is_done(&self) -> bool {
        self.phase == FinalizerPhase::Done
    }
}

impl Default for FinalizerGuard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_start_threshold_is_sane() {
        // Offsets are telemetry-only (nothing is shifted at mux); the
        // notice threshold just decides when a take mentions a sleepy device.
        assert!(LATE_START_NOTICE_MS >= 1000);
    }
}
