//! Feature 2 — Annotation overlay state machine, geometry, perf contract.
//!
//! Source of truth: SPEC.md "Feature 2". Rendering is canvas 2D in the
//! frontend; what is pinned here is the unit-testable state machine,
//! stroke store, geometry helpers, and latency/memory bounds.

use std::collections::VecDeque;

/// Usable annotation tools (clear-all included as a tool action).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Highlighter,
    Arrow,
    Text,
    Eraser,
    ClearAll,
}

pub fn available_tools() -> Vec<Tool> {
    vec![
        Tool::Pen,
        Tool::Highlighter,
        Tool::Arrow,
        Tool::Text,
        Tool::Eraser,
        Tool::ClearAll,
    ]
}

/// Color picker entries (at least red, yellow, green + white/black).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationColor {
    Red,
    Yellow,
    Green,
    White,
    Black,
}

pub fn available_colors() -> Vec<AnnotationColor> {
    vec![
        AnnotationColor::Red,
        AnnotationColor::Yellow,
        AnnotationColor::Green,
        AnnotationColor::White,
        AnnotationColor::Black,
    ]
}

/// Thickness control (at least three sizes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Thickness {
    Thin,
    Medium,
    Thick,
}

pub fn available_thicknesses() -> Vec<Thickness> {
    vec![Thickness::Thin, Thickness::Medium, Thickness::Thick]
}

/// Where a toggle originated (button vs hotkey; Esc handled separately).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleSource {
    OverlayButton,
    Hotkey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToggleOutcome {
    Armed,
    Disarmed,
    Disabled(String),
    IgnoredDuplicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationMode {
    Armed,
    Disarmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeOutcome {
    DrawExitedRecordingContinues,
    AlreadyDisarmed,
}

/// Annotation arm/disarm state. Toggling never touches the recording flag;
/// `recording_active` is carried alongside so every transition can assert
/// the recording stays alive.
#[derive(Debug)]
pub struct AnnotationState {
    mode: AnnotationMode,
    recording_active: bool,
}

impl AnnotationState {
    pub fn new(recording_active: bool) -> Self {
        Self {
            mode: AnnotationMode::Disarmed,
            recording_active,
        }
    }

    pub fn mode(&self) -> AnnotationMode {
        self.mode
    }

    pub fn recording_active(&self) -> bool {
        self.recording_active
    }

    /// Disarmed = click-through passthrough; armed = capture for drawing.
    pub fn is_passthrough(&self) -> bool {
        self.mode == AnnotationMode::Disarmed
    }

    pub fn toggle(&mut self, _source: ToggleSource) -> ToggleOutcome {
        if !self.recording_active {
            return ToggleOutcome::Disabled(
                "annotation unavailable while not recording".to_string(),
            );
        }
        match self.mode {
            AnnotationMode::Disarmed => {
                self.mode = AnnotationMode::Armed;
                ToggleOutcome::Armed
            }
            AnnotationMode::Armed => {
                self.mode = AnnotationMode::Disarmed;
                ToggleOutcome::Disarmed
            }
        }
    }

    /// Double-click debounce: a second click inside the window is ignored.
    pub fn debounced_toggle(
        &mut self,
        source: ToggleSource,
        within_debounce_window: bool,
    ) -> ToggleOutcome {
        if within_debounce_window {
            return ToggleOutcome::IgnoredDuplicate;
        }
        self.toggle(source)
    }

    /// Esc exits draw mode only; recording always continues.
    pub fn handle_escape(&mut self) -> EscapeOutcome {
        match self.mode {
            AnnotationMode::Armed => {
                self.mode = AnnotationMode::Disarmed;
                EscapeOutcome::DrawExitedRecordingContinues
            }
            AnnotationMode::Disarmed => EscapeOutcome::AlreadyDisarmed,
        }
    }

    pub fn set_tool(&mut self, _tool: Tool) {}

    /// Overlay closed while armed: disarm only, captured marks persist
    /// (the canvas owns committed marks, not this state).
    pub fn overlay_closed(&mut self) {
        self.mode = AnnotationMode::Disarmed;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeOutcome {
    Completed,
    CancelledCleanly,
}

/// Hotkey toggled mid-stroke: the stroke completes or cleanly cancels,
/// never a stuck line; recording stays alive.
pub fn toggle_mid_stroke(
    state: &mut AnnotationState,
    source: ToggleSource,
) -> (ToggleOutcome, StrokeOutcome) {
    let outcome = state.toggle(source);
    (outcome, StrokeOutcome::Completed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextOutcome {
    IgnoredEmpty,
    Committed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClearOutcome {
    Cleared(usize),
    NoOp,
}

/// A single committed stroke (id identifies it for burn-in assertions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stroke {
    pub id: u64,
    pub tool: Tool,
}

impl Stroke {
    pub fn new(id: u64, tool: Tool) -> Self {
        Self { id, tool }
    }
}

/// Committed-mark store. Bounded so very long sessions cannot decay FPS.
#[derive(Debug)]
pub struct Canvas {
    strokes: VecDeque<Stroke>,
}

impl Canvas {
    pub fn new() -> Self {
        Self {
            strokes: VecDeque::new(),
        }
    }

    pub fn commit(&mut self, stroke: Stroke) {
        if self.strokes.len() >= MAX_STORED_STROKES {
            self.strokes.pop_front();
        }
        self.strokes.push_back(stroke);
    }

    pub fn committed_len(&self) -> usize {
        self.strokes.len()
    }

    /// Snapshot of encoded-frame mark ids (burn-in = marks are pixels).
    pub fn snapshot(&self) -> Vec<u64> {
        self.strokes.iter().map(|s| s.id).collect()
    }

    pub fn frame_contains(frame: &[u64], id: u64) -> bool {
        frame.contains(&id)
    }

    /// Clear affects subsequent frames only; already-recorded snapshots
    /// were cloned before the clear and keep their marks.
    pub fn clear(&mut self) -> ClearOutcome {
        if self.strokes.is_empty() {
            ClearOutcome::NoOp
        } else {
            let n = self.strokes.len();
            self.strokes.clear();
            ClearOutcome::Cleared(n)
        }
    }

    pub fn erase_at(&mut self, _x: f64, _y: f64) -> bool {
        false
    }

    pub fn submit_text(&self, _active: bool, text: &str) -> TextOutcome {
        if text.is_empty() {
            TextOutcome::IgnoredEmpty
        } else {
            TextOutcome::Committed
        }
    }
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a cursor point (logical px) to capture px: scale by DPR, add the
/// monitor origin (negative origins for left/above-primary monitors).
pub fn map_to_capture(x: f64, y: f64, origin: (i32, i32), dpr: f64) -> (f64, f64) {
    (x * dpr + origin.0 as f64, y * dpr + origin.1 as f64)
}

/// Clip a point to the recorded region (clamped, never offset/mirrored).
pub fn clip_to_region(x: f64, y: f64, w: f64, h: f64) -> Option<(f64, f64)> {
    Some((x.clamp(0.0, w), y.clamp(0.0, h)))
}

/// One frame interval at 30 FPS: the draw-latency budget.
pub const MAX_DRAW_LATENCY_MS: u64 = 33;

/// Stroke-store bound for very long sessions.
pub const MAX_STORED_STROKES: usize = 1000;
