# Dr.Record v1.1.3 — Design

Source of truth for WHAT: `SPEC.md`. Grounding for HOW: `RESEARCH.md` plus repo files
`src-tauri/src/recorder.rs`, `src-tauri/src/audio.rs`, `src-tauri/src/lib.rs`,
`src-tauri/src/overlay.rs`, `src-tauri/src/config.rs`, `src/overlay.js`, `src/main.js`.

Product intent: minimal-risk incremental changes for go-live. Preserve hotkey, tray,
settings, and output-directory behavior. No new heavy deps (canvas 2D + existing
Tauri APIs + bundled FFmpeg + existing crates only).

## 1. Baseline (observed, must keep working)

- Video: `start_recording` (`recorder.rs:414`) builds gdigrab args in
  `build_ffmpeg_args` (`recorder.rs:130`), spawns bundled `ffmpeg.exe`
  (`find_ffmpeg`, `recorder.rs:178`), stdin piped for `q` stop, stderr to
  `%TEMP%/dr-record-ffmpeg.log`, `CREATE_NO_WINDOW`. Watchdog thread
  (`recorder.rs:513`) polls `try_wait` every 500 ms, emits `recording-crashed`.
- Audio: two independent `AudioRecorder`s (`audio.rs:24`) via cpal
  (system loopback + mic), 4-attempt backoff (`start_audio_recorder_with_retry`,
  `recorder.rs:26`), F32/I16/U16 to f32, `mpsc` to `run_wav_writer` (`audio.rs:184`)
  writing 32-bit float WAV via hound with 100 ms silence padding. `stop`
  (`audio.rs:173`) pauses stream and joins writer.
- Stop/mux: `stop_recording` (`recorder.rs:557`) sends `q`, waits 3 s then kills,
  stops audio writers, runs a second FFmpeg mux (`copy` video + `aac` audio,
  `amix normalize=0` for two tracks, mic-only `volume=2.0`), then unconditionally
  deletes `_video/_sys/_mic` temps (`recorder.rs:712-715`). Mux exit status
  unchecked; no validation; missing track only `tracing::error`.
- Frontend: `lib.rs:handle_hotkey` toggles start/stop; commands `start_rec`,
  `stop_rec`, `get_elapsed_secs`, `load_config`, `save_config`, `reload_hotkey`,
  `enum_sources`, `get_thumbnail`, `get_microphones`; events `status-changed`,
  `recording-started/stopped`, `recording-error`, `audio-error`,
  `recording-crashed`. Overlay pill (`overlay.rs:4`, 180x44, transparent,
  undecorated, `focusable(false)`, bottom-right) polls `get_elapsed_secs`.
  Settings UI (`main.js`) owns sources, mics, levels, notifications.
  `tauri-plugin-dialog` registered (`lib.rs:257`), `open()` used for dir picker.

## 2. Target architecture

### 2.1 Feature 1 — Common clock, offsets, validation, retry

**Common Instant clock.** Add one `common_start: Instant` (plus `common_start_wall:
SystemTime` for logs) to `RecorderState` (`recorder.rs:56`). Set it in
`start_recording` BEFORE spawning FFmpeg or audio recorders. All elapsed-time
readouts (`get_elapsed`, overlay timer) and stop metadata derive from it.
`start_time: Mutex<Option<Instant>>` stays as the storage slot (signature of
`get_elapsed` unchanged) but its value becomes the common start. Existing
`get_elapsed_secs` command signature preserved.

**Offset metadata.** Change cpal data callbacks in `audio.rs:90,108,129` from
`|data, _: &_|` to `|data, info: &cpal::InputCallbackInfo|` and read
`info.timestamp()` (`StreamTimestamp { device, callback }`, WASAPI/QPC backed).
Record per-track `first_sample_offset_ms: Option<i64>` on `AudioRecorder`:
`offset = first_device_timestamp - common_start` in ms, signed
(positive = audio late, negative = audio early). If timestamp unavailable,
fallback = arrival time (`Instant::now() - common_start`) and mark
`offset_source: "device" | "arrival" | "none"`. Pass `common_start: Instant`
into `AudioRecorder::start` as a new trailing parameter (additive overload via
new param with default at call sites, or a new `start_with_clock` method so the
existing 5-arg signature path is preserved for previews). Log offsets via
`tracing` and include them in the stop result struct.

**Stop result struct (additive, no signature break).** Keep
`stop_recording(state) -> Result<Option<String>, String>` and
`stop_rec -> Result<Option<String>, String>` working, but add a new command
`stop_rec_ex -> Result<StopResult, String>` carrying:

```rust
struct StopResult {
  take_id: String,            // uuid or timestamp id, journal key
  default_path: String,       // DrRecord_<Label>_<timestamp>.mp4
  final_path: Option<String>, // validated output or None on failure
  valid: bool,
  validation: ValidationReport, // tier results + reason
  offsets_ms: { system: Option<i64>, mic: Option<i64> },
  offset_source: { system: String, mic: String },
  warnings: Vec<TrackWarning>,  // { track: "system"|"mic", reason }
  retained: Vec<String>,        // temp artifact paths kept for retry
}
```

`stop_rec` becomes a thin wrapper returning `final_path` so old callers
(hotkey, tray, tests) keep working. New UI uses `stop_rec_ex`.

**Drift correction at mux.** Reuse measured offsets to pick mux args in a new
`build_mux_args()` extracted from `stop_recording` (`recorder.rs:615-698`):
constant offset goes to `-itsoffset <s>` on the late/early audio input
(either direction, `-c copy` compatible) or `adelay=<ms>` when audio is ahead
only; add `aresample=async=1000,aresample=48000` (start with `async=1000`,
tune down to 1 if artifacts) on the audio leg(s) before `amix` for growing
drift, plus `-ar 48000` standardization (mux already re-encodes audio to aac,
so cost is nil). Keep `-c:v copy` default; reserve full CFR video re-encode
(`-fps_mode cfr -r <fps>`) as a fallback flag `force_cfr: bool` used only when
long-recording drift is confirmed, not the default path. Silence padding in
`run_wav_writer` stays (it already bounds short gaps).

**Validation + retry.** New `validate_output(path, expected: &ExpectedTracks)
-> ValidationReport` in `recorder.rs`, tiers cheapest first, bundled
`ffmpeg.exe` via `find_ffmpeg` only (ffprobe NOT bundled, treat as optional):

1. exists + size > 0;
2. ffprobe `-v error -show_streams -print_format json` only if resolvable
   (nonzero exit, `moov atom not found`, `Invalid data` = fail);
3. null-decode `ffmpeg -v error -i <final> -f null -` (catches truncated moov
   that probes green);
4. enabled-track audit: requested vs muxed inputs, zero-length WAV data chunk
   (`_sys.wav`/`_mic.wav`) maps to `TrackWarning { track, reason:
   device-lost | stream-error | empty-file }`, never silent.

On failure: KEEP temps (`_video/_sys/_mic` + ffmpeg log tail), emit
non-technical user error plus a `Retry` action. Split `stop_recording` into
`stop_capture()` (quit FFmpeg, join audio, collect artifacts) and
`finalize_take(take_id)` (mux + validate, reusable by retry). New command
`retry_finalize { take_id }` re-runs only mux/validate from retained artifacts,
no re-record. Add `finalizing: AtomicBool` + `pending_dialog: AtomicBool` on
`RecorderState`; `handle_hotkey`/`start_rec` check both so double-stop is a
no-op and start-during-mux is rejected with an informational event. Exactly one
final outcome, no double-mux, no deadlock, no panic.

### 2.2 Feature 2 — Annotation overlay (burn-in via gdigrab)

**Transparent annotation window with set_ignore_cursor_events toggle.**
New fullscreen window `annotation` in `overlay.rs` beside the pill:

- `create_annotation_window(monitor: &MonitorInfo)`: `transparent(true)`,
  `decorations(false)`, `always_on_top(true)`, `skip_taskbar(true)`,
  `focusable(false)`, fullscreen (or monitor-sized + positioned at monitor
  origin reusing the `monitor_position` origin/scale math, `overlay.rs:94`).
- `close_annotation_window()`, `set_annotation_clickthrough(ignore: bool)`
  wrapping `Window::set_ignore_cursor_events(bool)` (Tauri 2 desktop API;
  JS side `getCurrentWindow().setIgnoreCursorEvents()`).
- Disarmed = `ignore=true`, mouse passes through to windows below (normal use).
  Armed = `ignore=false`, window receives mouse and draws. Switch is immediate;
  overlay chrome shows armed/disarmed state (pill dot or toolbar badge + cursor
  change). Toggle sources: overlay button AND global-shortcut hotkey (new
  registration via existing `tauri-plugin-global-shortcut` pattern,
  `lib.rs:257-265`; existing record hotkey untouched) AND Esc (frontend
  keydown). Toggling never touches `RecorderState::is_recording`; closing the
  annotation window while armed disarms only; marks already captured persist.

**Burn-in via gdigrab.** No capture change needed: gdigrab reads screen pixels,
so any overlay pixels inside the capture region are recorded automatically.
Drawing surface is plain `<canvas>` 2D in `annotation.html` (new file) with
`src/annotation.js` (new file): tools pen/highlighter/arrow/text/eraser/
clear-all, color picker (min red, yellow, green, white/black), thickness
(min 3 sizes). Eraser uses `destination-out` on the canvas (later frames clean;
already-recorded frames keep marks, per SPEC). Text with empty submit is a
no-op staying in mode. Keep the drawing layer pixel-clean (no borders, no pill
inside the annotation window) so chrome never burns in. `recorder.rs` capture
path unchanged; optionally log configured-vs-actual fps alongside annotation
sessions.

**Multi-monitor / DPR.** Size and position from `enum_monitors` /
`resolve_monitor` (`recorder.rs:254`, negative offsets already handled); scale
canvas by monitor `scale_factor` and `devicePixelRatio` (same size/scale math
as `monitor_position`); clip strokes to the recorded region (`all` = virtual
desktop union, `monitor:<id>` = that monitor rect). Strokes crossing edges are
clipped, never offset/mirrored.

**Performance.** rAF-throttled canvas strokes; one composited transparent layer;
no per-stroke FFmpeg interaction. Log arm/disarm source and tool selections
(no stroke content) plus any dropped-frame/encoder-stall signal observed while
drawing.

### 2.3 Feature 3 — Save dialog modal (rename_take/delete_take + journal)

**Post-stop modal.** After `finalize_take` succeeds (or fails with retained
artifacts), backend emits `recording-stopped` with `{ take_id, default_name,
final_path, warnings }` and opens a dedicated `save-dialog` webview
(`save-dialog.html` + `src/save-dialog.js`; preferred over reusing settings to
avoid coupling to settings hide-on-close, `overlay.rs:83`). Modal shows:
editable filename prefilled `DrRecord_<Label>_<timestamp>` (no extension typed
by user; backend appends `.mp4`), `Save`, `Delete Locally`, and a defined
Cancel path. Never appears mid-recording. Enter = Save, Esc = cancel path,
double-click Save/Delete executes exactly once (frontend in-flight guard +
backend take-token guard).

**Filename sanitizer (authoritative in backend, mirrored in frontend).**
New `sanitize_filename(name: &str, output_dir: &str) -> Result<String, Rule>`
in `recorder.rs` (unit-tested): reject empty/whitespace-only, Windows-illegal
`< > : " / \ | ? *` + control chars (U+0000-U+001F), reserved
`CON PRN AUX NUL COM1-9 LPT1-9` case-insensitive (with or without extension),
trailing dot/space, overlong (component > 255 UTF-16 units or full path >
260 conservative, plus surface OS error on rename attempt). Collision with an
existing file requires explicit overwrite confirm (frontend `ask()` from the
already-registered dialog plugin); never silent overwrite. Rename to same name
as default is Save with no rename. Invalid input keeps dialog open with inline
error; nothing bad is saved; no crash.

**rename_take / delete_take + take-id journal.** New commands (all additive):

- `rename_take { take_id, new_name } -> final_path` (`std::fs::rename` inside
  configured output dir only; sanitizer runs first; collision without
  `overwrite: true` returns `Collision` error for confirm flow).
- `delete_take { take_id }` removes final + `_video/_sys/_mic` + take log entry,
  confirms deletion; locked-file failure reports without claiming deletion.
- `discard_take { take_id }`, `get_pending_take -> Option<PendingTake>`,
  `resolve_pending_take { take_id, action }` for startup recovery.

`RecorderState` gains `pending_take: Mutex<Option<PendingTake>>` where
`PendingTake { take_id, default_path, final_path, artifacts: Vec<String>,
status: Validated|FailedRetainable }`. Every finalize writes a small JSON
journal beside the output dir (or config dir):
`<output_dir>/.dr-record-pending/<take_id>.json`. Journal cleared on dialog
resolution (save/delete/discard). Startup orphan-sweep + crash recovery:
on launch, if journal entries exist, re-offer the dialog (preferred) or place
the file under the default name and inform the user; after resolution no
`_video/_sys/_mic` temps for that take remain. App quit with dialog open goes
through the same journal path. Cancel/close semantics defined once in the
dialog spec: Cancel keeps the file under the default name and closes the
dialog (no orphan, no silent loss); Delete requires explicit confirm.

## 3. Interfaces (all additions are additive; existing signatures preserved)

**New Tauri commands** (`lib.rs` invoke_handler additions; existing commands
unchanged):

| Command | Args | Returns | Notes |
|---|---|---|---|
| `stop_rec_ex` | — | `StopResult` | rich metadata; `stop_rec` wraps it |
| `retry_finalize` | `{ take_id }` | `StopResult` | mux+validate from retained temps |
| `rename_take` | `{ take_id, new_name, overwrite?: bool }` | `String` final path | sanitizer authoritative |
| `delete_take` | `{ take_id }` | `()` | final + temps + journal clear |
| `discard_take` | `{ take_id }` | `()` | drop retained temps after failure |
| `get_pending_take` | — | `Option<PendingTake>` | startup recovery |
| `toggle_annotation` | `{ source?: "button"\|"hotkey"\|"esc" }` | `{ armed: bool }` | never touches recording flag |
| `set_annotation_tool` | `{ tool, color, thickness }` | `()` | tool state only |
| `get_av_offsets` | — | `{ system, mic }` | last-take offsets for UI/logs |

**Events** (existing payloads unchanged; new events added):

- `recording-stopped` payload extended only on the new path (old `Option<String>`
  path still emitted by `stop_rec` wrapper for compat; new UI listens for the
  rich object on `take-finalized`).
- New: `take-finalized` (`StopResult`), `save-warning` (`TrackWarning[]`),
  `save-error` (`{ take_id, message, retryable }`), `annotation-state`
  (`{ armed, tool?, source }`).

**JS APIs:**

- `src/annotation.js` (new): `arm(source)`, `disarm(source)`,
  `setTool(tool)`, `setColor(c)`, `setThickness(px)`, `clearAll()`,
  DPR-aware canvas setup, Esc/button/hotkey wiring,
  `setIgnoreCursorEvents` bridging.
- `src/save-dialog.js` (new): prefill, inline validation mirror, Enter/Esc,
  single-execution guards, `ask()` overwrite/delete confirms, invoke
  `rename_take`/`delete_take`/`discard_take`/`retry_finalize`.
- `src/main.js`: render offsets/warnings/retry UI in notifications; no change
  to config schema or hotkey capture.
- `src/overlay.js`: add annotate toggle button + armed/disarmed indicator;
  elapsed still from `get_elapsed_secs` (now common-start derived). No other
  behavior change.

**Filename sanitizer contract** (`sanitize_filename`): pure function,
`(input, output_dir) -> Ok(stem) | Err { rule, message }`, rules:
`empty | illegal-chars | reserved | trailing-dot-space | too-long | collision`.
Frontend mirrors messages only; backend is authoritative.

## 4. Data flow

```
start (hotkey/UI/tray)
 -> common_start = Instant::now() (+ wall clock for logs)
 -> spawn FFmpeg gdigrab to <take>_video.mp4
 -> start AudioRecorders with common_start (first-sample offsets measured)
 -> is_recording=true, overlay pill + annotation window (disarmed) shown
capture (recording)
 -> gdigrab pixels incl. annotation overlay (burn-in automatic)
 -> WAV writers with silence padding; stream errors logged per track
stop (hotkey/UI/tray, guarded by is_recording + finalizing flags)
 -> stop_capture: q -> wait 3 s -> kill; join audio writers
 -> finalize_take(take_id): build_mux_args (itsoffset/adelay/aresample) -> mux
 -> validate_output tiers -> StopResult { offsets, warnings, retained }
 -> journal write; emit take-finalized / save-warning / save-error
dialog (save-dialog modal, never mid-recording)
 -> prefill default; inline validation; Save -> rename_take -> confirm path
 -> Delete Locally -> ask() confirm -> delete_take (final + temps + journal)
 -> Cancel -> keep default name, clear journal, no orphans
 -> failure path -> error + Retry -> retry_finalize from retained temps
finalize
 -> orphan sweep assert (no _video/_sys/_mic for take); logs; Idle
```

Concurrency rules: double-stop = second is no-op; start-during-mux/save-dialog
= rejected or queued until resolution, never two dialogs; annotation toggle
mid-stroke completes or cleanly cancels the stroke; clear-all applies after
in-flight stroke; quit-with-pending journals first.

## 5. Build order

1. AV sync + validation (backend only, no UI change except richer logs).
2. Annotation overlay (new window + canvas, capture untouched).
3. Save dialog (journal + commands + modal, then wire stop flow to it).
4. E2E + hardening (drift soak, monitor matrix, dialog battery, regression).

## 6. SPEC acceptance mapping (every criterion to file/interface)

| SPEC # | Requirement | File / interface |
|---|---|---|
| 2 | final mp4 plays, both tracks audible | `recorder.rs: build_mux_args`, `validate_output` tier 3 |
| 3 | offsets observable, sign documented | `audio.rs: first_sample_offset_ms`, `stop_rec_ex`, telemetry |
| 4 | 60 s no growing desync | `build_mux_args` (aresample) + soak test |
| 5 | 0-byte/missing/moov-less never success | `validate_output` tiers 1-3, `save-error` event |
| 6 | failed mux retains temps, retry works | `finalize_take`, `retry_finalize`, `retained` |
| 7 | empty enabled track warns by name | `TrackWarning`, `save-warning`, never silent |
| 8 | pen/highlighter/arrow/text/eraser/clear-all + color/thickness | `annotation.html`, `src/annotation.js` |
| 9 | toggle via button + hotkey, visible state, never stops rec | `toggle_annotation`, `annotation-state`, overlay button |
| 10 | disarmed passthrough, armed draws, Esc exits mode only | `set_ignore_cursor_events` toggle, Esc handler |
| 11 | marks burned in, stock player, no sidecar | gdigrab pixel capture, no recorder change |
| 12 | correct on negative-offset + high-DPR | `create_annotation_window` monitor-origin + DPR scaling |
| 13 | no FPS drop while drawing | rAF canvas, fps log, perf test |
| 14 | post-stop modal with default + Save + Delete | `save-dialog.html`, `take-finalized` |
| 15 | bad names rejected inline, dialog stays | `sanitize_filename`, inline errors |
| 16 | collision needs explicit confirm | `rename_take overwrite` + `ask()` |
| 17 | double Save/Delete executes once | take-token + frontend guards |
| 18 | no temps after resolution; Delete removes all | `delete_take`, journal clear, sweep |
| 19 | hotkey/tray/settings/tests preserved | additive-only commands/events, no test weakening |

SPEC 1 (SPEC.md exists) is satisfied by the repo itself.

## 7. Risks and assumptions

1. ffprobe not bundled (`tauri.conf.json` ships only `ffmpeg.exe`): validation
   defaults to file checks + bundled-ffmpeg null-decode; ffprobe tier optional.
   Confirm before relying on tier 2.
2. gdigrab timestamps are capture-clock, `-c copy` propagates source PTS;
   CFR re-encode kept as fallback, not default (costs a full re-encode per stop).
3. cpal `StreamTimestamp.device` varies by host; `None` falls back to arrival
   time with `offset_source` marked, or `none` if unmeasurable.
4. Annotation pixels burn in by design; keep the drawing layer chrome-free and
   verify the REC pill behavior in annotated takes is acceptable.
5. Pending-dialog crash recovery needs the on-disk journal; without it,
   quit-with-open-dialog risks orphans.
6. New annotation hotkey must not collide with the record hotkey or tray
   accelerators; exact binding is a product decision (SPEC requires button +
   hotkey + Esc-off only).
7. `Cargo.lock` / `package-lock.json` untouched unless a versioned API proves
   missing; no new heavy deps (canvas 2D + existing Tauri APIs only).
8. Existing public command signatures preserved; all new surface is additive
   (`stop_rec_ex`, `retry_finalize`, `rename_take`, `delete_take`,
   `toggle_annotation`, `set_annotation_tool`, `get_av_offsets`).

## 8. Telemetry (existing `tracing` only)

- Lifecycle: common-start wall time, per-track offsets ms signed + source,
  configured vs actual fps, mux args outcome, validation pass/fail + reason,
  final path or deletion confirm.
- Audio warnings: track (`system`/`mic`), device name, stream errors,
  empty-file events, retry attempts with backoff.
- Annotation: arm/disarm with source, tool/color/thickness selections
  (no stroke content), dropped-frame/stall signals.
- Save dialog: prefilled default, final name or deletion, rejection rule
  (not keystrokes), overwrite confirms, orphan-cleanup results, recovery
  actions. User errors non-technical; logs carry FFmpeg tail / OS error /
  offsets. No keystroke, annotation text, file-content, or extra PII logging.
