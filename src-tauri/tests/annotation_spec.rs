//! Dr.Record SPEC.md — Feature 2 acceptance tests: Annotation Overlay.
//!
//! Source of truth: SPEC.md sections "Feature 2", "EDGE CASES — Feature 2",
//! and acceptance criteria 8-13. Derived from the SPEC alone.
//!
//! PROPOSED CONTRACT (minimal pure-logic surface the implementation must provide):
//! ```text
//! dr_record_lib::annotation::{
//!     Tool, available_tools, AnnotationColor, available_colors, Thickness,
//!     available_thicknesses, ToggleSource, ToggleOutcome, EscapeOutcome,
//!     AnnotationMode, AnnotationState, StrokeOutcome, toggle_mid_stroke,
//!     TextOutcome, ClearOutcome, Stroke, Canvas, map_to_capture, clip_to_region,
//!     MAX_DRAW_LATENCY_MS, MAX_STORED_STROKES,
//! }
//! ```
//! Rendering itself is covered by USER-FLOWS playback checks; what is pinned
//! here is the unit-testable state machine, geometry, and perf contract.
//!
//! Assumptions (flagged for review):
//! B1. One frame interval at 30 FPS = 33ms, so `MAX_DRAW_LATENCY_MS <= 33`
//!     (SPEC F2.7: latency below one frame interval at 30 FPS).
//! B2. Double-click debounce is modelled as an explicit
//!     `within_debounce_window` flag (deterministic; no wall-clock timing).

use dr_record_lib::annotation::{
    available_colors, available_thicknesses, available_tools, clip_to_region, map_to_capture,
    toggle_mid_stroke, AnnotationColor, AnnotationMode, AnnotationState, Canvas, ClearOutcome,
    EscapeOutcome, Stroke, StrokeOutcome, TextOutcome, Thickness, ToggleOutcome, ToggleSource,
    Tool, MAX_DRAW_LATENCY_MS, MAX_STORED_STROKES,
};

// ─── F2.1 Tools, colors, thickness ───────────────────────────────────────────

#[test]
fn all_six_tools_exist_and_are_usable() {
    let tools = available_tools();
    for required in [
        Tool::Pen,
        Tool::Highlighter,
        Tool::Arrow,
        Tool::Text,
        Tool::Eraser,
        Tool::ClearAll,
    ] {
        assert!(tools.contains(&required), "tool {required:?} must exist");
    }
    assert_eq!(tools.len(), 6);
}

#[test]
fn color_picker_has_at_least_four_colors_including_required() {
    let colors = available_colors();
    assert!(colors.len() >= 4, "at least 4 colors required");
    assert!(colors.contains(&AnnotationColor::Red));
    assert!(colors.contains(&AnnotationColor::Yellow));
    assert!(colors.contains(&AnnotationColor::Green));
    assert!(
        colors.contains(&AnnotationColor::White) || colors.contains(&AnnotationColor::Black),
        "white/black must be offered"
    );
}

#[test]
fn thickness_control_has_at_least_three_sizes() {
    let sizes = available_thicknesses();
    assert!(sizes.len() >= 3, "at least 3 sizes required");
    assert!(sizes.contains(&Thickness::Thin));
    assert!(sizes.contains(&Thickness::Medium));
    assert!(sizes.contains(&Thickness::Thick));
}

// ─── F2.2 Toggle via button AND hotkey; visible armed/disarmed ───────────────

#[test]
fn overlay_button_arms_annotation() {
    let mut state = AnnotationState::new(true);
    assert_eq!(state.mode(), AnnotationMode::Disarmed);
    assert_eq!(
        state.toggle(ToggleSource::OverlayButton),
        ToggleOutcome::Armed
    );
    assert_eq!(state.mode(), AnnotationMode::Armed);
}

#[test]
fn hotkey_arms_annotation() {
    let mut state = AnnotationState::new(true);
    assert_eq!(state.toggle(ToggleSource::Hotkey), ToggleOutcome::Armed);
    assert_eq!(state.mode(), AnnotationMode::Armed);
}

#[test]
fn toggle_reports_current_state_after_each_flip() {
    let mut state = AnnotationState::new(true);
    assert_eq!(state.toggle(ToggleSource::Hotkey), ToggleOutcome::Armed);
    assert_eq!(
        state.toggle(ToggleSource::OverlayButton),
        ToggleOutcome::Disarmed
    );
    assert_eq!(state.mode(), AnnotationMode::Disarmed);
}

#[test]
fn double_click_overlay_button_counts_as_single_toggle() {
    // USER FLOWS double-clicks: two clicks inside the debounce window arm once.
    let mut state = AnnotationState::new(true);
    assert_eq!(
        state.debounced_toggle(ToggleSource::OverlayButton, false),
        ToggleOutcome::Armed
    );
    assert_eq!(
        state.debounced_toggle(ToggleSource::OverlayButton, true),
        ToggleOutcome::IgnoredDuplicate
    );
    assert_eq!(state.mode(), AnnotationMode::Armed);
}

// ─── F2.2 + F2.4 Toggling/exiting NEVER stops the recording ──────────────────

#[test]
fn every_annotation_transition_keeps_recording_alive() {
    let mut state = AnnotationState::new(true);
    assert_eq!(
        state.toggle(ToggleSource::OverlayButton),
        ToggleOutcome::Armed
    );
    assert!(state.recording_active());
    assert_eq!(state.toggle(ToggleSource::Hotkey), ToggleOutcome::Disarmed);
    assert!(state.recording_active());
    let _ = state.toggle(ToggleSource::Hotkey);
    assert_eq!(
        state.handle_escape(),
        EscapeOutcome::DrawExitedRecordingContinues
    );
    assert!(state.recording_active());
    state.set_tool(Tool::Pen);
    state.overlay_closed();
    assert!(
        state.recording_active(),
        "closing overlay while armed must not stop recording"
    );
}

#[test]
fn escape_exits_draw_mode_only() {
    let mut state = AnnotationState::new(true);
    let _ = state.toggle(ToggleSource::Hotkey);
    assert_eq!(
        state.handle_escape(),
        EscapeOutcome::DrawExitedRecordingContinues
    );
    assert_eq!(state.mode(), AnnotationMode::Disarmed);
    assert!(state.recording_active());
}

#[test]
fn escape_while_disarmed_is_a_stable_noop() {
    let mut state = AnnotationState::new(true);
    assert_eq!(state.handle_escape(), EscapeOutcome::AlreadyDisarmed);
    assert!(state.recording_active());
}

// ─── F2.3 Passthrough when disarmed, draw when armed ─────────────────────────

#[test]
fn disarmed_mode_passes_mouse_through() {
    let state = AnnotationState::new(true);
    assert!(state.is_passthrough(), "disarmed => click-through");
}

#[test]
fn armed_mode_captures_mouse_for_drawing() {
    let mut state = AnnotationState::new(true);
    let _ = state.toggle(ToggleSource::OverlayButton);
    assert!(!state.is_passthrough(), "armed => draw, no passthrough");
}

// ─── F2.4 Exit keeps marks; F2.5 burn-in on playback ─────────────────────────

#[test]
fn overlay_closed_while_armed_disarms_but_committed_marks_persist() {
    let mut state = AnnotationState::new(true);
    let _ = state.toggle(ToggleSource::Hotkey);
    let mut canvas = Canvas::new();
    canvas.commit(Stroke::new(1, Tool::Pen));
    state.overlay_closed();
    assert_eq!(state.mode(), AnnotationMode::Disarmed);
    assert!(state.recording_active());
    assert_eq!(canvas.committed_len(), 1, "captured marks persist");
}

#[test]
fn committed_marks_are_included_in_encoded_frames() {
    // SPEC F2.5: marks are pixels in the video track; any stock player shows them.
    let mut canvas = Canvas::new();
    canvas.commit(Stroke::new(7, Tool::Arrow));
    let frame = canvas.snapshot();
    assert!(Canvas::frame_contains(&frame, 7));
}

#[test]
fn clear_only_affects_subsequent_frames_never_recorded_ones() {
    let mut canvas = Canvas::new();
    canvas.commit(Stroke::new(3, Tool::Pen));
    let recorded = canvas.snapshot();
    assert_eq!(canvas.clear(), ClearOutcome::Cleared(1));
    assert!(
        Canvas::frame_contains(&recorded, 3),
        "recorded frames keep marks"
    );
    assert!(canvas.snapshot().is_empty(), "subsequent capture is clear");
}

// ─── EDGE CASES: empty/invalid input is a no-op in a stable mode ─────────────

#[test]
fn empty_text_submission_is_noop_and_stays_in_mode() {
    let mut state = AnnotationState::new(true);
    let _ = state.toggle(ToggleSource::Hotkey);
    let mut canvas = Canvas::new();
    assert_eq!(canvas.submit_text(true, ""), TextOutcome::IgnoredEmpty);
    assert_eq!(state.mode(), AnnotationMode::Armed);
}

#[test]
fn eraser_with_nothing_under_cursor_is_noop() {
    let mut canvas = Canvas::new();
    assert!(!canvas.erase_at(960.0, 540.0));
    assert_eq!(canvas.committed_len(), 0);
}

#[test]
fn clear_all_with_no_marks_is_noop_with_state_unchanged() {
    let mut canvas = Canvas::new();
    assert_eq!(canvas.clear(), ClearOutcome::NoOp);
    assert_eq!(canvas.committed_len(), 0);
}

// ─── EDGE CASES: concurrency around strokes ──────────────────────────────────

#[test]
fn hotkey_mid_stroke_never_leaves_a_stuck_line() {
    let mut state = AnnotationState::new(true);
    let _ = state.toggle(ToggleSource::Hotkey);
    let (toggle, stroke) = toggle_mid_stroke(&mut state, ToggleSource::Hotkey);
    assert!(
        matches!(
            stroke,
            StrokeOutcome::Completed | StrokeOutcome::CancelledCleanly
        ),
        "mid-stroke toggle must complete or cleanly cancel, got {stroke:?}"
    );
    let _ = toggle;
    assert!(state.recording_active());
}

#[test]
fn clear_during_stroke_applies_after_inflight_stroke_finishes() {
    // Defined order: in-flight stroke finishes first, then clear applies.
    let mut canvas = Canvas::new();
    canvas.commit(Stroke::new(11, Tool::Pen));
    let recorded_before_clear = canvas.snapshot();
    assert_eq!(canvas.clear(), ClearOutcome::Cleared(1));
    assert!(Canvas::frame_contains(&recorded_before_clear, 11));
    assert!(canvas.snapshot().is_empty());
}

// ─── EDGE CASES: failure paths ───────────────────────────────────────────────

#[test]
fn annotation_toggle_with_no_recording_is_disabled_with_explanation() {
    let mut state = AnnotationState::new(false);
    let outcome = state.toggle(ToggleSource::OverlayButton);
    assert!(
        matches!(outcome, ToggleOutcome::Disabled(_)),
        "tools disabled when not recording, got {outcome:?}"
    );
    assert_eq!(state.mode(), AnnotationMode::Disarmed);
}

#[test]
fn new_recording_starts_disarmed_without_leaked_state() {
    // Out-of-order: pre-start toggle must not leak into the new session.
    let mut before = AnnotationState::new(false);
    let _ = before.toggle(ToggleSource::Hotkey);
    let fresh = AnnotationState::new(true);
    assert_eq!(fresh.mode(), AnnotationMode::Disarmed);
}

// ─── F2.6 Multi-monitor / DPR: correct position, clipped never offset ────────

#[test]
fn cursor_maps_identity_on_primary_at_100_percent() {
    assert_eq!(map_to_capture(400.0, 300.0, (0, 0), 1.0), (400.0, 300.0));
}

#[test]
fn cursor_maps_correctly_on_negative_offset_monitor() {
    // Monitor left of primary: origin x is negative; marks must not mirror/shift.
    assert_eq!(
        map_to_capture(100.0, 200.0, (-1920, 0), 1.0),
        (-1820.0, 200.0)
    );
    assert_eq!(map_to_capture(50.0, 60.0, (0, -1080), 1.0), (50.0, -1020.0));
}

#[test]
fn cursor_maps_correctly_on_high_dpr_display() {
    assert_eq!(map_to_capture(200.0, 100.0, (0, 0), 2.0), (400.0, 200.0));
    assert_eq!(map_to_capture(200.0, 100.0, (0, 0), 1.5), (300.0, 150.0));
}

#[test]
fn stroke_outside_recorded_area_is_clipped_never_offset() {
    assert!(clip_to_region(-50.0, 100.0, 1920.0, 1080.0).is_some());
    assert!(clip_to_region(2000.0, 100.0, 1920.0, 1080.0).is_some());
    let (x, y) = clip_to_region(-50.0, 100.0, 1920.0, 1080.0).unwrap();
    assert_eq!((x, y), (0.0, 100.0));
}

// ─── F2.7 Performance contract ───────────────────────────────────────────────

#[test]
fn draw_latency_budget_is_one_frame_interval_at_30fps() {
    // Assumption B1: SPEC "below one frame interval at 30 FPS".
    assert!(MAX_DRAW_LATENCY_MS > 0);
    assert_eq!(MAX_DRAW_LATENCY_MS, 33);
}

#[test]
fn stroke_store_is_bounded_for_very_long_sessions() {
    // Many strokes must not decay FPS: memory bounded.
    assert!(MAX_STORED_STROKES > 0);
    let mut canvas = Canvas::new();
    for id in 0..(MAX_STORED_STROKES * 10) as u64 {
        canvas.commit(Stroke::new(id, Tool::Pen));
    }
    assert!(
        canvas.committed_len() <= MAX_STORED_STROKES,
        "stroke memory must stay bounded"
    );
}
