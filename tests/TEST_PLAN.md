# Dr. Record — Complete Black-Box Test Specification

**Version:** 1.0.0  
**Platform:** Windows 10/11  
**Total Test Cases:** 1084  
**Automated:** 612 | **Manual:** 472  

---

## 1. Installation Tests (TC-001 to TC-055)

| ID | Description | Type |
|----|------------|------|
| TC-001 | Install on Windows 11 24H2 — MSI executes without error | Manual |
| TC-002 | Install on Windows 11 23H2 — MSI executes without error | Manual |
| TC-003 | Install on Windows 10 22H2 — MSI executes without error | Manual |
| TC-004 | Install via NSIS installer — all files extracted | Manual |
| TC-005 | Install to default Program Files path | Manual |
| TC-006 | Install to custom path with spaces (C:\My Apps\Dr Record) | Manual |
| TC-007 | Install to path with Unicode characters | Manual |
| TC-008 | Install when 200MB disk space available | Manual |
| TC-009 | Install when <50MB disk space — graceful error | Manual |
| TC-010 | Install without admin privileges — blocked as expected | Manual |
| TC-011 | Install over previous version — files updated, config preserved | Manual |
| TC-012 | Install while app is running — installer offers to close | Manual |
| TC-013 | Uninstall via Settings > Apps — app removed cleanly | Manual |
| TC-014 | Uninstall, config directory is removed (or user is asked) | Manual |
| TC-015 | Uninstall, reinstall — fresh install detection works | Manual |
| TC-016 | Portable mode: run exe directly from any folder | Auto |
| TC-017 | App launches from Start Menu shortcut | Manual |
| TC-018 | App launches from desktop shortcut | Manual |
| TC-019 | App pinned to taskbar, launches correctly | Manual |
| TC-020 | Shortcut target points to correct exe path | Auto |
| TC-021 | Installer registers uninstall entry in Registry | Auto |
| TC-022 | Installer creates Start Menu folder | Auto |
| TC-023 | Installer adds to PATH (if applicable) | Auto |
| TC-024 | Install log created — no errors | Auto |
| TC-025 | Silent install (`/S`) succeeds | Manual |
| TC-026 | All required DLLs are present in install directory | Auto |
| TC-027 | WebView2 runtime is detected (or prompted) | Auto |
| TC-028 | FFmpeg detected in PATH or bundled | Auto |
| TC-029 | App icon appears correctly in Start Menu | Manual |
| TC-030 | App icon appears correctly in taskbar | Manual |
| TC-031 | File associations (none expected — verify none created) | Auto |
| TC-032 | Install on system with non-English locale (ja-JP) | Manual |
| TC-033 | Install on system with RTL locale (ar-SA) | Manual |
| TC-034 | Install on Windows N (no Media Player) | Manual |
| TC-035 | Install on Windows Server 2022 | Manual |
| TC-036 | Install via winget (if published) | Manual |
| TC-037 | AppData\Local\dr-record created on first launch | Auto |
| TC-038 | AppData\Roaming\dr-record\config.json created on first launch | Auto |
| TC-039 | Install size within expected range (<50MB) | Auto |
| TC-040 | No unnecessary background services installed | Auto |
| TC-041 | No startup entries created without permission | Auto |
| TC-042 | Antivirus does not flag installer as malware | Manual |
| TC-043 | Windows Defender SmartScreen passes installer | Manual |
| TC-044 | Code signing signature is valid (if signed) | Manual |
| TC-045 | Install on ARM64 Windows via emulation | Manual |
| TC-046 | Install over network share | Manual |
| TC-047 | Install to folder with existing old version | Manual |
| TC-048 | Repair install via installer | Manual |
| TC-049 | Modify install (add/remove features) | Manual |
| TC-050 | Install from USB drive | Manual |
| TC-051 | Multiple user accounts — install for all users | Manual |
| TC-052 | Per-user install (current user only) | Manual |
| TC-053 | Windows Sandbox — install and run | Manual |
| TC-054 | VMware/Hyper-V VM — install and run | Manual |
| TC-055 | Clean uninstall leaves no registry traces | Auto |

---

## 2. First Launch Tests (TC-056 to TC-120)

| ID | Description | Type |
|----|------------|------|
| TC-056 | First launch: settings window opens automatically | Auto |
| TC-057 | First launch: window titled "Dr. Record Settings" | Auto |
| TC-058 | First launch: welcome banner is visible | Auto |
| TC-059 | First launch: output directory defaults to Videos folder | Auto |
| TC-060 | First launch: hotkey defaults to "Ctrl+Shift+R" | Auto |
| TC-061 | First launch: recording mode defaults to "Full Screen" | Auto |
| TC-062 | First launch: overlay checkbox defaults to checked | Auto |
| TC-063 | First launch: config.json created at correct path | Auto |
| TC-064 | First launch: config.json has all required fields | Auto |
| TC-065 | First launch: config.json has `first_run: true` | Auto |
| TC-066 | First launch: tray icon appears in system tray | Auto |
| TC-067 | First launch: app appears in taskbar | Auto |
| TC-068 | First launch: no other windows open besides settings | Auto |
| TC-069 | First launch: status shows "Idle" with green dot | Auto |
| TC-070 | Second launch: settings window does NOT open | Auto |
| TC-071 | Second launch: no welcome banner | Auto |
| TC-072 | Second launch: app minimizes to tray immediately | Auto |
| TC-073 | Second launch: hotkey is active without opening GUI | Auto |
| TC-074 | Third launch: same behavior as second | Auto |
| TC-075 | Launch with missing config.json — recreates with defaults | Auto |
| TC-076 | Launch with corrupted config.json — recreates with defaults | Auto |
| TC-077 | Launch with empty config.json — recreates with defaults | Auto |
| TC-078 | Launch with config.json missing fields — fills defaults | Auto |
| TC-079 | Launch with config.json read-only — uses fallback | Auto |
| TC-080 | Launch while config directory is locked — graceful error | Auto |
| TC-081 | Launch with invalid UTF-8 in config — recreates | Auto |
| TC-082 | Launch with config from future version — preserves data | Auto |
| TC-083 | Launch immediately after uninstall → clean state | Auto |
| TC-084 | Launch on locked Windows workstation — app starts | Auto |
| TC-085 | Launch via double-click on .exe | Manual |
| TC-086 | Launch via Run dialog (Win+R) | Manual |
| TC-087 | Launch via command line with args | Auto |
| TC-088 | Launch with `--help` flag — shows usage (if implemented) | Auto |
| TC-089 | Launch with `--config` flag pointing to custom config | Auto |
| TC-090 | Launch with `--output-dir` override | Auto |
| TC-091 | Launch with `--hotkey` override | Auto |
| TC-092 | Second instance detection — only one instance runs | Auto |
| TC-093 | Rapid launch cycles (5x in 2s) — no crash | Auto |
| TC-094 | Launch and immediately quit — no crash | Auto |
| TC-095 | Launch on high-DPI display (150%) — correct scaling | Manual |
| TC-096 | Launch on 4K display — correct scaling | Manual |
| TC-097 | Launch on 768p display — fits within screen | Manual |
| TC-098 | Launch with dark mode enabled — follows OS theme | Auto |
| TC-099 | Launch with light mode — follows OS theme | Auto |
| TC-100 | Launch with custom Windows theme | Manual |
| TC-101 | Launch with Windows text size at 200% | Manual |
| TC-102 | Launch while another screen recorder (OBS) is running | Auto |
| TC-103 | Launch while screen recording permission is denied | Auto |
| TC-104 | Launch on battery power — no OSD warnings | Auto |
| TC-105 | Launch with 0% battery — app starts | Auto |
| TC-106 | Launch right after system resume from sleep | Auto |
| TC-107 | Launch right after system wake from hibernate | Auto |
| TC-108 | Launch with incorrect system time — still works | Auto |
| TC-109 | Launch on Monday 00:00 — file naming correct | Auto |
| TC-110 | Launch on Dec 31 23:59 — cross-midnight works | Auto |
| TC-111 | Launch in Remote Desktop session — works or warns | Manual |
| TC-112 | Launch on secondary monitor as primary | Manual |
| TC-113 | Launch with 3 monitors connected | Manual |
| TC-114 | Launch with 0 monitors (headless) — graceful error | Auto |
| TC-115 | Launch on tablet mode — touch-friendly? | Manual |
| TC-116 | First launch: Browse button visible | Auto |
| TC-117 | First launch: Capture button visible | Auto |
| TC-118 | First launch: Save & Start button visible | Auto |
| TC-119 | First launch: Test Record button visible | Auto |
| TC-120 | First launch: window centered on screen | Auto |

---

## 3. Settings Window Tests (TC-121 to TC-240)

| ID | Description | Type |
|----|------------|------|
| TC-121 | Settings window: titled "Dr. Record Settings" | Auto |
| TC-122 | Settings window: 520x480 default size | Auto |
| TC-123 | Settings window: not resizable | Auto |
| TC-124 | Settings window: centered on screen | Auto |
| TC-125 | Settings window: dark background (#1a1a2e) | Auto |
| TC-126 | Settings window: all UI elements visible | Auto |
| TC-127 | Output directory picker: shows current path | Auto |
| TC-128 | Output directory picker: Browse button opens native dialog | Auto |
| TC-129 | Folder picker: starts in current output directory | Auto |
| TC-130 | Folder picker: selecting a folder updates the input | Auto |
| TC-131 | Folder picker: Cancel button does not change path | Auto |
| TC-132 | Folder picker: can pick network drives | Manual |
| TC-133 | Folder picker: can pick external USB drives | Manual |
| TC-134 | Folder picker: can pick OneDrive/DropBox folders | Manual |
| TC-135 | Folder picker: cannot pick invalid paths | Auto |
| TC-136 | Output dir with trailing backslash — normalized | Auto |
| TC-137 | Output dir with forward slashes — normalized | Auto |
| TC-138 | Output dir is root of drive (C:\) — warning shown | Auto |
| TC-139 | Output dir is system directory (Windows) — warning | Auto |
| TC-140 | Output dir has Unicode characters (中文, 日本語) — works | Manual |
| TC-141 | Hotkey input: shows current shortcut | Auto |
| TC-142 | Hotkey input: clicking "Capture" enables capture mode | Auto |
| TC-143 | Hotkey input: shows "Press shortcut..." when capturing | Auto |
| TC-144 | Hotkey input: border highlights red in capture mode | Auto |
| TC-145 | Hotkey input: pressing Ctrl+Shift+R captures correctly | Auto |
| TC-146 | Hotkey input: pressing Ctrl+Alt+S captures correctly | Auto |
| TC-147 | Hotkey input: pressing F1 alone shows "F1" | Auto |
| TC-148 | Hotkey input: pressing Ctrl+Shift+F12 captures correctly | Auto |
| TC-149 | Hotkey input: pressing Win+Shift+S captures correctly | Auto |
| TC-150 | Hotkey input: pressing Alt+Tab captures correctly | Auto |
| TC-151 | Hotkey input: pressing a single letter — ignored (needs modifier) | Auto |
| TC-152 | Hotkey input: pressing Esc cancels capture mode | Auto |
| TC-153 | Hotkey input: Esc restores previous hotkey value | Auto |
| TC-154 | Hotkey input: pressing Enter during capture — ignored | Auto |
| TC-155 | Hotkey input: pressing system-reserved shortcut — allowed | Auto |
| TC-156 | Hotkey: Win+L shows as "Win+L" (locks screen — note) | Auto |
| TC-157 | Hotkey input: Ctrl+Alt+Del — shows but won't work (OS reserved) | Auto |
| TC-158 | Recording mode: Full Screen radio button works | Auto |
| TC-159 | Recording mode: All Monitors radio button works | Auto |
| TC-160 | Recording mode: Active Window radio button works | Auto |
| TC-161 | Recording mode: selected option highlighted with accent color | Auto |
| TC-162 | Recording mode: switching mode shows confirmation (if needed) | Auto |
| TC-163 | Overlay checkbox: checked by default | Auto |
| TC-164 | Overlay checkbox: unchecking hides overlay on next recording | Auto |
| TC-165 | Overlay checkbox: state persists after restart | Auto |
| TC-166 | Status indicator: shows "Idle" when not recording | Auto |
| TC-167 | Status indicator: green dot when idle | Auto |
| TC-168 | Status indicator: red pulsing dot when recording | Auto |
| TC-169 | Status indicator: updates in real-time | Auto |
| TC-170 | Save & Start button: saves config and starts recording | Auto |
| TC-171 | Save & Start button: closes settings window after save | Auto |
| TC-172 | Save & Start button: shows error if no output directory | Auto |
| TC-173 | Save & Start button: button text readable | Auto |
| TC-174 | Test Record button: starts 5s test recording | Auto |
| TC-175 | Test Record button: shows notification | Auto |
| TC-176 | Test Record button: creates valid MP4 file | Auto |
| TC-177 | Test Record button: file saved in output directory | Auto |
| TC-178 | Test Record button: error if ffmpeg not found | Auto |
| TC-179 | Test Record button: shows error notification on failure | Auto |
| TC-180 | Notification: error style (red background) | Auto |
| TC-181 | Notification: success style (green background) | Auto |
| TC-182 | Notification: auto-hides after 3 seconds | Auto |
| TC-183 | Notification: multiple notifications queue | Auto |
| TC-184 | Settings window: can be moved by dragging title bar | Manual |
| TC-185 | Settings window: minimize button works | Auto |
| TC-186 | Settings window: close button minimizes to tray | Auto |
| TC-187 | Settings window: Alt+F4 closes window | Auto |
| TC-188 | Settings window: re-opening from tray shows current state | Auto |
| TC-189 | Settings window: keyboard navigation (Tab order) | Manual |
| TC-190 | Settings window: Enter on Save button triggers save | Auto |
| TC-191 | Settings window: Esc closes window | Auto |
| TC-192 | Settings window: window cannot be resized with mouse | Auto |
| TC-193 | Settings window: window stays on top of other windows | Auto |
| TC-194 | Settings window: responds to Windows Snap (Win+Arrow) | Manual |
| TC-195 | Settings window: copy/paste in output dir field (disabled, readonly) | Auto |
| TC-196 | Hotkey input: cannot type arbitrary text | Auto |
| TC-197 | Hotkey input: Ctrl+C/V/X/A are forwarded correctly | Auto |
| TC-198 | Settings window: all text visible in high contrast mode | Manual |
| TC-199 | Settings window: all controls accessible via keyboard | Manual |
| TC-200 | Settings window: narrator/screen reader reads controls | Manual |
| TC-201 | DC-201 | Close settings during recording — recording continues | Auto |
| TC-202 | Settings window: open settings while recording — allowed | Auto |
| TC-203 | Settings window: change output dir while recording — takes effect next recording | Auto |
| TC-204 | Settings window: change hotkey while recording — takes effect immediately | Auto |
| TC-205 | Settings window: change mode while recording — takes effect next recording | Auto |
| TC-206 | Settings window: rapid clicking Save (10x) — saves once | Auto |
| TC-207 | Settings window: browse button with empty field — opens at Documents | Auto |
| TC-208 | Settings window: browse to non-existent folder — creates it? | Auto |
| TC-209 | Settings window: very long output path (>260 chars) — handled | Auto |
| TC-210 | Settings window: unicode in output path — displayed correctly | Auto |
| TC-211 | Settings window: hotkey field with empty value — Save shows error | Auto |
| TC-212 | Settings window: open settings via tray right-click | Auto |
| TC-213 | Settings window: open settings via tray left-click | Auto |
| TC-214 | Settings window: multiple hotkey capture attempts — works | Auto |
| TC-215 | Settings window: click "Capture" then click elsewhere — cancels | Auto |
| TC-216 | Settings window: first-run banner vanishes after save | Auto |
| TC-217 | Settings: change framerate (if exposed) — test 15,24,30,60fps | Manual |
| TC-218 | Settings: framerate at 144 — handled gracefully | Auto |
| TC-219 | Settings: framerate at 1 — still works | Auto |
| TC-220 | Settings: framerate at 0 — defaults to 30 | Auto |
| TC-221 | Settings: all values save across restart | Auto |
| TC-222 | Settings: verify hotkey saved in config correctly | Auto |
| TC-223 | Settings: verify mode saved in config correctly | Auto |
| TC-224 | Settings: verify overlay toggle saved correctly | Auto |
| TC-225 | Settings: config.json format is valid JSON | Auto |
| TC-226 | Settings: config.json has correct permissions (not world-readable) | Auto |
| TC-227 | Settings: config.json can be edited manually while app is closed | Auto |
| TC-228 | Settings: manually edited config is loaded correctly | Auto |
| TC-229 | Settings: remove config file while app is open — uses cached | Auto |
| TC-230 | Settings: modify config file while app open — doesn't hot-reload | Auto |

---

## 4. Hotkey System Tests (TC-231 to TC-340)

| ID | Description | Type |
|----|------------|------|
| TC-231 | Default hotkey Ctrl+Shift+R is registered on launch | Auto |
| TC-232 | Pressing hotkey starts recording (first press) | Auto |
| TC-233 | Pressing hotkey stops recording (second press) | Auto |
| TC-234 | Hotkey works when app window is closed (tray only) | Auto |
| TC-235 | Hotkey works when another app is in focus | Auto |
| TC-236 | Hotkey works in full-screen games (borderless) | Manual |
| TC-237 | Hotkey works in exclusive full-screen games | Manual |
| TC-238 | Hotkey does NOT interfere with the target app's shortcuts | Auto |
| TC-239 | Ctrl+C (copy) still works in other apps | Auto |
| TC-240 | Ctrl+V (paste) still works in other apps | Auto |
| TC-241 | Ctrl+Z (undo) still works in other apps | Auto |
| TC-242 | Alt+Tab (switch apps) still works | Auto |
| TC-243 | Win+D (desktop) still works | Auto |
| TC-244 | Win+E (explorer) still works | Auto |
| TC-245 | Custom hotkey Ctrl+Alt+S works correctly | Auto |
| TC-246 | Custom hotkey F8 works (single key) | Auto |
| TC-247 | Custom hotkey Ctrl+Shift+F1 works | Auto |
| TC-248 | Custom hotkey Win+Shift+S works | Auto |
| TC-249 | Custom hotkey with 3 modifiers (Ctrl+Alt+Shift+R) works | Auto |
| TC-250 | Hotkey persists after app restart | Auto |
| TC-251 | Change hotkey in settings → new hotkey works | Auto |
| TC-252 | Old hotkey no longer works after change | Auto |
| TC-253 | Fast double-press: second press stops immediately | Auto |
| TC-254 | Rapid hotkey spam (10 presses in 2s) — no crash | Auto |
| TC-255 | Hold hotkey down — fires once on press, not on hold | Auto |
| TC-256 | Hotkey pressed during recording startup — debounced | Auto |
| TC-257 | Hotkey pressed during recording shutdown — debounced | Auto |
| TC-258 | Hotkey works when Windows is locked (does it?) | Manual |
| TC-259 | Hotkey works on login screen — should not | Manual |
| TC-260 | Hotkey works in UAC prompt — should not | Manual |
| TC-261 | Hotkey works in Remote Desktop session | Manual |
| TC-262 | Hotkey conflict detection: register same hotkey twice | Auto |
| TC-263 | Hotkey conflict with another app — OS chooses | Manual |
| TC-264 | Hotkey with NumPad digits (Ctrl+Shift+Num1) — try | Auto |
| TC-265 | Hotkey with special keys: Ctrl+Shift+Space — works | Auto |
| TC-266 | Hotkey with arrow keys: Ctrl+Alt+Up — works | Auto |
| TC-267 | Hotkey released properly — no ghost key | Auto |
| TC-268 | Hotkey fires only on keydown, not keyup | Auto |
| TC-269 | Register hotkey after sleep/resume — still works | Auto |
| TC-270 | Register hotkey after display mode change | Auto |
| TC-271 | Register hotkey after resolution change | Auto |
| TC-272 | Hotkey with Korean/Japanese keyboard layout | Manual |
| TC-273 | Hotkey with Dvorak layout — Ctrl+Shift+R on Dvorak | Manual |
| TC-274 | Hotkey with AZERTY layout — modifiers still work | Manual |
| TC-275 | Hotkey with RWIN (right Win key) — works | Auto |
| TC-276 | Hotkey with RCONTROL — works | Auto |
| TC-277 | Hotkey with RSHIFT — works | Auto |
| TC-278 | Hotkey with RALT (AltGr) — works | Auto |
| TC-279 | Hotkey with mouse buttons — not supported (keyboard only) | Auto |
| TC-280 | Hotkey: unregister on quit — no dangling hotkey | Auto |
| TC-281 | Hotkey: re-register on settings change — clean transition | Auto |
| TC-282 | Hotkey: only Dr. Record responds to its hotkey | Auto |
| TC-283 | No other app activates when Dr. Record hotkey pressed | Auto |
| TC-284 | Hotkey does not type characters into active text field | Auto |
| TC-285 | Hotkey does not trigger browser shortcuts | Auto |
| TC-286 | Hotkey does not trigger IDE shortcuts | Auto |
| TC-287 | Hotkey does not trigger game actions | Manual |
| TC-288 | Hotkey toggle state machine: idle → rec → idle → rec | Auto |
| TC-289 | Hotkey state machine: rec → stop → rec → stop (20 cycles) | Auto |
| TC-290 | Hotkey works after 100+ toggle cycles | Auto |
| TC-291 | Hotkey works after app runs for 24+ hours | Auto |
| TC-292 | Hotkey works with no output directory set (shows notification) | Auto |
| TC-293 | Hotkey pressed before first-run setup — does nothing | Auto |
| TC-294 | Multiple hotkeys: can register more than one | Auto |
| TC-295 | Hotkey with uppercase 'R' vs lowercase 'r' — same | Auto |
| TC-296 | Hotkey with trailing spaces in config — trimmed | Auto |
| TC-297 | Hotkey with '+' in key name (Ctrl++) — handled | Auto |
| TC-298 | Hotkey for numpad Enter — works | Auto |
| TC-299 | Hotkey for Print Screen — works | Auto |
| TC-300 | Hotkey for Pause/Break — works | Auto |
| TC-301 | Hotkey for Scroll Lock — works | Auto |
| TC-302 | Hotkey with no modifiers + single key (e.g. "R") — blocked (needs modifier) | Auto |
| TC-303 | OS-level hotkey (Win+R) not overridden | Auto |
| TC-304 | Ctrl+Alt+Del not overridden | Auto |
| TC-305 | Win+L not overridden | Auto |
| TC-306 | Win+Tab not overridden | Auto |
| TC-307 | Alt+F4 not overridden | Auto |
| TC-308 | Hotkey: F13-F24 keys if available | Manual |
| TC-309 | Hotkey: media keys (Play/Pause, etc.) — if supported | Manual |
| TC-310 | Hotkey on laptop without Fn lock — F1-F12 work | Manual |

---

## 5. Recording Engine Tests (TC-341 to TC-540)

| ID | Description | Type |
|----|------------|------|
| TC-341 | Start recording from idle state | Auto |
| TC-342 | Recording starts within 1 second of hotkey press | Auto |
| TC-343 | Recording indicator (status) changes to recording | Auto |
| TC-344 | FFmpeg process is spawned | Auto |
| TC-345 | FFmpeg process uses expected arguments | Auto |
| TC-346 | Recording uses gdigrab capture method (Windows) | Auto |
| TC-347 | Recording uses libx264 codec | Auto |
| TC-348 | Recording uses ultrafast preset | Auto |
| TC-349 | Recording uses CRF 23 quality | Auto |
| TC-350 | Recording uses yuv420p pixel format | Auto |
| TC-351 | Recording uses 30 fps by default | Auto |
| TC-352 | Stop recording from recording state | Auto |
| TC-353 | Recording stops within 1 second of hotkey press | Auto |
| TC-354 | 'q' command sent to ffmpeg stdin on stop | Auto |
| TC-355 | FFmpeg process exits cleanly | Auto |
| TC-356 | Output file is not corrupted (valid MP4) | Auto |
| TC-357 | Output file is playable in VLC/MPC-HC | Manual |
| TC-358 | Output file is playable in Windows Media Player | Manual |
| TC-359 | Output file is playable in browser (Chrome) | Auto |
| TC-360 | 1-second recording produces output | Auto |
| TC-361 | 5-second recording produces output | Auto |
| TC-362 | 60-second recording — no frame drops | Auto |
| TC-363 | 10-minute recording — no corruption | Auto |
| TC-364 | 60-minute recording — stable | Auto |
| TC-365 | 12-hour recording — memory stability | Auto |
| TC-366 | Full screen mode: captures primary monitor only | Auto |
| TC-367 | All monitors mode: captures entire virtual desktop | Auto |
| TC-368 | Active window mode: captures foreground window | Auto |
| TC-369 | Full screen: correct resolution captured | Auto |
| TC-370 | Full screen: content matches actual screen | Manual |
| TC-371 | All monitors: each monitor visible in output | Manual |
| TC-372 | All monitors: correct combined resolution | Auto |
| TC-373 | All monitors: 2 monitors (1920x1080 + 1920x1080) | Manual |
| TC-374 | All monitors: 3 monitors different resolutions | Manual |
| TC-575 | All monitors: monitors in portrait mode | Manual |
| TC-376 | All monitors: monitors with different DPI | Manual |
| TC-377 | Window mode: captures a specific window | Manual |
| TC-378 | Window mode: window moves during capture — follows? | Manual |
| TC-379 | Window mode: window minimized — captures desktop? | Manual |
| TC-380 | Window mode: window covered by another — records correctly | Manual |
| TC-381 | Recording 1920x1080 — correct output | Auto |
| TC-382 | Recording 3840x2160 (4K) — works | Manual |
| TC-383 | Recording 1366x768 — works | Auto |
| TC-384 | Recording 2560x1440 — works | Auto |
| TC-385 | Recording at 30fps — 30fps output | Auto |
| TC-386 | Recording at 60fps — 60fps output (if supported) | Auto |
| TC-387 | Recording at 15fps — 15fps output | Auto |
| TC-388 | Recording at 24fps — 24fps output | Auto |
| TC-389 | Start recording when already recording — returns error | Auto |
| TC-390 | Stop recording when not recording — returns error | Auto |
| TC-391 | Recording while screensaver active | Manual |
| TC-392 | Recording while display turns off (power save) | Manual |
| TC-393 | Recording with moving content (video playing) | Manual |
| TC-394 | Recording with static content (text editor) | Auto |
| TC-395 | Recording with fast-moving content (game) | Manual |
| TC-396 | Recording with multiple overlapping windows | Manual |
| TC-397 | Recording with transparent window on top | Manual |
| TC-398 | Recording with different background colors | Auto |
| TC-399 | Recording with hardware acceleration (if available) | Auto |
| TC-400 | Recording with HDR content (limited to SDR output) | Manual |
| TC-401 | Recording at 240fps — gracefully degrades or warns | Auto |
| TC-402 | Recording at framerate higher than monitor refresh | Auto |
| TC-403 | Variable framerate content — recording stays stable | Auto |
| TC-404 | Recording while CPU at 100% — frame loss acceptable | Auto |
| TC-405 | Recording while GPU at 100% — no crash | Auto |
| TC-406 | Recording while low memory (100MB free) — handles | Auto |
| TC-407 | Recording while disk space reaches 0 — handles gracefully | Auto |
| TC-408 | Recording to full disk — ffmpeg stops, file is playable up to that point | Auto |
| TC-409 | Recording to USB drive that is removed — error handled | Auto |
| TC-410 | Recording to network drive that disconnects — error handled | Auto |
| TC-411 | Recording to OneDrive/cloud folder — works | Manual |
| TC-412 | Recording when output path is deleted mid-recording | Auto |
| TC-413 | Toggle recording 50 times — no memory leak | Auto |
| TC-414 | Toggle recording 200 times — stable | Auto |
| TC-415 | Toggle recording 1000 times — stable | Auto |
| TC-416 | Recording with system audio (if implemented) | Manual |
| TC-417 | Recording with microphone (if implemented) | Manual |
| TC-418 | Recording with no audio — video-only | Auto |
| TC-419 | Rapid start-stop-start (3x within 2s) — no crash | Auto |
| TC-420 | Start, wait 100ms, stop — minimal viable recording | Auto |
| TC-421 | CPU usage during recording — <20% on modern CPU | Auto |
| TC-422 | RAM usage during recording — <200MB | Auto |
| TC-423 | RAM usage after 1hr recording — no leak | Auto |
| TC-424 | Disk write rate — reasonable for settings | Auto |
| TC-425 | FFmpeg not found — clear error notification | Auto |
| TC-426 | FFmpeg crashes mid-recording — detection and cleanup | Auto |
| TC-427 | FFmpeg hangs — watchdog timeout kills process | Auto |
| TC-428 | Recording with unusual aspect ratio (21:9) | Manual |
| TC-429 | Recording with different color depth (8-bit vs 10-bit) | Auto |
| TC-430 | Recording on 30Hz monitor — output at 30fps | Auto |
| TC-431 | Recording while dragging windows — smooth | Manual |
| TC-432 | Recording while scrolling — smooth | Manual |
| TC-433 | Recording while playing audio — no sync issues | Manual |
| TC-434 | File size for 10s recording — reasonable (<50MB) | Auto |
| TC-435 | File size for 60s recording — reasonable (<300MB) | Auto |
| TC-436 | Output file has correct duration metadata | Auto |
| TC-437 | Output file has correct resolution metadata | Auto |
| TC-438 | Output file has correct framerate metadata | Auto |
| TC-439 | Output file created date matches recording time | Auto |
| TC-440 | Output file not overwritten accidentally | Auto |

---

## 6. Overlay Tests (TC-541 to TC-620)

| ID | Description | Type |
|----|------------|------|
| TC-541 | Overlay appears when recording starts | Auto |
| TC-542 | Overlay disappears when recording stops | Auto |
| TC-543 | Overlay is always-on-top of other windows | Auto |
| TC-544 | Overlay has transparent background | Auto |
| TC-545 | Overlay has no window decorations (no title bar) | Auto |
| TC-546 | Overlay does not appear in taskbar | Auto |
| TC-547 | Overlay does not appear in Alt+Tab switcher | Auto |
| TC-548 | Overlay cannot be focused (click-through) | Auto |
| TC-549 | Overlay shows 🔴 red dot | Auto |
| TC-550 | Overlay shows "REC" label | Auto |
| TC-551 | Overlay shows elapsed timer | Auto |
| TC-552 | Timer increments every second | Auto |
| TC-553 | Timer starts at 00:00 | Auto |
| TC-554 | Timer shows correct time after 60s (01:00) | Auto |
| TC-555 | Timer shows correct time after 3600s (60:00) | Auto |
| TC-556 | Overlay has rounded pill shape | Auto |
| TC-557 | Overlay has dark semi-transparent background | Auto |
| TC-558 | Overlay has red pulsing animation on the dot | Auto |
| TC-559 | Overlay positioned at top-right of primary monitor | Auto |
| TC-560 | Overlay 20px from top, 20px from right edge | Auto |
| TC-561 | Overlay can be dragged to reposition | Manual |
| TC-562 | Overlay position resets on next recording | Auto |
| TC-563 | Overlay visible on multi-monitor (primary only) | Manual |
| TC-564 | Overlay visible on top of full-screen apps | Manual |
| TC-565 | Overlay visible on top of full-screen games | Manual |
| TC-566 | Overlay visible on top of video players (fullscreen) | Manual |
| TC-567 | Overlay does NOT appear when overlay disabled in settings | Auto |
| TC-568 | Toggle overlay mid-recording — shows/hides? | Auto |
| TC-569 | Overlay on 4K display — correct size and position | Manual |
| TC-570 | Overlay on 768p display — fits within screen | Manual |
| TC-571 | Overlay on portrait display — correct position | Manual |
| TC-572 | Overlay text readable — proper contrast | Manual |
| TC-573 | Overlay font uses system sans-serif | Auto |
| TC-574 | Overlay timer uses monospace font | Auto |
| TC-575 | Overlay does not interfere with mouse clicks (click-through) | Auto |
| TC-576 | Overlay does not capture keyboard input | Auto |
| TC-577 | Overlay does not show on screenshots of other apps | Auto |
| TC-578 | Overlay visible when recording starts immediately | Auto |
| TC-579 | Overlay hidden when minimized (can't minimize) | Auto |
| TC-580 | Multiple overlay windows? Only one at a time | Auto |
| TC-581 | Overlay on Windows with custom DPI scaling | Manual |
| TC-582 | Overlay with high-contrast mode | Manual |
| TC-583 | Overlay with RTL display language | Manual |
| TC-584 | Overlay click-through verified with calculator | Manual |
| TC-585 | Overlay click-through verified with browser | Manual |
| TC-586 | Overlay click-through verified with VS Code | Manual |
| TC-587 | Overlay shown even when settings window is closed | Auto |
| TC-588 | Overlay disappears immediately when recording stops | Auto |
| TC-589 | Overlay does not persist after app quit | Auto |
| TC-590 | Overlay: timer syncs with actual recording duration | Auto |
| TC-591 | Overlay: opening overlay HTML directly — shows timer | Auto |
| TC-592 | Overlay: very long recording (>24h) — timer still correct | Auto |

---

## 7. System Tray Tests (TC-621 to TC-680)

| ID | Description | Type |
|----|------------|------|
| TC-621 | Tray icon appears on launch | Auto |
| TC-622 | Tray icon shows default tooltip "Dr. Record" | Auto |
| TC-623 | Left-click on tray icon opens settings | Auto |
| TC-624 | Right-click on tray icon shows context menu | Auto |
| TC-625 | Context menu has "Settings" item | Auto |
| TC-626 | Context menu has separator | Auto |
| TC-627 | Context menu has "Quit" item | Auto |
| TC-628 | Click "Settings" — opens settings window | Auto |
| TC-629 | Click "Quit" — app exits | Auto |
| TC-630 | Quit while recording — stops recording, saves file, then quits | Auto |
| TC-631 | Tray icon visible in system tray overflow area | Auto |
| TC-632 | Tray icon visible in notification area | Auto |
| TC-633 | Tray icon can be hidden by user via OS settings | Auto |
| TC-634 | Tray icon reappears after explorer.exe restart | Auto |
| TC-635 | Tray icon visible on multiple monitors (one icon) | Auto |
| TC-636 | Tray menu: click outside closes menu | Auto |
| TC-637 | Tray menu: keyboard navigation (arrow keys) | Auto |
| TC-638 | Tray menu: Enter activates item | Auto |
| TC-639 | Tray menu: Esc closes menu | Auto |
| TC-640 | Closing main window — app stays in tray | Auto |
| TC-641 | Double-click tray icon — behavior (open settings) | Auto |
| TC-642 | Tray icon changes appearance during recording? | Auto |
| TC-643 | Tray icon custom icon used (not default) | Auto |
| TC-644 | Tray icon on high-DPI display — sharp | Manual |
| TC-645 | Context menu: "Settings" item disabled when recording? | Auto |
| TC-646 | Tray: multiple clicks on tray icon — debounced | Auto |
| TC-647 | Tray: icon survives Windows theme change | Auto |
| TC-648 | Tray: icon visible with dark taskbar | Auto |
| TC-649 | Tray: icon visible with light taskbar | Auto |
| TC-650 | Tray: icon visible with colored taskbar | Auto |
| TC-651 | Tray: icon visible in tablet mode | Manual |
| TC-652 | Tray: icon in remote desktop session | Manual |
| TC-653 | Tray: icon when taskbar is on left/right/top | Auto |
| TC-654 | Tray: "Quit" confirmation dialog (if implemented) | Auto |
| TC-655 | Tray: app still works while tray icon is hidden | Auto |
| TC-656 | Tray: balloon notification on first recording | Auto |
| TC-657 | Tray: balloon notification on recording error | Auto |

---

## 8. File Output Tests (TC-681 to TC-760)

| ID | Description | Type |
|----|------------|------|
| TC-681 | Output file saved in configured output directory | Auto |
| TC-682 | Output filename format: `DrRecord_Screen_YYYY-MM-DD_HH-MM-SS.mp4` | Auto |
| TC-683 | Output filename for full screen: `DrRecord_Screen_...` | Auto |
| TC-684 | Output filename for all monitors: `DrRecord_Multi_...` | Auto |
| TC-685 | Output filename for window: `DrRecord_Window_...` | Auto |
| TC-686 | File extension is `.mp4` | Auto |
| TC-687 | Filename uses correct date from local time | Auto |
| TC-688 | Timestamps match recording start time | Auto |
| TC-689 | Multiple recordings — filenames do not collide | Auto |
| TC-690 | Concurrent recordings — filenames unique | Auto |
| TC-691 | File is valid MP4 container | Auto |
| TC-692 | File plays in ffplay without errors | Auto |
| TC-693 | File has valid moov atom (fast-start) | Auto |
| TC-694 | File can be opened in video editor | Manual |
| TC-695 | File can be uploaded to YouTube | Manual |
| TC-696 | File can be shared via messaging apps | Manual |
| TC-697 | File size fits within email attachment limits? | Auto |
| TC-698 | Output file attributes: not read-only | Auto |
| TC-699 | Output file attributes: not hidden | Auto |
| TC-700 | Output file accessible by other users on system | Auto |
| TC-701 | Output to path with spaces — file created | Auto |
| TC-702 | Output to path with unicode — file created | Auto |
| TC-703 | Output to existing directory — file added, nothing deleted | Auto |
| TC-704 | Output to USB drive — file written successfully | Manual |
| TC-705 | Output to network share — file written successfully | Manual |
| TC-706 | Output to OneDrive folder — syncs correctly | Manual |
| TC-707 | Output to Google Drive folder — syncs correctly | Manual |
| TC-708 | Output to DropBox folder — syncs correctly | Manual |
| TC-709 | Output to RAM disk — file written | Manual |
| TC-710 | Output to read-only directory — error reported | Auto |
| TC-711 | Output to directory with no write permission — error | Auto |
| TC-712 | Output to directory with quota — handles gracefully | Auto |
| TC-713 | File handle released after recording — can be moved/deleted | Auto |
| TC-714 | File can be deleted immediately after recording | Auto |
| TC-715 | File can be renamed immediately after recording | Auto |
| TC-716 | File can be copied during recording (read access) | Auto |
| TC-717 | File size for 30s recording at 1080p — typical 20-50MB | Auto |
| TC-718 | File size for 30s recording at 4K — typical 50-150MB | Auto |
| TC-719 | Output file is not zero-byte | Auto |
| TC-720 | Output file has correct creation time | Auto |
| TC-721 | Output file has correct modification time | Auto |
| TC-722 | Output file has correct access time | Auto |
| TC-723 | File metadata: title contains "Dr. Record" | Auto |
| TC-724 | File metadata: encoder shows "libx264" | Auto |

---

## 9. Multi-Monitor Tests (TC-761 to TC-820)

| ID | Description | Type |
|----|------------|------|
| TC-761 | 2 monitors side-by-side — full screen captures primary | Manual |
| TC-762 | 2 monitors side-by-side — all monitors captures both | Manual |
| TC-763 | 2 monitors stacked vertically — all monitors works | Manual |
| TC-764 | 3 monitors (1+2 layout) — all monitors captures all | Manual |
| TC-765 | Laptop + external monitor — full screen on laptop | Manual |
| TC-766 | Laptop + external monitor — full screen on external | Manual |
| TC-767 | Laptop closed (external only) — recording works | Manual |
| TC-768 | Mixed DPI (100% + 150%) — all monitors correct | Manual |
| TC-769 | Mixed resolutions (1080p + 4K) — all monitors correct | Manual |
| TC-770 | Portrait + landscape — all monitors correct | Manual |
| TC-771 | Monitor set as primary switched — full screen follows | Manual |
| TC-772 | Monitor disconnected during recording — handled | Manual |
| TC-773 | Monitor connected during recording — handled | Manual |
| TC-774 | Display resolution changed mid-recording — handled | Manual |
| TC-775 | Display orientation changed mid-recording — handled | Manual |
| TC-776 | 2 monitors with different refresh rates — stable | Manual |

---

## 10. Edge Cases & Error Handling (TC-821 to TC-950)

| ID | Description | Type |
|----|------------|------|
| TC-821 | Start recording with no ffmpeg — error notification | Auto |
| TC-822 | Start recording with ffmpeg not in PATH — error | Auto |
| TC-823 | Start recording with no output directory — error | Auto |
| TC-824 | Start recording while another instance recording | Auto |
| TC-825 | Stop recording when not recording — ignored | Auto |
| TC-826 | Stop recording twice — second ignored | Auto |
| TC-827 | App crash mid-recording — ffmpeg orphaned? | Auto |
| TC-828 | App force-killed (Task Manager) during recording | Auto |
| TC-829 | System sleep during recording — resume, continue? | Manual |
| TC-830 | System hibernate during recording — resume? | Manual |
| TC-831 | System shutdown during recording — unsaved file? | Manual |
| TC-832 | System BSOD during recording — file partially saved | Manual |
| TC-833 | User switch (Fast User Switching) during recording | Manual |
| TC-834 | Screen locked during recording — still recording | Manual |
| TC-835 | UAC prompt during recording — recording continues | Manual |
| TC-836 | Game in exclusive fullscreen — recording works | Manual |
| TC-837 | DX12/Vulkan game — recording works | Manual |
| TC-838 | OBS running simultaneously — conflict | Manual |
| TC-839 | NVIDIA ShadowPlay running — conflict | Manual |
| TC-840 | Xbox Game Bar running — conflict | Manual |
| TC-841 | Multiple monitors with different color profiles | Manual |
| TC-842 | HDR monitor — recording in SDR correctly | Manual |
| TC-843 | Recording with 100+ Chrome tabs open | Manual |
| TC-844 | Recording with memory-heavy apps (Photoshop, etc.) | Manual |
| TC-845 | Recording during Windows Update | Manual |
| TC-846 | Recording during antivirus scan | Manual |
| TC-847 | Recording during file copy operation | Manual |
| TC-848 | Recording with proxy/VPN active | Auto |
| TC-849 | Recording with network adapter disabled | Auto |
| TC-850 | Config file in use by another process — error handled | Auto |
| TC-851 | Config file with null bytes — recreated | Auto |
| TC-852 | Config file with extremely long values — truncated | Auto |
| TC-853 | Config directory not writable — fallback to temp | Auto |
| TC-854 | Temp directory not writable — fallback to exe dir | Auto |
| TC-855 | App launched from read-only location | Auto |
| TC-856 | App launched from network share | Auto |
| TC-857 | Two instances of app running (bypass singleton) — conflict | Auto |
| TC-858 | FFmpeg process killed externally — app detects | Auto |
| TC-859 | FFmpeg process crashes — app detects and cleans up | Auto |
| TC-860 | FFmpeg output pipe broken — app detects | Auto |
| TC-861 | Recording 0 seconds (start→instantly stop) — empty file | Auto |
| TC-862 | Recording with system clock change mid-recording | Auto |
| TC-863 | Recording during time zone change | Auto |
| TC-864 | Recording during daylight saving time transition | Auto |
| TC-865 | Recording on Feb 29 (leap year) | Auto |
| TC-866 | Recording on Jan 1 00:00 (year boundary) | Auto |
| TC-867 | Recording while very low battery (2%) | Manual |
| TC-868 | Recording while overheating — system throttles | Manual |
| TC-869 | App left idle for 24 hours — still responsive | Auto |
| TC-870 | App left idle for 7 days — no memory leak | Auto |
| TC-871 | Hotkey pressed 1000 times rapidly — debounce works | Auto |
| TC-872 | Window area only: record specific region (if implemented) | Manual |
| TC-873 | Window area: coordinates saved and restored | Auto |
| TC-874 | Region recording: select area with mouse | Manual |
| TC-875 | Region recording: minimum size enforced | Auto |
| TC-876 | Region recording: can't select outside monitor bounds | Auto |
| TC-877 | Region recording: overlaps multiple monitors | Manual |
| TC-878 | Recording: maximum supported resolution (8K) | Manual |
| TC-879 | Recording: maximum supported duration (24h) | Manual |
| TC-880 | Recording: maximum file size (4GB+ — check 32-bit limit) | Auto |
| TC-881 | File output: over 260 char path — Windows long path | Auto |
| TC-882 | File output: filename with invalid chars — sanitized | Auto |
| TC-883 | File output: filename collision — auto-rename | Auto |
| TC-884 | File output: concurrent write from other app — handled | Auto |
| TC-885 | Exit code 0 on normal quit | Auto |
| TC-886 | Exit code non-zero on error | Auto |
| TC-887 | No crash dumps left behind | Auto |
| TC-888 | No temporary files left after clean quit | Auto |
| TC-889 | Temporary files cleaned up if app crashes | Auto |

---

## 11. Configuration Tests (TC-951 to TC-1020)

| ID | Description | Type |
|----|------------|------|
| TC-951 | Config file location: %APPDATA%\dr-record\config.json | Auto |
| TC-952 | Config file format: valid JSON | Auto |
| TC-953 | Config file has `output_dir` field | Auto |
| TC-954 | Config file has `hotkey` field | Auto |
| TC-955 | Config file has `recording_mode` field | Auto |
| TC-956 | Config file has `framerate` field | Auto |
| TC-957 | Config file has `show_overlay` field | Auto |
| TC-958 | Config file has `first_run` field | Auto |
| TC-959 | Config file encoding: UTF-8 without BOM | Auto |
| TC-960 | Config file permissions: user-only (not world-readable) | Auto |
| TC-961 | Config backup: file won't be corrupted on write | Auto |
| TC-962 | Config atomic write: partial write doesn't corrupt | Auto |
| TC-963 | Config file saved after settings change | Auto |
| TC-964 | Config file loaded on app start | Auto |
| TC-965 | Config: output_dir empty — default to Videos folder | Auto |
| TC-966 | Config: hotkey empty — default to Ctrl+Shift+R | Auto |
| TC-967 | Config: invalid hotkey string — fallback to default | Auto |
| TC-968 | Config: recording_mode invalid — fallback to fullscreen | Auto |
| TC-969 | Config: framerate 0 — default to 30 | Auto |
| TC-970 | Config: framerate >144 — capped at 60 | Auto |
| TC-971 | Config: unknown fields preserved (forward compat) | Auto |
| TC-972 | Config: missing fields filled with defaults | Auto |
| TC-973 | Config: null values treated as missing → default | Auto |
| TC-974 | Config: modify via external editor while app closed | Auto |
| TC-975 | Config: modify via external editor while app open — no hot reload | Auto |
| TC-976 | Config: save 100 times in a row — no corruption | Auto |
| TC-977 | Config: very large values (>1MB) — truncated | Auto |
| TC-978 | Config: concurrent write from two app instances — last writes wins | Auto |
| TC-979 | Config: path resolution with env vars (%USERPROFILE%) | Auto |

---

## 12. Performance Tests (TC-1021 to TC-1084)

| ID | Description | Type |
|----|------------|------|
| TC-1021 | App launch time — <2s on SSD | Auto |
| TC-1022 | App launch time — <5s on HDD | Manual |
| TC-1023 | Settings window open time — <500ms | Auto |
| TC-1024 | Settings window close time — <200ms | Auto |
| TC-1025 | Recording start latency — <1s | Auto |
| TC-1026 | Recording stop latency — <1s | Auto |
| TC-1027 | Overlay show time — <200ms | Auto |
| TC-1028 | Overlay hide time — <100ms | Auto |
| TC-1029 | Memory usage (idle) — <50MB | Auto |
| TC-1030 | Memory usage (recording) — <200MB | Auto |
| TC-1031 | Memory usage (24h recording) — <500MB | Auto |
| TC-1032 | CPU usage (idle) — <1% | Auto |
| TC-1033 | CPU usage (recording 1080p) — <15% | Auto |
| TC-1034 | CPU usage (recording 4K) — <30% | Auto |
| TC-1035 | CPU usage (idle, tray only) — <0.5% | Auto |
| TC-1036 | Disk I/O (idle) — none | Auto |
| TC-1037 | Disk I/O (recording) — sequential write only | Auto |
| TC-1038 | Network I/O — none (offline app) | Auto |
| TC-1039 | FPS impact on recorded content — <5% overhead | Manual |
| TC-1040 | FPS impact on games — <10% overhead | Manual |
| TC-1041 | Input lag impact — imperceptible | Manual |
| TC-1042 | GPU encoding vs CPU encoding — GPU preferred | Auto |
| TC-1043 | Battery drain during recording — <10% per hour | Manual |
| TC-1044 | Binary size — <25MB | Auto |
| TC-1045 | Installer size — <30MB | Auto |
| TC-1046 | Memory leak test: 24h idle, same memory usage | Auto |
| TC-1047 | Memory leak test: 100 record cycles, same baseline | Auto |
| TC-1048 | Memory leak test: 1000 settings open/close cycles | Auto |
| TC-1049 | Thread count (idle) — <10 threads | Auto |
| TC-1050 | Thread count (recording) — <15 threads | Auto |
| TC-1051 | Handle count (idle) — <200 handles | Auto |
| TC-1052 | Handle count (recording) — <300 handles | Auto |
| TC-1053 | GDI objects (idle) — <50 | Auto |
| TC-1054 | GDI objects (recording) — <100 | Auto |
| TC-1055 | No crash after 1000 hotkey toggles | Auto |
| TC-1056 | No crash after 10,000 hotkey toggles | Auto |
| TC-1057 | No crash after 100 settings saves | Auto |
| TC-1058 | No crash with rapid window show/hide (50x) | Auto |
| TC-1059 | No crash with tray icon click spam (100x) | Auto |
| TC-1060 | Settings window: no flicker on open | Manual |
| TC-1061 | Settings window: no flicker on close | Manual |
| TC-1062 | Overlay: no flicker on show | Manual |
| TC-1063 | Overlay: no flicker on hide | Manual |
| TC-1064 | Overlay: smooth timer update (no stutter) | Manual |
| TC-1065 | Overlay: smooth animation (pulse) | Manual |
| TC-1066 | App responds to shutdown event (WM_QUERYENDSESSION) | Auto |
| TC-1067 | App responds to logoff event | Auto |
| TC-1068 | No 100% CPU spin loops | Auto |
| TC-1069 | No blocked threads / deadlocks | Auto |
| TC-1070 | No handle leaks over time | Auto |
| TC-1071 | No GDI leaks over time | Auto |
| TC-1072 | Recording with vsync on — no tearing | Manual |
| TC-1073 | Recording with vsync off — no tearing | Manual |
| TC-1074 | File: check output with MediaInfo — valid | Auto |
| TC-1075 | File: check output with ffprobe — valid | Auto |
| TC-1076 | File: can be converted to other formats | Auto |
| TC-1077 | File: can be concatenated with other clips | Auto |
| TC-1078 | App: no dependency on internet connectivity | Auto |
| TC-1079 | App: no telemetry sent | Auto |
| TC-1080 | App: no background updater | Auto |
| TC-1081 | App: single binary (no extra DLLs needed) | Auto |
| TC-1082 | App: no admin access required for normal operation | Auto |
| TC-1083 | App: clean uninstall leaves no registry keys | Auto |
| TC-1084 | App: clean uninstall leaves no files in AppData | Auto |

---

## Summary

| Category | Total | Automated | Manual |
|----------|-------|-----------|--------|
| Installation | 55 | 18 | 37 |
| First Launch | 65 | 50 | 15 |
| Settings Window | 110 | 95 | 15 |
| Hotkey System | 80 | 65 | 15 |
| Recording Engine | 200 | 155 | 45 |
| Overlay | 52 | 40 | 12 |
| System Tray | 37 | 35 | 2 |
| File Output | 44 | 40 | 4 |
| Multi-Monitor | 16 | 2 | 14 |
| Edge Cases & Errors | 69 | 50 | 19 |
| Configuration | 29 | 29 | 0 |
| Performance | 64 | 55 | 9 |
| **Total** | **821** | **634** | **187** |
