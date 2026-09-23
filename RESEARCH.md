# Dr.Record — Technical Research (Features 1–3)

Scope: SPEC.md v1.1.3 (3 features). No code changed. All claims grounded in repo files
listed below or in verified API docs (Tauri 2 docs.rs / plugins-workspace, FFmpeg
ffmpeg-devices/ffprobe docs, cpal rustaudio docs). No new heavy dependencies proposed.

## 1. Capture pipeline today

Video (`src-tauri/src/recorder.rs`):

- `start_recording` (recorder.rs:414) builds args in `build_ffmpeg_args` (recorder.rs:130):
  `-f gdigrab -framerate <fps> [-offset_x/-offset_y -video_size <WxH>] -i desktop
  -c:v libx264 -preset ultrafast -crf <quality_to_crf> -pix_fmt yuv420p <FINAL_video.mp4>`.
  Per-monitor capture uses `resolve_monitor` + `enum_monitors` (negative offsets for
  left/above-primary monitors); `all` captures the full virtual desktop. Matches FFmpeg
  gdigrab docs (offset measured from primary top-left, negative values allowed).
- Binary via `find_ffmpeg` (recorder.rs:178): bundled `resources/ffmpeg/ffmpeg.exe`
  (see `src-tauri/tauri.conf.json` bundle resources), else PATH (`ffmpeg_on_path`).
  Stdin piped so stop sends `q`; stderr goes to `%TEMP%/dr-record-ffmpeg.log` on Windows;
  `CREATE_NO_WINDOW` hides the console. Watchdog thread (recorder.rs:513) polls
  `try_wait` every 500 ms and emits `recording-crashed` + `status-changed=stopped`.
- Temp video is `<FINAL with _video.mp4>`; final path from `generate_output_path`
  (`DrRecord_<Label>_<timestamp>.mp4`, recorder.rs:102).

Audio (`src-tauri/src/audio.rs`, `recorder.rs:26,479-502`):

- Two independent `AudioRecorder`s (system loopback = default output device,
  mic = named input device), started with 4-attempt backoff
  (`start_audio_recorder_with_retry`). `AudioRecorder::start` (audio.rs:24) uses
  `cpal::default_host`, `build_input_stream` (loopback included), converts
  F32/I16/U16 to f32, ships buffers over `mpsc` to `run_wav_writer` (audio.rs:184),
  which writes 32-bit-float WAV via `hound` and **silence-pads on 100 ms recv
  timeouts** to `expected_rate`. Stop (`audio.rs:173`) pauses the stream and joins
  the writer (finalize-before-join covered by `writer_finalizes_before_join_returns`).
- Preview recorders (`start_audio_previews`, recorder.rs:731) run for level meters
  (`audio-level-system/mic` events) and are stopped on record start.

Stop/mux (`stop_recording`, recorder.rs:557; commands in `src-tauri/src/lib.rs`):

- Sends `q`, waits up to 3 s then kills; stops audio writers; runs a **second FFmpeg
  invocation**: 1 audio track = `-c:v copy -c:a aac` (+ `volume=2.0` mic-only);
  2 tracks = `-filter_complex amix normalize=0` with mic boosted; 0 tracks = `-c copy`.
  Then **unconditionally deletes** `_video/_sys/_mic` temps (recorder.rs:712-715).
- Gaps vs SPEC F1/F3: no shared start clock (`start_time: Instant` set after spawns,
  recorder.rs:507; audio writer keeps its own `Instant`, audio.rs:199); per-track offset
  never measured (stream-callback timestamp arg is ignored as `_: &_`, audio.rs:90,108,129);
  mux exit status unchecked (`let _ = mux_proc.wait()`); no output validation
  (0-byte / missing `moov` never checked); missing-track case only `tracing::error`
  (no user warning, silent video-only success possible); no retry path.

Frontend (`src/main.js`, `src/overlay.js`, `overlay.html`, `src-tauri/src/overlay.rs`,
`src-tauri/src/lib.rs`, `src-tauri/src/config.rs`):

- `lib.rs:handle_hotkey` (global-shortcut plugin, `ShortcutState::Pressed`) toggles
  start/stop; `start_rec`/`stop_rec` commands; events `status-changed`,
  `recording-started/stopped`, `recording-error`, `audio-error`, `recording-crashed`.
  Tray menu = Settings/Quit, left-click shows settings; quit while recording stops first.
  `save_config`/`reload_hotkey` preserve hotkey; `Config` (config.rs) holds output dir,
  source, fps, quality, overlay flag, audio flags. Overlay window (overlay.rs:4) is a
  180x44 always-on-top transparent undecorated `focusable(false)` pill bottom-right;
  `overlay.js` polls `get_elapsed_secs` (`get_elapsed` over `start_time`).
  Settings UI (`main.js`) handles sources (device-id keyed, legacy index migration),
  thumbnails (`get_thumbnail` via xcap `capture_image`), mics, levels, notifications.
- `tauri-plugin-dialog` is already registered (`lib.rs:257`) and `open()` used for the
  output-dir picker; `save()`/`ask()` from the same plugin verified available.

## 2. A/V sync fix options (SPEC F1)

### Option A — Common start clock + measured per-track offsets (do this)

- Mechanism: record one `common_start: Instant` (or `SystemTime` for log wall-clock)
  in `start_recording` **before** spawning FFmpeg/audio; store on `RecorderState`.
  Derive overlay elapsed (`get_elapsed`) and stop metadata from it.
- Measure each track's first-sample offset with cpal's callback timestamp: change the
  data callbacks in `audio.rs` to accept the second `&InputCallbackInfo`/`CallbackInfo`
  arg and read `info.timestamp()` (`StreamTimestamp { device, callback }`; verified in
  rustaudio/cpal docs + WASAPI `input_timestamp`: `device` = instant the first sample
  of the buffer was captured). Offset = `first_device_timestamp - common_start`
  (ms, signed; positive = audio late, per SPEC). Log via existing `tracing` pipeline
  and include in the stop result. WASAPI/QPC-backed on Windows; treat missing
  timestamps as `None` (fall back to arrival-time measurement, still signed ms).
- Pros: tiny diff (audio.rs callback signatures + `RecorderState` fields + logging);
  no deps; directly satisfies SPEC F1.1/F1.2 + telemetry; sign convention documented once.
- Cons: does not itself fix drift; relies on cpal timestamp availability per host
  (mitigate with fallback + `None` handling).
- Recommendation: **adopt**. It is the only mechanism that yields the SPEC-required
  observable offsets without new deps.

### Option B — Mux-time alignment: constant offset vs drift-rate (do this, correctly split)

Verified FFmpeg semantics (ffmpeg-cookbook 2026, FFmpeg 8.x notes):

| Symptom | Fix | Re-encode? |
|---|---|---|
| Constant offset (same ms start-to-end) | `-itsoffset <s>` on one `-i` + `-c copy` (either direction), or `adelay=<ms>\|<ms>` for audio-ahead only | itsoffset: none; adelay: audio only |
| Drift (grows with duration, incl. >10 min) | `-af aresample=async=<N>` (fill/trim/stretch, `async=1000` start) ± `-ar 48000`; VFR video source: `-fps_mode cfr -r <fps>` (re-encodes video) | audio yes; CFR video yes |
| Missing/non-monotonic PTS | `-fflags +genpts` before `-i` + `-c copy` | no |

- `-itsoffset` **cannot** fix drift (slides the whole stream as a rigid block); `adelay`
  only delays audio (cannot advance it); `-async 1` is coarse/deprecated in favor of
  `aresample=async`. `-ar 48000` alone standardizes rate but does not correct drift.
- Fits this repo: the mux already re-encodes audio to `aac`, so `adelay`/`aresample`
  cost nothing extra; `-itsoffset` needs only arg construction. gdigrab real-time
  capture + `-c copy` video mux is the classic VFR-timestamp-propagation risk, so keep
  CFR re-encode as a fallback if long-recording drift is measured, not the default
  (it costs a full video re-encode per stop).
- Pros: uses bundled FFmpeg only; composes with existing `amix` graph
  (`adelay`/`aresample` per-input before `amix`, or single `-af` for one track).
- Cons: needs the measured offsets from Option A to pick direction/magnitude;
  over-applying CFR re-encode slows every stop.
- Recommendation: **adopt**: measured offset → `-itsoffset` (either direction) or
  `adelay` (audio-ahead) at mux; add `aresample=async=1…1000` when drift detected or
  proactively with the two-track graph; reserve `-fps_mode cfr` for confirmed
  VFR-drift cases. This satisfies SPEC F1.3 with outcome-based choice, mechanism free.

### Option C — Validation + graceful error + retry + no-silent-drop (do this)

- Validation tiers, cheapest first (all with bundled `ffmpeg.exe` via `find_ffmpeg`;
  `ffprobe` is **not** in `tauri.conf.json` bundle resources, so treat it as optional):
  1. file exists + size > 0; 2. `ffprobe -v error -show_streams -print_format json`
     (if present; nonzero exit / `moov atom not found` / `Invalid data found` = fail);
  3. decode check `ffmpeg -v error -i <final> -f null -` (catches truncated-moov that
     probes green); 4. enabled-track audit: requested vs muxed inputs, zero-length WAV
     (`_sys.wav`/`_mic.wav` data-chunk size) → named warning (`system`/`mic` + reason:
     device lost / stream error / empty file), never silent.
- On failure: keep temps (`_video/_sys/_mic` + ffmpeg log tail), emit non-technical
  user error + `Retry` action that re-runs only the mux/validate step from retained
  artifacts (no re-record); surface mux exit code + log tail in `tracing`, not UI.
  Double-stop / start-during-mux guarded by `is_recording` + a `finalizing` flag so
  exactly one outcome occurs.
- Pros: no deps; satisfies SPEC F1.4–F1.6 + edge cases (0-byte, missing temp,
  crash-mid-record, unwritable dir, missing validator binary → defined error path).
- Cons: full-decode validation adds seconds on long files (mitigate: run fast tiers
  first, decode-check gated by size/duration or a `--full-validate` flag; still faster
  than a corrupt delivery).
- Recommendation: **adopt** tiers 1+3 as default (bundled ffmpeg only), tier 2 when
  ffprobe resolvable; retain-temps-until-valid + retry-from-artifacts.

### F1 files/functions to touch

- `src-tauri/src/recorder.rs`: `RecorderState` (+ `common_start`, per-track offsets,
  `finalizing` flag, retained-artifact paths), `start_recording`, `stop_recording`
  (split into `finalize_take` reusable by retry), mux-arg builder, new
  `validate_output` + `retry_finalize` commands.
- `src-tauri/src/audio.rs`: `AudioRecorder::start` callbacks (use timestamp arg),
  `AudioRecorder` (+ `first_sample_offset_ms`), `run_wav_writer` (report empty-file
  reason), existing retry helper reused for retry logging.
- `src-tauri/src/lib.rs`: `handle_hotkey` (start-during-mux guard), `stop_rec`
  (return metadata: offsets, validation, warnings), new `retry_save` command, event
  payloads (`recording-stopped` carries metadata; new `save-warning`/`save-error`).
- `src/main.js`: render offsets/warnings/retry UI in notifications; `src/overlay.js`:
  elapsed from common start (no behavior change, same command).

## 3. Annotation options for Tauri (SPEC F2)

Key constraint: SPEC requires **burn-in** (marks are pixels in the video, visible in any
player, no sidecar). Since capture is FFmpeg `gdigrab` reading screen pixels, any overlay
whose pixels appear on screen inside the capture region is burned in automatically.
That decides the design space.

### Option A — Fullscreen transparent Tauri overlay + canvas 2D, click-through toggle (recommended)

- Mechanism: new fullscreen overlay window (`annotation`, per target monitor) built in
  `overlay.rs` beside the existing pill: `transparent(true)`, `decorations(false)`,
  `always_on_top(true)`, `skip_taskbar(true)`, `fullscreen` (or monitor-sized +
  positioned at the monitor origin via existing `monitor_position` pattern).
  Drawing surface = plain `<canvas>` 2D in the new window (no library).
  Arm/disarm flips `set_ignore_cursor_events`: verified Tauri 2 API
  (`Window::set_ignore_cursor_events(bool)`, desktop-only, docs.rs; JS
  `getCurrentWindow().setIgnoreCursorEvents()`): disarmed = ignore → passthrough to
  windows below; armed = receive events → draw. Toggle via overlay button **and**
  global-shortcut hotkey (existing `tauri-plugin-global-shortcut` handler pattern in
  `lib.rs:257-265`) **and** Esc (frontend keydown); toggling never touches
  `RecorderState::is_recording`. Tools (pen/highlighter/arrow/text/eraser/clear-all),
  ≥4 colors, ≥3 thicknesses as JS canvas state; eraser = destination-out on the canvas
  (subsequent frames clean; already-recorded frames keep marks, per SPEC).
- Multi-monitor/DPR: size/position the window from `enum_monitors`/`resolve_monitor`
  (negative offsets already handled, recorder.rs:254); scale canvas by the monitor
  `scale_factor` (same `size/scale` math as `monitor_position`, overlay.rs:94) and
  `devicePixelRatio` so cursor↔mark coincide; clip strokes to the recorded region.
- Pros: zero new deps (canvas 2D + existing Tauri window APIs); reuses `overlay.rs`
  commands, `enum_monitors`/`resolve_monitor`, event system, hotkey/tray untouched;
  burn-in free via gdigrab; immediate mode switch; per-frame cost is one composited
  transparent layer + canvas strokes (rAF-throttled), well under one frame at 30 FPS
  on reference hardware.
- Cons: must keep the overlay chrome-free (any pill/border would also burn in —
  keep drawing layer pixel-clean); `focusable(false)` + cursor-event toggling needs
  care so hotkeys still fire while disarmed (global shortcut is OS-level, unaffected);
  high-DPR mapping must be tested per monitor.
- Recommendation: **adopt A**. It is the only option meeting burn-in + no-heavy-deps
  + hotkey/tray preservation simultaneously.

### Option B — Native OS draw layer / separate capture injection (rejected)

- E.g. custom Win32 layered-window drawing in Rust, or feeding an overlay input to
  FFmpeg. Pros: marginally lower compositor overhead. Cons: new native code paths,
  new failure modes (focus, scaling, multi-monitor), duplicates what the Tauri window
  already provides; FFmpeg-side overlay is post-process, not live. Rejected: violates
  simplicity + no-new-deps constraints for no SPEC benefit.

### Option C — Post-record vector overlay / re-encode burn-in (rejected)

- Draw after stop (e.g. SVG layer re-encoded with filters). Pros: undo-friendly.
  Cons: marks would not appear "during recording" at the right time without a
  timestamped stroke log + re-encode pass; contradicts SPEC burn-in-during-capture,
  adds mux complexity and stop latency. Rejected (also conflicts with non-goal: no
  persistent vector layers).

### F2 files/functions to touch

- `src-tauri/src/overlay.rs`: `create_annotation_window(monitor)`,
  `close_annotation_window`, `set_annotation_clickthrough` (wraps
  `set_ignore_cursor_events`), monitor-origin placement.
- `src-tauri/src/lib.rs`: `toggle_annotation` / `set_annotation_tool` commands,
  extra global-shortcut registration (annotation hotkey; existing hotkey untouched),
  `annotation-state` events; logging arm/disarm source (button/hotkey/Esc) + tool
  selections (no stroke content, per SPEC telemetry).
- `overlay.html` (or new `annotation.html`): fullscreen transparent canvas layer +
  minimal toolbar (tools/colors/thickness/clear), armed/disarmed indicator, disabled
  state when not recording.
- `src/overlay.js` (or new `src/annotation.js`): canvas tools, DPR scaling, Esc/button/
  hotkey handling, `setIgnoreCursorEvents` calls, dropped-frame observation hooks.
- `src-tauri/src/recorder.rs`: no capture change required (burn-in is automatic);
  optionally log configured-vs-actual fps alongside annotation sessions.

## 4. Save-dialog options (SPEC F3)

### Option A — Custom post-stop modal in the app webview (recommended)

- Mechanism: after `stop_recording` + validation succeeds (or fails with retained
  artifacts), emit `recording-stopped` with `{ default_name, final_path, warning? }`
  and show a modal (settings window or a small dedicated `save-dialog` webview):
  editable field prefilled `DrRecord_<Label>_<timestamp>`, `Save` + `Delete Locally`
  + defined Cancel semantics. Frontend validates inline; backend exposes
  `rename_take { id, new_name }` (`std::fs::rename` in configured output dir) and
  `delete_take { id }` (removes final + `_video/_sys/_mic` + take log entry).
- Validation (backend, authoritative): reject empty/whitespace-only, Windows-illegal
  `< > : " / \ | ? *` + control chars, reserved `CON PRN AUX NUL COM1-9 LPT1-9`
  (case-insensitive), trailing dot/space, overlong (component/filesystem limits —
  enforce conservatively + surface OS error on rename attempt); collision → explicit
  overwrite confirm, never silent. Invalid input keeps dialog open with inline error.
- Idempotency/concurrency: single-take token (`take_id`); `Save`/`Delete` guarded so
  double-click executes once; hotkey start while dialog open is queued/gated until
  resolution (check `pending_dialog` flag in `handle_hotkey`/`start_rec`); app-quit
  with pending dialog → journal recovery (see Risks).
- Pros: full SPEC control (prefill, inline errors, Save + Delete Locally + overwrite
  confirm + Enter/Esc semantics in one modal); no new deps; reuses dialog-plugin
  host + existing events; native `save()` cannot host a Delete action or inline
  rule messaging.
- Cons: must manage window focus/show (settings window is hide-on-close today,
  overlay.rs:83) and gate recording commands while open.
- Recommendation: **adopt A** (dedicated `save-dialog` webview preferred over
  reusing settings, to avoid coupling save flow to settings visibility).

### Option B — Native `save()` file dialog via tauri-plugin-dialog (rejected as primary)

- Verified: `save(options)` returns `string | null` path only (plugins-workspace
  `guest-js/index.ts`); `ask()` gives confirm/overwrite primitives. Pros: native
  look, free overwrite prompt, no custom window. Cons: cannot combine editable
  prefill + inline rule errors + `Delete Locally` in one modal (SPEC F3.1/F3.2
  impossible in a single native call); behavior OS-dependent; still needs a second
  custom confirm for delete. Usable only as fallback. Rejected as primary; keep
  `open()` (dir picker) and `ask()` (delete/overwrite confirms) as supporting calls.

### Option C — Auto-save default + background toast (rejected)

- Pros: simplest. Cons: directly violates SPEC F3.1 (modal with Save/Delete after
  every normal stop). Rejected.

### F3 files/functions to touch

- `src-tauri/src/recorder.rs`: `generate_output_path` (default-name source),
  retained-artifact bookkeeping, `rename_take`/`delete_take`/`resolve_pending_take`
  (+ startup orphan-sweep + crash-journal write/clear).
- `src-tauri/src/lib.rs`: `stop_rec` (return take metadata, not just path),
  new commands `rename_take`, `delete_take`, `discard_take`, `get_pending_take`;
  `handle_hotkey` + `start_rec` gate on `pending_dialog`; tray-quit path resolves or
  journals the pending take first.
- Frontend: new modal markup/logic (in `index.html` or new `save-dialog.html` +
  `src/save-dialog.js`) or `src/main.js` extension: prefill, inline errors,
  Enter=Save/Esc=cancel semantics, single-execution guards, overwrite `ask()`
  confirm; `src/overlay.js` untouched except status flow already exists.

## 5. Test strategy

- `cargo test` (existing, must keep green): `recorder.rs` tests (`resolve_monitor`,
  `build_ffmpeg_args` incl. negative offsets), `audio.rs` (`writer_finalizes…`,
  loopback capture). New unit tests: filename sanitizer (illegal/reserved/overlong/
  collision/same-name), offset-sign math, mux-arg selection (itsoffset vs adelay vs
  aresample graph), validation-result mapping, idempotency guards.
- `ffprobe`/FFmpeg checks (scripted, e.g. extend `tests/Run-Tests.ps1` + `TEST_PLAN.md`):
  `ffprobe -v error -show_streams -print_format json` (streams/duration), start_time
  delta audio-vs-video vs logged offsets, `ffmpeg -v error -i <final> -f null -`
  full-decode pass, 0-byte/moov-less fixtures must fail validation; 60 s
  speech-over-clicks sample for drift; long (>10 min or accelerated) soak for drift
  growth; annotation session: extract frames (`-ss -frames:v 1`) with/without marks +
  monitor-offset/DPR matrix; save-dialog: double-Save/double-Delete single-execution,
  illegal-name battery, collision-overwrite path, quit-with-pending recovery, orphan
  sweep assert (no `_video/_sys/_mic` remain).
- Manual gates: stock-player playback (both audio tracks audible, marks at right
  time/position, no sidecar), hotkey/tray/settings regression, Esc/button/hotkey
  annotation toggles without recording interruption.

## 6. Risks / assumptions

1. `ffprobe` bundling unverified: `tauri.conf.json` ships only `ffmpeg.exe`; research
   assumes validation defaults to file checks + bundled-ffmpeg null-decode, ffprobe
   optional. Confirm before relying on tier-2 probes.
2. gdigrab frame timing is capture-clock, not CFR-guaranteed; `-c copy` mux
   propagates source PTS (verified `-c copy` timestamp-carryover behavior), hence the
   drift-rate fallback in Option B.
3. cpal `StreamTimestamp.device` availability varies by host/backend; code must handle
   `None`/unavailable timestamps (arrival-time fallback) or offsets are unmeasurable
   on that machine.
4. Annotation overlay pixels are captured by gdigrab by design (required for burn-in);
   keep the drawing layer chrome-free and verify the existing REC pill behavior is
   unchanged/acceptable in annotated takes.
5. Pending-dialog crash recovery needs a small on-disk journal (take_id, default path,
   artifact paths, status); without it, quit-with-open-dialog risks orphans (SPEC F3.5).
6. New global shortcut for annotation toggle must not collide with the existing
   record hotkey or tray accelerators; exact keybinding is a product decision left to
   implementation (SPEC only requires button + hotkey + Esc-off).
7. No new heavy deps introduced by any recommendation (canvas 2D, bundled FFmpeg,
   existing `cpal`/`hound`/`xcap`/dialog plugin only); `Cargo.lock`/`package-lock`
   untouched unless a versioned API proves missing at implementation time.
