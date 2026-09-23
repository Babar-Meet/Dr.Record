//! Dr.Record SPEC.md — Feature 1 acceptance tests: Audio/Video Sync + Corruption Fix.
//!
//! Source of truth: SPEC.md sections "Feature 1", "EDGE CASES — Feature 1
//! (sync/corruption)", "USER FLOWS", and acceptance criteria 2-7. These tests
//! are derived from the SPEC alone; they do not depend on any implementation.
//!
//! PROPOSED CONTRACT (minimal pure-logic surface the implementation must provide):
//! ```text
//! dr_record_lib::av_sync::{
//!     offset_ms, RecordingClock, validate_final_file, FinalFileVerdict,
//!     mux_succeeded, should_retain_temps, plan_mux, MuxPlan, MuxInputsError,
//!     missing_track_warnings, TrackWarning, AudioTrack,
//!     compensated_audio_frames, max_allowed_drift_ms, FinalizerGuard,
//! }
//! ```
//! If the builder places this logic under different paths, update the `use`
//! lines below. The BEHAVIORAL assertions must stay as written.
//!
//! Assumptions (flagged for review, all traceable to SPEC wording):
//! A1. `RecordingClock::elapsed_ms` clamps pre-start queries to 0 (SPEC is
//!     silent; clamping keeps the overlay timer monotonic).
//! A2. `max_allowed_drift_ms()` is asserted only as `> 0` and `<= 100`; the
//!     rigorous no-grow check is the exactness property across durations.
//!     (30 FPS default comes from TEST_PLAN TC-351.)

use dr_record_lib::av_sync::{
    compensated_audio_frames, max_allowed_drift_ms, missing_track_warnings, mux_succeeded,
    offset_ms, plan_mux, should_retain_temps, validate_final_file, AudioTrack, FinalFileVerdict,
    FinalizerGuard, MuxInputsError, RecordingClock,
};
use std::fs;
use std::path::PathBuf;
use std::process;

// ─── helpers (std only, isolated scratch dir per test) ───────────────────────

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dr-record-spec1-{}-{}", name, process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir must be creatable");
    dir
}

fn finish(dir: &PathBuf) {
    let _ = fs::remove_dir_all(dir);
}

// ─── 1. Common-start clock: one origin shared by video + every audio track ───

#[test]
fn clock_video_and_audio_elapsed_agree_on_shared_origin() {
    // SPEC F1.1: overlay timer and muxed output derive from one shared origin.
    let clock = RecordingClock::new(5_000);
    let video_elapsed = clock.elapsed_ms(6_234);
    let audio_elapsed = clock.elapsed_ms(6_234);
    assert_eq!(video_elapsed, 1_234);
    assert_eq!(audio_elapsed, video_elapsed);
}

#[test]
fn clock_elapsed_at_start_is_zero() {
    let clock = RecordingClock::new(9_000);
    assert_eq!(clock.elapsed_ms(9_000), 0);
}

#[test]
fn clock_elapsed_before_origin_clamps_to_zero() {
    // Assumption A1: pre-start queries clamp so the overlay never shows negatives.
    let clock = RecordingClock::new(9_000);
    assert_eq!(clock.elapsed_ms(8_500), 0);
}

#[test]
fn clock_covers_recordings_longer_than_10_minutes() {
    let clock = RecordingClock::new(0);
    assert_eq!(clock.elapsed_ms(601_000), 601_000);
    assert_eq!(clock.elapsed_ms(3_600_000), 3_600_000);
}

// ─── 2. Per-track offset, signed ms: positive = audio late ───────────────────

#[test]
fn offset_audio_late_is_positive() {
    assert_eq!(offset_ms(1_000, 1_040), 40);
}

#[test]
fn offset_audio_early_is_negative() {
    assert_eq!(offset_ms(1_000, 975), -25);
}

#[test]
fn offset_simultaneous_start_is_zero() {
    assert_eq!(offset_ms(7_500, 7_500), 0);
}

#[test]
fn offset_measured_for_sub_second_recordings() {
    // SPEC edge: recordings shorter than 1s still get a measured offset.
    assert_eq!(offset_ms(0, 12), 12);
    assert_eq!(offset_ms(400, 395), -5);
}

#[test]
fn offset_handles_large_values_without_overflow() {
    assert_eq!(offset_ms(0, 3_600_000), 3_600_000);
    assert_eq!(offset_ms(3_600_000, 0), -3_600_000);
}

// ─── 3. Drift correction: alignment must not grow with duration ──────────────

#[test]
fn compensated_audio_length_is_exact_at_30fps_48khz() {
    // 1s of video => exactly 1s of audio; any rounding would accumulate.
    assert_eq!(compensated_audio_frames(30, 30, 48_000), 48_000);
}

#[test]
fn compensated_audio_length_does_not_drift_over_10_minutes() {
    // SPEC F1.3: drift SHALL NOT grow with recording length.
    let one_min = compensated_audio_frames(1_800, 30, 48_000);
    let ten_min = compensated_audio_frames(18_000, 30, 48_000);
    let one_hour = compensated_audio_frames(108_000, 30, 48_000);
    assert_eq!(one_min, 2_880_000);
    assert_eq!(ten_min, 10 * one_min);
    assert_eq!(one_hour, 60 * one_min);
}

#[test]
fn compensated_audio_length_handles_mismatched_track_rates() {
    // SPEC edge: sample-rate mismatch between system and mic tracks.
    let sys = compensated_audio_frames(300, 30, 48_000);
    let mic = compensated_audio_frames(300, 30, 44_100);
    assert_eq!(sys, 480_000);
    assert_eq!(mic, 441_000);
}

#[test]
fn drift_bound_is_small_and_positive() {
    // Assumption A2: bound exists and is frame-scale, not seconds-scale.
    let bound = max_allowed_drift_ms();
    assert!(bound > 0, "drift bound must be positive");
    assert!(bound <= 100, "drift bound must stay near lip-sync scale");
}

// ─── 4. Corrupt-file detection: never report corrupt output as success ───────

#[test]
fn missing_final_file_is_never_success() {
    let dir = scratch("missing");
    let verdict = validate_final_file(&dir.join("DrRecord_Screen_2026-09-21_10-00-00.mp4"));
    assert_ne!(verdict, FinalFileVerdict::Valid);
    assert_eq!(verdict, FinalFileVerdict::Missing);
    finish(&dir);
}

#[test]
fn zero_byte_final_file_is_never_success() {
    let dir = scratch("zero-byte");
    let path = dir.join("final.mp4");
    fs::write(&path, []).unwrap();
    assert_ne!(validate_final_file(&path), FinalFileVerdict::Valid);
    finish(&dir);
}

#[test]
fn garbage_without_moov_is_never_success() {
    let dir = scratch("no-moov");
    let path = dir.join("final.mp4");
    fs::write(&path, vec![0xAAu8; 4096]).unwrap();
    let verdict = validate_final_file(&path);
    assert_ne!(verdict, FinalFileVerdict::Valid);
    assert_eq!(verdict, FinalFileVerdict::MoovMissing);
    finish(&dir);
}

#[test]
fn failed_mux_is_never_success_even_with_stale_output_present() {
    // SPEC F1.4: FFmpeg mux failure (nonzero exit / no output) => error path.
    assert!(!mux_succeeded(1, 0));
    assert!(!mux_succeeded(1, 1024));
    assert!(!mux_succeeded(0, 0));
    assert!(mux_succeeded(0, 1024));
}

// ─── 5. Graceful error + retry: retain temps until valid output or discard ───

#[test]
fn every_failed_verdict_retains_temp_artifacts_for_retry() {
    for verdict in [
        FinalFileVerdict::Missing,
        FinalFileVerdict::ZeroByte,
        FinalFileVerdict::MoovMissing,
    ] {
        assert!(
            should_retain_temps(&verdict),
            "temps must be retained for retry on {verdict:?}"
        );
    }
}

#[test]
fn mux_with_no_inputs_is_a_graceful_error_not_success() {
    // SPEC edge: mux invoked with no inputs (all temps missing/empty).
    let err = plan_mux(0, true, 0, true, 0).expect_err("empty mux must fail");
    assert!(matches!(
        err,
        MuxInputsError::NoVideoInput | MuxInputsError::NoInputs
    ));
}

#[test]
fn mux_without_video_but_with_audio_is_still_an_error() {
    let err = plan_mux(0, true, 48_000, false, 0).expect_err("video-less mux must fail");
    assert!(matches!(err, MuxInputsError::NoVideoInput));
}

// ─── 6. No silent drop: missing enabled track is a warning naming the track ──

#[test]
fn healthy_take_with_both_tracks_produces_no_warnings() {
    let warnings = missing_track_warnings(true, 480_000, true, 441_000);
    assert!(warnings.is_empty());
}

#[test]
fn empty_system_track_warns_naming_system() {
    let warnings = missing_track_warnings(true, 0, true, 441_000);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].track, AudioTrack::System);
    assert!(!warnings[0].reason.is_empty());
    let shown = format!("{}", warnings[0].track);
    assert!(shown.to_lowercase().contains("system"));
}

#[test]
fn empty_mic_track_warns_naming_mic() {
    let warnings = missing_track_warnings(true, 480_000, true, 0);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].track, AudioTrack::Mic);
    let shown = format!("{}", warnings[0].track);
    assert!(shown.to_lowercase().contains("mic"));
}

#[test]
fn both_tracks_empty_warns_for_both_tracks() {
    // SPEC edge: one enabled track has data while the other has none, and
    // the extreme where neither has data — every missing track is named.
    let warnings = missing_track_warnings(true, 0, true, 0);
    assert_eq!(warnings.len(), 2);
    let names: Vec<String> = warnings.iter().map(|w| format!("{}", w.track)).collect();
    assert!(names.iter().any(|n| n.to_lowercase().contains("system")));
    assert!(names.iter().any(|n| n.to_lowercase().contains("mic")));
}

#[test]
fn disabled_track_with_no_data_is_not_a_warning() {
    let warnings = missing_track_warnings(false, 0, true, 441_000);
    assert!(warnings.is_empty());
}

#[test]
fn video_only_plan_when_enabled_audio_missing_still_flags_warning() {
    let plan = plan_mux(1_000_000, true, 0, false, 0).expect("video-only mux is allowed");
    assert!(plan.video_only);
    assert!(!plan.include_sys);
    let warnings = missing_track_warnings(true, 0, false, 0);
    assert_eq!(
        warnings.len(),
        1,
        "video-only output must carry the warning"
    );
}

#[test]
fn full_take_plans_both_tracks_for_mix() {
    let plan = plan_mux(1_000_000, true, 480_000, true, 441_000).expect("full take must plan");
    assert!(!plan.video_only);
    assert!(plan.include_sys && plan.include_mic);
}

// ─── Concurrency: double stop => exactly one final outcome ───────────────────

#[test]
fn double_stop_finalizes_exactly_once() {
    // SPEC edge: stop pressed twice; watchdog race with normal stop.
    let mut guard = FinalizerGuard::new();
    assert!(guard.try_begin_finalize(), "first stop begins finalization");
    assert!(
        !guard.try_begin_finalize(),
        "second stop must be a no-op, never a second mux"
    );
    guard.complete();
    assert!(guard.is_done());
}

#[test]
fn fresh_recording_gets_a_fresh_finalizer() {
    let mut first = FinalizerGuard::new();
    assert!(first.try_begin_finalize());
    first.complete();
    let mut second = FinalizerGuard::new();
    assert!(second.try_begin_finalize(), "new take must be finalizable");
}
