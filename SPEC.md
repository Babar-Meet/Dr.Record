# Dr.Record v1.1.3 — SPEC

## Overview

Dr.Record v1.1.3 (Tauri 2 + Rust + JS + FFmpeg `gdigrab` + `cpal` audio) fixes the
two most painful field issues and adds two user-facing workflows, without changing
existing hotkey, tray, settings, or output-directory behavior.

Current architecture (observed): FFmpeg `gdigrab` writes a temp `_video.mp4` while
two independent `cpal`-based `AudioRecorder`s (system loopback + mic) write
`_sys.wav` / `_mic.wav`; on stop, FFmpeg is quit via `q`, audio writers are joined,
and a second FFmpeg invocation muxes (`copy` video + `aac` audio, `amix` when both
tracks exist) into the final `DrRecord_<Label>_<timestamp>.mp4`, then temp files are
deleted. There is no shared start clock, no measured A/V offset, no output
validation, no on-screen annotation, and no post-stop rename step: the timestamped
final path is the permanent name on disk.

This spec defines WHAT the product must do. It is implementation-agnostic: it does
not mandate threads vs tasks, specific FFmpeg flags, canvas libraries, or UI
frameworks. Any implementation that satisfies the observable behavior and acceptance
criteria is acceptable, subject to the file-touch and dependency constraints below.

Scope (may be touched later): `src-tauri/src/*.rs`, `src/*.js`, `*.html`,
`SPEC.md` itself (scope section only), plus `src-tauri/capabilities/*.json`,
`src-tauri/gen/schemas/*`, and `vite.config.js` (required for annotation
clickthrough permissions and the multi-page webview build).
Must NOT change: `resources/ffmpeg` binaries, `Cargo.lock` / `package-lock` unless
strictly required. No new heavy dependencies. Preserve existing hotkey, tray,
settings behavior. Never weaken existing tests.

### Feature 1 — Audio/Video Sync + Corruption Fix

Problem: intermittent corrupt/unplayable outputs and A/V desync (audio slightly
early or late vs screen) reported by some users.

1. Common-start clock: the product SHALL define one observable recording start
   moment shared by the video track and every enabled audio track. Elapsed time
   shown in the overlay and the muxed output SHALL both be derived from that
   shared origin, not from independent per-track start times.
2. Per-track offset measurement: for each enabled audio track, the product SHALL
   measure and expose the offset between the common start and that track's first
   captured sample (in milliseconds, signed: positive = audio late, negative =
   audio early). Offsets SHALL be recorded in logs and included in the stop/save
   result metadata.
3. Drift correction: the product SHALL keep audio and video aligned for the full
   duration of a recording (short clips and recordings longer than 10 minutes).
   Observable requirement: speech and on-screen action stay in sync on playback;
   drift SHALL NOT grow with recording length. Silence-padding, resampling, or
   mux-time offset/delay are all acceptable means; the spec requires the outcome,
   not the mechanism.
4. Corrupt-file detection: after stop/mux, the product SHALL validate the final
   file before reporting success. At minimum it SHALL detect: 0-byte or missing
   file, missing/truncated `moov` (unplayable in standard players), and FFmpeg
   mux failure (nonzero exit / no output produced). Detection may use `ffprobe`,
   bundled FFmpeg, or file-level checks; the requirement is that no corrupt file
   is reported as a successful save.
5. Graceful error + retry: when validation fails or the mux fails, the user SHALL
   see a non-technical error message with what happened and what to do next, plus
   one retry action that re-attempts finalization from retained temp artifacts
   without re-recording. Temp video/audio artifacts SHALL be retained until either
   a valid final file exists or the user explicitly discards them.
6. No silent drop: an enabled audio track that produced no data SHALL NEVER be
   silently omitted. The product SHALL surface which track is missing (system vs
   mic), why (device lost, stream error, empty file), and whether the final file
   contains video-only output. A missing enabled track is a warning state, not a
   silent success.

### Feature 2 — Annotation Overlay

Problem: users cannot draw attention on screen during recording. Reference pattern:
Dr.Player annotation UX (draw/mark while viewing, exit quits annotation mode,
marks visible on playback). Minimal viable equivalent for Dr.Record: draw on top
of the recording, marks burn into the final video, marks visible on playback.

1. Tools: while recording, the user SHALL be able to annotate with pen,
   highlighter, arrow, text, eraser, and clear-all. Color picker (at least 4
   colors including red, yellow, green, white/black) and thickness control (at
   least 3 sizes) SHALL apply to pen/highlighter/arrow/text.
2. Toggle: annotation mode SHALL be toggled via BOTH an overlay button and a
   keyboard hotkey. The overlay SHALL always show whether annotation mode is
   armed (draw) or disarmed (passthrough). Toggling SHALL NEVER stop or pause the
   recording.
3. Mouse passthrough vs draw mode: when disarmed, mouse events SHALL pass through
   to windows below (normal computer use). When armed, mouse input SHALL draw on
   the annotation layer. The mode switch SHALL be immediate and visible (cursor or
   overlay state change).
4. Exit behavior: exiting annotation mode (toggle off, hotkey, or Esc) SHALL quit
   annotation/draw mode only; it SHALL NEVER stop the recording, close the
   overlay, or quit the app. Existing marks SHALL persist on screen (as part of
   the capture) and remain in the final video.
5. Burn-in guarantee on playback: every visible mark SHALL be captured as pixels
   in the recorded video track (burned in), so any standard player shows the
   marks at the correct time and position with no extra player, plugin, or
   sidecar file. Clearing with eraser/clear-all removes only subsequent capture;
   already-recorded frames keep their marks.
6. Multi-monitor / DPR handling: annotations SHALL appear at the correct screen
   position on the monitor being recorded, including negative-offset monitors
   (left/above primary) and high-DPR/scaled displays. Marks SHALL NOT be offset,
   mirrored, clipped, or scaled relative to the cursor on any supported source
   (`all`, `monitor:<id>`).
7. Performance: with annotation armed and actively drawing, recording frame rate
   SHALL NOT drop observably below the configured frame rate on a reference
   machine, and drawing latency SHALL stay below one frame interval at 30 FPS.
   Annotation SHALL NOT cause FFmpeg crashes, encoder stalls, or new A/V desync.

### Feature 3 — Save-with-Name / Delete Dialog

Problem: every recording lands with a fixed timestamp name; users must rename or
clean up in Explorer, and failed/embarrassing takes leave files behind.

1. Post-stop modal: after every normal stop (hotkey, UI, tray), once the final
   file is validated, the product SHALL show a modal dialog (not a background
   toast) with: an editable filename field prefilled with the timestamp default
   (`DrRecord_<Label>_<timestamp>`), a `Save` action, and a `Delete Locally`
   action. The dialog SHALL appear only after finalization succeeds or fails with
   a retained artifact; it SHALL NEVER appear mid-recording.
2. Validation / sanitization: the filename field SHALL reject empty input,
   whitespace-only input, and characters illegal on Windows (`< > : " / \ | ? *`
   and control characters). Reserved names (`CON`, `PRN`, `AUX`, `NUL`,
   `COM1-9`, `LPT1-9`) SHALL be rejected case-insensitively. Overlong names
   (beyond filesystem limits) SHALL be rejected with a message. Invalid input
   SHALL keep the dialog open with an inline error; it SHALL NEVER save a bad
   name or crash.
3. Save vs Delete Locally: `Save` SHALL persist the file under the edited
   (or default) name in the configured output directory and confirm the final
   path. `Delete Locally` SHALL permanently delete the final file AND all
   associated temp artifacts (`_video`, `_sys`, `_mic`, logs for that take) and
   confirm deletion, so no manual cleanup is needed. Cancel/close behavior SHALL
   be specified and SHALL NOT orphan files (see Edge Cases).
4. Keyboard and click handling: `Enter` confirms Save, `Esc` follows the defined
   cancel semantics, double-clicking `Save` or `Delete Locally` SHALL execute the
   action exactly once (idempotent, no duplicate files), and out-of-order events
   (e.g. hotkey pressed while dialog open starts a NEW recording only if the
   dialog state is resolved first; the dialog SHALL NEVER corrupt an in-progress
   recording).
5. No orphan temp files: after the dialog resolves (save, delete, or defined
   cancel path), no `_video` / `_sys` / `_mic` temp files for that take SHALL
   remain on disk. App quit or crash with a pending dialog SHALL recover to a
   defined state on next launch (either re-offer the dialog or place the file
   under the default name and inform the user); silent orphans are not allowed.

## Non-goals

1. No editor for post-recording trimming, cutting, or re-encoding presets.
2. No cloud upload, sharing links, or account/sync features.
3. No persistent vector annotation layers, undo history across sessions, or
   collaborative cursors; burn-in only.
4. No changes to hotkey defaults, tray menu structure, settings schema, or output
   directory semantics beyond what the three features require.
5. No new heavyweight dependencies (canvas/UI frameworks, muxing libraries);
   reuse bundled FFmpeg and existing crates/packages.
6. No weakening of existing tests; new behavior ships with new tests.

## EDGE CASES

Feature 1 (sync/corruption):

1. Empty/invalid input: zero-length video temp file, zero-length audio WAV,
   missing temp file (deleted externally), mux invoked with no inputs. Each SHALL
   map to the graceful-error path, never a success toast.
2. Boundaries: recordings shorter than 1s; recordings longer than 10 min
   (drift accumulation); sample-rate mismatch between system and mic tracks;
   one enabled track produces data while the other produces none.
3. Failure paths: FFmpeg video process crashes mid-recording; `cpal` device
   unplugged mid-recording; output directory becomes unwritable; disk full
   during mux; `ffprobe`/validator binary missing. Temp artifacts retained for
   retry; user told which track/stage failed.
4. Concurrency: stop pressed twice; hotkey start pressed during mux/save;
   watchdog crash event races with normal stop. Exactly one final file outcome;
   no double-mux, no deadlock, no panic.

Feature 2 (annotation):

1. Empty/invalid input: text tool submitted with empty string (no-op, stays in
   mode); eraser stroke with nothing under cursor (no-op); clear-all with no
   marks (no-op with confirmation state unchanged).
2. Boundaries: stroke crossing monitor edges or outside the recorded area
   (clipped, never offset); extremely fast strokes (rendered as continuous line);
   very long sessions with many strokes (memory bounded, no FPS decay).
3. Failure paths: annotation toggled when no recording is active (disabled with
   explanation); overlay window closed while annotation armed (mode disarms, marks
   already captured persist); GPU/compositor slow (drawing degrades gracefully,
   recording continues).
4. Concurrency: hotkey toggles mode mid-stroke (stroke completes or cleanly
   cancels, never a stuck line); clear-all pressed while drawing (defined order:
   in-flight stroke finishes, then clear applies to subsequent frames).

Feature 3 (save/delete dialog):

1. Empty/invalid input: empty, whitespace-only, illegal-character, reserved, or
   overlong names (inline error, dialog stays open); name that collides with an
   existing file (prompt to overwrite with explicit confirm, or require a new
   name; never silent overwrite).
2. Boundaries: filename at max length; output directory path with trailing
   slash; rename to same name as default (treated as Save with no rename).
3. Failure paths: output dir deleted between stop and save; disk full on rename;
   file locked by player/antivirus during delete (report, offer retry, never
   claim deletion that did not happen); final file missing when dialog opens
   (dialog shows error state, offers discard of temps).
4. Concurrency: double-click Save/Delete (single execution); hotkey start/stop
   while dialog open (dialog input disabled or new recording queued only after
   resolution; never two dialogs); app quit with dialog open (defined recovery,
   no orphans).

## USER FLOWS

First-run:

1. User installs, launches, picks output dir, keeps defaults, presses the
   hotkey: recording starts, overlay shows REC + timer. User stops: validation
   passes, save dialog appears with timestamp default. User presses Enter: file
   saved, confirmation shows path, no temps remain.
2. User enables mic + system audio, records speech over screen action, stops:
   playback shows speech in sync with clicks; offsets logged; no warnings.
3. User arms annotation mid-recording via overlay button, draws a circle and an
   arrow, types a label, disarms with Esc: recording continues uninterrupted,
   marks appear in playback at the right place and time.

Misclicks:

1. User clicks `Delete Locally` by accident: product requires explicit confirm
   for delete; confirm deletes final + temps and reports; cancel returns to the
   dialog with the name intact.
2. User types illegal characters in the filename: inline error names the bad
   characters; Save disabled until valid; Esc follows cancel semantics without
   losing the recorded file silently.
3. User clicks annotation tools while NOT recording: tools disabled with a hint;
   nothing crashes, no phantom window.

Out-of-order:

1. User presses stop hotkey twice quickly: first stop finalizes, second is a
   no-op with an informational state (never a second dialog or error crash).
2. User presses start hotkey while the save dialog is open: dialog resolves
   first (save/delete/cancel path); only then may a new recording start.
3. User toggles annotation before starting a recording, then starts: annotation
   begins disarmed; prior toggle does not leak state into the new session.

Double-clicks:

1. Double-click `Save`: exactly one file with the chosen name; no `name (1)`
   duplicates, no second dialog.
2. Double-click overlay annotate button: single toggle (armed), not armed then
   instantly disarmed.
3. Double-click tray icon during save/mux: settings may show, but no second
   stop/mux is triggered.

Dead-ends (never crash/hang/dead-end):

1. Every modal (save dialog, overwrite confirm, error dialog) has at least one
   enabled exit path (Save / Delete / Cancel / Retry / Close) at all times.
2. FFmpeg crash mid-recording ends in the graceful-error state with retry, not a
   hung SAVING indicator; overlay timer stops; user can start a new recording.
3. Validation failure with no temp artifacts ends in an error state naming the
   missing piece, with a safe exit (close/discard) that leaves no orphans.
4. No flow leaves the app in a state where hotkey, tray, and settings all stop
   responding; the app always returns to Idle or Recording with status accurate.

## Acceptance Criteria

1. `SPEC.md` exists at the repository root and contains all sections named in
   the task (Overview, Feature 1, Feature 2, Feature 3, Non-goals, EDGE CASES,
   USER FLOWS, Acceptance Criteria, Telemetry/Logs).
2. A recording with system + mic audio stopped normally produces a final `.mp4`
   that plays in a stock player with both tracks audible.
3. Measured per-track offsets are observable (log or save-result metadata) with
   sign convention documented (positive = audio late).
4. A 60s speech-over-clicks sample shows no perceptible growing desync on
   playback (drift does not accumulate with duration).
5. A 0-byte, missing, or `moov`-less final artifact is NEVER reported as
   success; the user sees an error with a retry action.
6. A failed mux retains temp artifacts until valid output or explicit discard;
   retry without re-recording can succeed.
7. An enabled audio track with no data produces a visible warning naming the
   track; success is never claimed silently.
8. Pen, highlighter, arrow, text, eraser, and clear-all are all usable during a
   recording with color and thickness controls.
9. Annotation toggles via both overlay button and hotkey without stopping the
   recording, with visible armed/disarmed state.
10. Disarmed mode passes mouse through; armed mode draws; Esc exits draw mode
    only (recording continues).
11. Playback of an annotated recording in a stock player shows all marks at the
    correct time and position with no sidecar files.
12. Annotation position is correct on negative-offset and high-DPR monitors.
13. Drawing while recording does not drop the recorded frame rate observably
    below the configured rate.
14. Post-stop modal appears after every normal stop with prefilled timestamp
    default, editable name, Save, and Delete Locally.
15. Empty, illegal-character, reserved, and overlong names are rejected inline;
    the dialog stays open and nothing bad is saved.
16. Filename collision requires explicit overwrite confirm; silent overwrite
    never happens.
17. Double-clicking Save or Delete executes exactly once (one file, one delete).
18. After dialog resolution, no `*_video.mp4` / `*_sys.wav` / `*_mic.wav` temps
    for that take remain; Delete removes final + temps.
19. Existing hotkey, tray, settings, and test suite behavior is preserved; no
    existing test is weakened.

## Telemetry/Logs

1. All logs use the existing `tracing` pipeline; no new logging framework.
2. Recording lifecycle SHALL log: common-start timestamp, per-track first-sample
   offsets (ms, signed), configured vs actual frame rate, mux command outcome,
   validation outcome (pass/fail + reason), final path or deletion confirmation.
3. Audio warnings SHALL log: which track (`system`/`mic`), device name, stream
   errors, empty-file events, retry attempts (with backoff, mirroring the
   existing 4-attempt audio-start retry policy).
4. Annotation SHALL log: mode arm/disarm events with source (button/hotkey/Esc),
   tool/color/thickness selections (no stroke-content capture), and any dropped-
   frame or encoder-stall signal observed while drawing.
5. Save dialog SHALL log: prefilled default, final chosen name (or deletion),
   validation rejections (rule triggered, not keystrokes), overwrite confirms,
   orphan-cleanup results, and recovery actions after quit/crash with pending
   dialog.
6. Errors shown to the user SHALL be non-technical; the corresponding log entry
   SHALL carry the technical detail (FFmpeg tail, HRESULT/OS error, offsets).
7. No keystroke content, annotation text content, file contents, or PII beyond
   device/file paths already logged today SHALL be recorded.
