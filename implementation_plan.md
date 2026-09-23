# Dr.Record v1.1.3 — Implementation Plan

Ordered build steps. Each step lists files to touch, tests to add, and a verify
command. Constraints: MAY touch `src-tauri/src/*.rs`, `src/*.js`, `*.html`,
`tests/*.rs`. MUST NOT change `resources/ffmpeg` binaries or lockfiles unless
required. Existing public command signatures stay (additive only). No new heavy
deps. Never weaken tests. Read `design.md` for the architecture behind this order.

## Step 1 — AV sync + validation (backend only)

Goal: SPEC F1 (criteria 2-7) with no UI change except richer logs/events.

Files to touch:

- `src-tauri/src/recorder.rs`: add `common_start` (+ wall clock), `finalizing`
  flag, `pending_take` slot, `StopResult` / `ValidationReport` / `TrackWarning`
  / `PendingTake` types; set common start before spawns in `start_recording`;
  extract `build_mux_args()` (itsoffset / adelay / aresample graph, mic boost
  preserved); add `validate_output()` tiers (exists+size, optional ffprobe,
  null-decode, track audit); split stop into `stop_capture()` +
  `finalize_take(take_id)`; retain temps on failure; orphan sweep helper;
  `sanitize_filename()` pure function (shared with Step 3).
- `src-tauri/src/audio.rs`: use `InputCallbackInfo::timestamp()` in all three
  format callbacks; add `first_sample_offset_ms` + `offset_source`; accept
  `common_start` (new param or `start_with_clock`, keep old path for previews);
  report empty-file reason from `run_wav_writer`.
- `src-tauri/src/lib.rs`: add `stop_rec_ex`, `retry_finalize`,
  `get_av_offsets` commands (additive); keep `stop_rec` as wrapper; guard
  `handle_hotkey`/`start_rec` on `is_recording + finalizing`; extend event
  payloads with `take-finalized` / `save-warning` / `save-error`; wire
  `tracing` logs (offsets, mux outcome, validation).
- `src/main.js`: render offsets/warnings/retry in notifications only.
- `src/overlay.js`: no logic change (elapsed now common-start derived).

Tests to add (all in `src-tauri/src/recorder.rs` `mod tests` + `audio.rs` tests,
plus scripted checks; never weaken existing):

- `sanitize_filename` battery: empty, whitespace, each illegal char,
  reserved names (case-insensitive), trailing dot/space, overlong, collision.
- Offset-sign math: `device_ts - common_start` positive = late.
- `build_mux_args` selection: ahead goes adelay, behind goes itsoffset,
  two-track graph carries aresample before amix, mic-only keeps volume boost.
- `validate_output` mapping: 0-byte, missing, moov-less fixtures fail;
  missing enabled track yields named warning, not silent success.
- Idempotency: double `finalize_take` / double `retry_finalize` single outcome.
- Scripted (`tests/Run-Tests.ps1` + `TEST_PLAN.md`): 60 s speech-over-clicks
  drift check, `ffmpeg -v error -i <final> -f null -` decode pass, start_time
  delta audio-vs-video vs logged offsets.

Verify: `cargo test` (must stay green) then
`powershell -ExecutionPolicy Bypass -File tests/Run-Tests.ps1`.

## Step 2 — Annotation overlay

Goal: SPEC F2 (criteria 8-13). Capture path untouched; burn-in via gdigrab.

Files to touch:

- `src-tauri/src/overlay.rs`: `create_annotation_window(monitor)`,
  `close_annotation_window()`, `set_annotation_clickthrough(bool)` over
  `set_ignore_cursor_events`; monitor-origin placement + scale math reuse.
- `src-tauri/src/lib.rs`: additive `toggle_annotation`, `set_annotation_tool`
  commands; register annotation hotkey beside existing hotkey (untouched);
  emit `annotation-state`; log arm/disarm source + tool selections.
- `annotation.html` (new): fullscreen transparent canvas + minimal toolbar
  (tools, colors min 4, thickness min 3, clear-all, armed indicator,
  disabled-when-not-recording state).
- `src/annotation.js` (new): canvas tools (pen/highlighter/arrow/text/eraser/
  clear-all), DPR scaling, Esc/button/hotkey wiring, `setIgnoreCursorEvents`
  bridge, rAF throttle, dropped-frame hooks.
- `overlay.html` / `src/overlay.js`: add annotate toggle button + armed state;
  timer path unchanged.
- `src-tauri/src/recorder.rs`: log configured-vs-actual fps during annotation
  sessions only.

Tests to add:

- Unit: tool-state machine (arm/disarm source, Esc exits mode only, toggle
  while idle disabled, empty text no-op, eraser/clear-all on empty no-op).
- Scripted: annotated recording frame extract (`-ss -frames:v 1` with/without
  marks), monitor-offset/DPR matrix (primary, negative-offset, high-DPR),
  30 FPS draw session with no observable fps drop, hotkey/button/Esc toggles
  without recording interruption, mid-stroke toggle and clear-during-draw order.

Verify: `cargo test` then
`powershell -ExecutionPolicy Bypass -File tests/Run-Tests.ps1 -Include Annotation`.

## Step 3 — Save dialog with journal

Goal: SPEC F3 (criteria 14-18). Depends on Step 1 `StopResult` + journal types.

Files to touch:

- `src-tauri/src/recorder.rs`: `rename_take` / `delete_take` / `discard_take` /
  `resolve_pending_take` logic, take journal write/clear
  (`.dr-record-pending/<take_id>.json`), startup orphan-sweep + recovery,
  authoritative `sanitize_filename` enforcement, collision handling.
- `src-tauri/src/lib.rs`: additive `rename_take`, `delete_take`,
  `discard_take`, `get_pending_take` commands; `stop_rec_ex` emits
  `take-finalized` and opens save dialog; gate `handle_hotkey`/`start_rec` on
  `pending_dialog`; tray-quit journals pending take first.
- `save-dialog.html` (new) + `src/save-dialog.js` (new): prefilled default,
  inline errors, Enter=Save / Esc=cancel, single-execution guards, `ask()`
  overwrite + delete confirms, retry button on failure state.
  (Alternative: build the modal inside `index.html` + `src/main.js` if a new
  webview is rejected; keep the same command contract.)
- `src/main.js`: dialog open wiring + final-path / deletion confirmations.

Tests to add:

- Unit (reuse Step 1 sanitizer battery): collision-without-confirm rejected,
  same-name treated as no-rename save, double-Save / double-Delete single
  execution via take-token.
- Scripted: illegal-name battery, overwrite-confirm path, quit-with-pending
  recovery, orphan sweep assert (no `_video`/`_sys`/`_mic` remain after save,
  delete, or cancel), hotkey-while-dialog gating (no second dialog, no corrupt
  in-progress recording).

Verify: `cargo test` then
`powershell -ExecutionPolicy Bypass -File tests/Run-Tests.ps1 -Include SaveDialog`.

## Step 4 — E2E hardening and go-live gates

Goal: prove all 19 SPEC criteria end to end without breaking hotkey/tray/
settings/tests.

Files to touch: `tests/*.rs`, `tests/Run-Tests.ps1`, `TEST_PLAN.md` only
(product code frozen except defect fixes under the same file constraints).

Tests to add / run:

- Full matrix: system+mic 60 s sync sample (criterion 4), corrupt-fixture
  rejection + retry-from-temps (5, 6), missing-track warning (7), annotation
  burn-in + monitor/DPR matrix + fps (11, 12, 13), dialog battery + orphans
  (15-18), hotkey/tray/settings regression + existing suite green (19).
- Manual gates: stock-player playback (both tracks audible, marks at right
  time/position, no sidecar), all modals have an exit path, crash-mid-record
  ends in graceful error with retry (never hung SAVING).

Verify: `cargo test` then full
`powershell -ExecutionPolicy Bypass -File tests/Run-Tests.ps1` (no filters),
plus stock-player spot checks. Go-live only when every gate in `design.md`
section 6 maps green and no existing test was weakened.

## Assumptions and risks (checked at build time)

- ffprobe optional (not bundled); default validation is file checks +
  bundled-ffmpeg null-decode.
- cpal timestamps may be `None`; arrival-time fallback with source marked.
- CFR video re-encode is fallback only, not default.
- Annotation hotkey binding chosen to avoid collision with the record hotkey.
- Journal location finalized in Step 3 before quit-recovery tests run.
