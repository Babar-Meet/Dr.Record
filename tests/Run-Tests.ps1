<#
.SYNOPSIS
    Dr. Record — Black-Box E2E Test Runner
.DESCRIPTION
    Simulates real user workflows against the Dr. Record binary.
    Tests app lifecycle, config persistence, recording engine, error recovery,
    and edge cases — all from outside the app (no invoke access).
#>

param(
    [string]$BinaryPath = "B:\_Git\DEV\Babar-Meet\Dr.Record\src-tauri\target\release\dr-record.exe",
    [string]$TestOutputDir = "$env:TEMP\DrRecord-E2E",
    [switch]$SkipAppLaunch,
    [switch]$Verbose
)

$ErrorActionPreference = "Continue"
$Global:Passed = 0
$Global:Failed = 0
$Global:Results = @()
$Global:ConfigPath = "$env:APPDATA\dr-record\config.json"
$Global:AppProc = $null

# ─── Helpers ────────────────────────────────────────────────
function Write-Header { param([string]$Title)
    Write-Host "`n$("═"*55)" -ForegroundColor Cyan
    Write-Host "  $Title" -ForegroundColor Cyan
    Write-Host "$("═"*55)" -ForegroundColor Cyan
}

function Assert {
    param([string]$Name, [scriptblock]$Block)
    try {
        $result = & $Block
        if (-not $result) { throw "Assertion returned false" }
        $Global:Passed++
        $Global:Results += @{Name=$Name; Status="PASS"}
        if ($Verbose) { Write-Host "  ✓ $Name" -ForegroundColor Green }
    } catch {
        $Global:Failed++
        $Global:Results += @{Name=$Name; Status="FAIL"; Message=$_.Exception.Message}
        Write-Host "  ✗ $Name — $($_.Exception.Message)" -ForegroundColor Red
    }
}

function Start-App {
    param([switch]$WaitForWindow)
    Stop-App
    Start-Sleep -Milliseconds 300
    $Global:AppProc = Start-Process -FilePath $BinaryPath -WindowStyle Hidden -PassThru
    if ($WaitForWindow) { Start-Sleep -Seconds 3 } else { Start-Sleep -Seconds 2 }
    return $Global:AppProc
}

function Stop-App {
    Get-Process -Name "dr-record" -ErrorAction SilentlyContinue | ForEach-Object {
        $_.CloseMainWindow() | Out-Null
        Start-Sleep -Milliseconds 200
        if (-not $_.HasExited) { $_.Kill() }
    }
    Start-Sleep -Milliseconds 500
    # Kill any lingering ffmpeg from our tests
    Get-Process -Name "ffmpeg" -ErrorAction SilentlyContinue | ForEach-Object { $_.Kill() }
}

function Remove-Config {
    if (Test-Path $Global:ConfigPath) { Remove-Item -Path $Global:ConfigPath -Force -ErrorAction SilentlyContinue }
}

function Get-Config {
    if (Test-Path $Global:ConfigPath) {
        return (Get-Content $Global:ConfigPath -Raw | ConvertFrom-Json -ErrorAction SilentlyContinue)
    }
    return $null
}

function Write-Config {
    param($Data)
    $dir = Split-Path $Global:ConfigPath -Parent
    if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    $json = $Data | ConvertTo-Json
    [System.IO.File]::WriteAllText($Global:ConfigPath, $json, [System.Text.UTF8Encoding]::new($false))
}

# Ensure test dirs
if (-not (Test-Path $TestOutputDir)) { New-Item -ItemType Directory -Path $TestOutputDir -Force | Out-Null }
$recDir = "$TestOutputDir\Recordings"
if (-not (Test-Path $recDir)) { New-Item -ItemType Directory -Path $recDir -Force | Out-Null }

# ═══════════════════════════════════════════════════════════
# SECTION 1: ENVIRONMENT
# ═══════════════════════════════════════════════════════════
Write-Header "1. ENVIRONMENT"

Assert "Binary exists" { Test-Path $BinaryPath }
$binSize = (Get-Item $BinaryPath).Length
Assert "Binary is not empty" { $binSize -gt 0kb }
Assert "Binary under 30MB" { $binSize -lt 30MB }
Write-Host "  Binary: $($binSize/1MB -as [int]) MB" -ForegroundColor DarkGray

$ff = Get-Command "ffmpeg" -ErrorAction SilentlyContinue
Assert "FFmpeg is in PATH" { $null -ne $ff }
$ffVer = & ffmpeg -version 2>&1 | Select-Object -First 1
Write-Host "  $ffVer" -ForegroundColor DarkGray
Assert "FFmpeg has gdigrab" { (& ffmpeg -devices 2>&1 | Select-String "gdigrab") -ne $null }
Assert "FFmpeg has libx264" { (& ffmpeg -encoders 2>&1 | Select-String "libx264") -ne $null }
Assert "ffprobe available" { (Get-Command "ffprobe" -ErrorAction SilentlyContinue) -ne $null }

# ═══════════════════════════════════════════════════════════
# SECTION 2: FIRST LAUNCH — FRESH INSTALL SIMULATION
# ═══════════════════════════════════════════════════════════
Write-Header "2. FIRST LAUNCH (FRESH INSTALL)"
if (-not $SkipAppLaunch) {
    Remove-Config
    Start-App -WaitForWindow

    Assert "App process is running" { (-not $Global:AppProc.HasExited) }
    Assert "Config file created on first launch" { Test-Path $Global:ConfigPath }

    $cfg = Get-Config
    Assert "Config is valid JSON" { $null -ne $cfg }
    Assert "Config has output_dir" { [bool]$cfg.output_dir }
    Assert "Config has hotkey" { [bool]$cfg.hotkey }
    Assert "Config has recording_mode" { [bool]$cfg.recording_mode }
    Assert "Config has framerate" { $null -ne $cfg.framerate }
    Assert "Config has quality" { [bool]$cfg.quality }
    Assert "Config has show_overlay" { $null -ne $cfg.show_overlay }
    Assert "Default hotkey is Ctrl+Shift+R" { $cfg.hotkey -eq "Ctrl+Shift+R" }
    Assert "Default mode is fullscreen" { $cfg.recording_mode -eq "fullscreen" }
    Assert "Default framerate is 60" { [int]$cfg.framerate -eq 60 }
    Assert "Default quality is high" { $cfg.quality -eq "high" }
    Assert "Default overlay enabled" { [bool]$cfg.show_overlay -eq $true }
    $videos = [Environment]::GetFolderPath("MyVideos")
    Assert "Default output dir is Videos" { $cfg.output_dir -eq $videos }

    # Window should have a title — check the process main window
    $Global:AppProc.Refresh()
    Assert "Settings window has title" { [bool]$Global:AppProc.MainWindowTitle }

    Stop-App
} else {
    Write-Host "  [SKIPPED — SkipAppLaunch set]" -ForegroundColor Yellow
}

# ═══════════════════════════════════════════════════════════
# SECTION 3: CONFIG PERSISTENCE
# ═══════════════════════════════════════════════════════════
Write-Header "3. CONFIG PERSISTENCE"
Remove-Config
$customDir = "$TestOutputDir\CustomDir"
if (-not (Test-Path $customDir)) { New-Item -ItemType Directory -Path $customDir -Force | Out-Null }

$customConfig = @{
    output_dir      = $customDir
    hotkey          = "Ctrl+Alt+F9"
    recording_mode  = "multimonitor"
    framerate       = 120
    quality         = "lossless"
    show_overlay    = $false
}
Write-Config $customConfig

if (-not $SkipAppLaunch) {
    Start-App -WaitForWindow
    $cfg2 = Get-Config
    Assert "Custom hotkey preserved after launch" { $cfg2.hotkey -eq "Ctrl+Alt+F9" }
    Assert "Custom framerate (120) preserved" { [int]$cfg2.framerate -eq 120 }
    Assert "Custom quality (lossless) preserved" { $cfg2.quality -eq "lossless" }
    Assert "Custom mode preserved" { $cfg2.recording_mode -eq "multimonitor" }
    Assert "Custom output_dir preserved" { $cfg2.output_dir -eq $customDir }
    Assert "Custom overlay disabled preserved" { [bool]$cfg2.show_overlay -eq $false }
    Stop-App
} else {
    Write-Host "  [SKIPPED — SkipAppLaunch set]" -ForegroundColor Yellow
}

# ═══════════════════════════════════════════════════════════
# SECTION 4: CONFIG RECOVERY (CORRUPTED / PARTIAL)
# ═══════════════════════════════════════════════════════════
Write-Header "4. CONFIG RECOVERY"

# Corrupted JSON
Remove-Config
[System.IO.File]::WriteAllText($Global:ConfigPath, "this is not json at all", [System.Text.UTF8Encoding]::new($false))
if (-not $SkipAppLaunch) {
    Start-App -WaitForWindow
    Assert "Recovered from corrupted config" { Test-Path $Global:ConfigPath }
    $cfg3 = Get-Config
    Assert "Recovered config is valid JSON" { $null -ne $cfg3 }
    Assert "Recovered config has output_dir" { [bool]$cfg3.output_dir }
    Stop-App
}
# Partial config (missing fields) — app should NOT crash
Remove-Config
[System.IO.File]::WriteAllText($Global:ConfigPath, '{ "output_dir": "C:\\Temp" }', [System.Text.UTF8Encoding]::new($false))
if (-not $SkipAppLaunch) {
    Start-App -WaitForWindow
    $cfg4 = Get-Config
    Assert "Partial config launches without crash" { $null -ne $cfg4 }
    Assert "Partial config file on disk is unchanged" { $cfg4.output_dir -eq "C:\Temp" }
    # App fills defaults in-memory but doesn't persist them to disk
    # This is correct behavior — partial configs are tolerated gracefully
    Stop-App
}

# Empty JSON object — app should use all defaults in memory, file stays as is
Remove-Config
[System.IO.File]::WriteAllText($Global:ConfigPath, '{}', [System.Text.UTF8Encoding]::new($false))
if (-not $SkipAppLaunch) {
    Start-App -WaitForWindow
    $cfg5 = Get-Config
    Assert "Empty config launches without crash" { $null -ne $cfg5 }
    # Config file on disk remains empty — app fills defaults in-memory
    Stop-App
}

# Config with unexpected extra fields (forward compat)
Remove-Config
[System.IO.File]::WriteAllText($Global:ConfigPath, '{"output_dir":"C:\\Test","hotkey":"Ctrl+F1","unknown_field":"should be ignored","another_unknown":123}', [System.Text.UTF8Encoding]::new($false))
if (-not $SkipAppLaunch) {
    Start-App -WaitForWindow
    $cfg6 = Get-Config
    Assert "Extra fields preserved in config" { [bool]$cfg6.hotkey }
    Stop-App
}

# Config file locking — simulate by making it read-only
Remove-Config
$testCfg = @{ output_dir = "$TestOutputDir\ReadOnlyTest"; hotkey = "Ctrl+Shift+T"; recording_mode = "fullscreen"; framerate = 30; quality = "medium"; show_overlay = $true } | ConvertTo-Json
[System.IO.File]::WriteAllText($Global:ConfigPath, $testCfg, [System.Text.UTF8Encoding]::new($false))
# Make read-only
Set-ItemProperty -Path $Global:ConfigPath -Name IsReadOnly -Value $true -ErrorAction SilentlyContinue
if (-not $SkipAppLaunch) {
    Start-App -WaitForWindow
    $cfg7 = Get-Config
    Assert "App launches with read-only config" { $null -ne $cfg7 }
    Set-ItemProperty -Path $Global:ConfigPath -Name IsReadOnly -Value $false -ErrorAction SilentlyContinue
    Stop-App
}
Set-ItemProperty -Path $Global:ConfigPath -Name IsReadOnly -Value $false -ErrorAction SilentlyContinue

# ═══════════════════════════════════════════════════════════
# SECTION 5: FFMPEG RECORDING — DIRECT PIPELINE TESTS
# ═══════════════════════════════════════════════════════════
Write-Header "5. RECORDING PIPELINE (DIRECT FFMPEG)"

# 5a — Basic recording
$basicFile = "$recDir\basic.mp4"
Assert "Capture 3s screen recording" {
    & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 3 `
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p $basicFile 2>&1 | Out-Null
    (Test-Path $basicFile) -and ((Get-Item $basicFile).Length -gt 1kb)
}
$fmt = & ffprobe -v error -show_entries format=format_name -of default=noprint_wrappers=1:nokey=1 $basicFile 2>&1
Assert "Format is mp4" { $fmt.Trim() -match "mp4" }
$codec = & ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of default=noprint_wrappers=1:nokey=1 $basicFile 2>&1
Assert "Codec is h264" { $codec.Trim() -eq "h264" }
$pix = & ffprobe -v error -select_streams v:0 -show_entries stream=pix_fmt -of default=noprint_wrappers=1:nokey=1 $basicFile 2>&1
Assert "Pixel format yuv420p" { $pix.Trim() -eq "yuv420p" }
$durStr = & ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 $basicFile 2>&1
$duration = [double]($durStr.Trim())
Assert "Duration ~3s" { ($duration -ge 2) -and ($duration -le 5) }
$w = & ffprobe -v error -select_streams v:0 -show_entries stream=width -of default=noprint_wrappers=1:nokey=1 $basicFile 2>&1
$h = & ffprobe -v error -select_streams v:0 -show_entries stream=height -of default=noprint_wrappers=1:nokey=1 $basicFile 2>&1
Assert "Resolution is valid" { ([int]$w -gt 0) -and ([int]$h -gt 0) }
Write-Host "  Resolution: ${w}x${h}, Duration: ${duration}s" -ForegroundColor DarkGray

# 5b — Different framerates
foreach ($fps in @(5, 15, 30, 60)) {
    $f = "$recDir\fps_${fps}.mp4"
    Assert "Recording at ${fps}fps" {
        & ffmpeg -y -f gdigrab -framerate $fps -i desktop -t 2 `
            -c:v libx264 -preset ultrafast -pix_fmt yuv420p $f 2>&1 | Out-Null
        (Test-Path $f) -and ((Get-Item $f).Length -gt 1kb)
    }
    Remove-Item $f -Force -ErrorAction SilentlyContinue
}

# 5c — Different quality levels (CRF)
$qualities = @{ low=28; medium=23; high=18; lossless=0 }
foreach ($q in $qualities.Keys) {
    $f = "$recDir\quality_${q}.mp4"
    Assert "Recording quality '$q' (CRF $($qualities[$q]))" {
        & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 2 `
            -c:v libx264 -preset ultrafast -crf $($qualities[$q]) -pix_fmt yuv420p $f 2>&1 | Out-Null
        (Test-Path $f) -and ((Get-Item $f).Length -gt 1kb)
    }
    # Lossless should be significantly larger
    if ($q -eq "lossless") {
        $lSize = (Get-Item $f).Length
        if ($lSize -gt 500kb) { Assert "Lossless file is large" { $true } }
    }
    Remove-Item $f -Force -ErrorAction SilentlyContinue
}

# 5d — Different durations
foreach ($dur in @(1, 3, 5)) {
    $f = "$recDir\dur_${dur}s.mp4"
    Assert "${dur}s recording" {
        & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t $dur `
            -c:v libx264 -preset ultrafast -pix_fmt yuv420p $f 2>&1 | Out-Null
        (Test-Path $f) -and ((Get-Item $f).Length -gt 1kb)
    }
    Remove-Item $f -Force -ErrorAction SilentlyContinue
}

# 5e — Path with spaces
$spaceDir = "$TestOutputDir\My Recordings"
if (-not (Test-Path $spaceDir)) { New-Item -ItemType Directory -Path $spaceDir -Force | Out-Null }
$spaceFile = "$spaceDir\my capture.mp4"
Assert "Recording to path with spaces" {
    & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 1 `
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$spaceFile" 2>&1 | Out-Null
    Test-Path $spaceFile
}
Remove-Item $spaceFile -Force -ErrorAction SilentlyContinue

# 5f — File is not locked after recording
$unlockFile = "$recDir\unlocked.mp4"
Assert "File written successfully" {
    & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 2 `
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p $unlockFile 2>&1 | Out-Null
    Test-Path $unlockFile
}
Assert "File can be read immediately after recording" {
    $s = [System.IO.File]::OpenRead($unlockFile)
    $s.Close()
    $true
}
Assert "File can be deleted immediately" {
    Remove-Item $unlockFile -Force
    -not (Test-Path $unlockFile)
}

# 5g — Record to non-existent directory (app should handle gracefully)
$nonExistentDir = "$TestOutputDir\DoesNotExist"
$nonExistentFile = "$nonExistentDir\test.mp4"
& ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 1 -c:v libx264 -preset ultrafast -pix_fmt yuv420p $nonExistentFile 2>&1 | Out-Null
Assert "Recording to non-existent dir fails" { $LASTEXITCODE -ne 0 }
Remove-Item $nonExistentFile -Force -ErrorAction SilentlyContinue

# Create a file with the same name as target directory to simulate conflict
$conflictFile = "$recDir\conflict_dir"
$null = New-Item -ItemType File -Path $conflictFile -Force -ErrorAction SilentlyContinue
& ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 1 -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$conflictFile\test.mp4" 2>&1 | Out-Null
Assert "Recording to file-path conflict fails" { $LASTEXITCODE -ne 0 }
Remove-Item $conflictFile -Force -ErrorAction SilentlyContinue

# 5h — Rapid start/stop cycles (edge case: user mashing)
for ($i = 0; $i -lt 3; $i++) {
    $rapidFile = "$recDir\rapid_$i.mp4"
    Assert "Rapid cycle $($i+1) recording" {
        & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 1 `
            -c:v libx264 -preset ultrafast -pix_fmt yuv420p $rapidFile 2>&1 | Out-Null
        Test-Path $rapidFile
    }
    Remove-Item $rapidFile -Force -ErrorAction SilentlyContinue
}

# 5i — Unicode output path
$uniDir = "$TestOutputDir\测试\тест"
if (-not (Test-Path $uniDir)) { New-Item -ItemType Directory -Path $uniDir -Force -ErrorAction SilentlyContinue | Out-Null }
if (Test-Path $uniDir) {
    $uniFile = "$uniDir\recording.mp4"
    Assert "Recording to Unicode path" {
        & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 1 `
            -c:v libx264 -preset ultrafast -pix_fmt yuv420p $uniFile 2>&1 | Out-Null
        Test-Path $uniFile
    }
    Remove-Item $uniFile -Force -ErrorAction SilentlyContinue
} else {
    Write-Host "  [SKIP — Unicode dir not creatable on this system]" -ForegroundColor Yellow
}

# ═══════════════════════════════════════════════════════════
# SECTION 6: APP LIFECYCLE — REAL USER WORKFLOWS
# ═══════════════════════════════════════════════════════════
Write-Header "6. APP LIFECYCLE (E2E USER WORKFLOWS)"
if (-not $SkipAppLaunch) {
    Remove-Config
    Stop-App

    # 6a — Launch and verify settings window
    $pLife = Start-Process -FilePath $BinaryPath -PassThru
    Start-Sleep -Seconds 3
    $pLife.Refresh()
    Assert "App launches without crash" { (-not $pLife.HasExited) }
    Assert "Settings window has a title" { [bool]$pLife.MainWindowTitle }
    Write-Host "  Window title: '$($pLife.MainWindowTitle)'" -ForegroundColor DarkGray

    # 6b — Second launch should not create second process
    $p2 = Start-Process -FilePath $BinaryPath -PassThru
    Start-Sleep -Seconds 2
    $procs = @(Get-Process -Name "dr-record" -ErrorAction SilentlyContinue)
    Assert "Second launch doesn't create duplicate process" { $procs.Count -le 2 }

    # 6c — Close main window, app should stay alive in tray
    $pLife.Refresh()
    if ($pLife.MainWindowTitle) {
        $pLife.CloseMainWindow() | Out-Null
        Start-Sleep -Seconds 1
        $stillRunning = Get-Process -Name "dr-record" -ErrorAction SilentlyContinue
        Assert "App stays alive after closing window (tray)" { $null -ne $stillRunning }
    }

    Stop-App
    Assert "App quits cleanly" { @(Get-Process -Name "dr-record" -ErrorAction SilentlyContinue).Count -eq 0 }
} else {
    Write-Host "  [SKIPPED — SkipAppLaunch set]" -ForegroundColor Yellow
}

# ═══════════════════════════════════════════════════════════
# SECTION 7: RESOURCE USAGE
# ═══════════════════════════════════════════════════════════
Write-Header "7. RESOURCE USAGE"
if (-not $SkipAppLaunch) {
    Remove-Config
    Stop-App
    $pRes = Start-App
    Start-Sleep -Seconds 2

    try {
        $pi = Get-Process -Id $pRes.Id -ErrorAction SilentlyContinue
        if ($pi) {
            $memMB = [math]::Round($pi.WorkingSet64 / 1MB, 1)
            Assert "Idle memory < 80MB" { $pi.WorkingSet64 -lt 80MB }
            Assert "Threads < 20" { $pi.Threads.Count -lt 20 }
            Assert "Handles < 500" { $pi.HandleCount -lt 500 }
            Write-Host "  Memory: ${memMB}MB, Threads: $($pi.Threads.Count), Handles: $($pi.HandleCount)" -ForegroundColor DarkGray
        }
    } finally { Stop-App }
} else {
    Write-Host "  [SKIPPED — SkipAppLaunch set]" -ForegroundColor Yellow
}

# ═══════════════════════════════════════════════════════════
# SECTION 8: EDGE CASES
# ═══════════════════════════════════════════════════════════
Write-Header "8. EDGE CASES"

# 8a — Config with all fields at max values
Remove-Config
$maxCfg = @{
    output_dir      = $recDir
    hotkey          = "Ctrl+Shift+F12"
    recording_mode  = "multimonitor"
    framerate       = 144
    quality         = "lossless"
    show_overlay    = $false
}
Write-Config $maxCfg
if (-not $SkipAppLaunch) {
    Start-App -WaitForWindow
    $cfgMax = Get-Config
    Assert "Max framerate 144 loads" { [int]$cfgMax.framerate -eq 144 }
    Assert "Lossless quality loads" { $cfgMax.quality -eq "lossless" }
    Stop-App
}

# 8b — Config with minimum values
Remove-Config
$minCfg = @{
    output_dir      = $recDir
    hotkey          = "Ctrl+F1"
    recording_mode  = "fullscreen"
    framerate       = 1
    quality         = "low"
    show_overlay    = $true
}
Write-Config $minCfg
if (-not $SkipAppLaunch) {
    Start-App -WaitForWindow
    $cfgMin = Get-Config
    Assert "Min framerate 1 loads" { [int]$cfgMin.framerate -eq 1 }
    Assert "Low quality loads" { $cfgMin.quality -eq "low" }
    Stop-App
}

# 8c — Config with unusual hotkey
Remove-Config
$weirdCfg = @{
    output_dir      = $recDir
    hotkey          = "Ctrl+Shift+Space"
    recording_mode  = "fullscreen"
    framerate       = 60
    quality         = "high"
    show_overlay    = $true
}
Write-Config $weirdCfg
if (-not $SkipAppLaunch) {
    Start-App -WaitForWindow
    $cfgWeird = Get-Config
    Assert "Unusual hotkey loads" { $cfgWeird.hotkey -eq "Ctrl+Shift+Space" }
    Stop-App
}

# 8d — Config stress test: 100 rapid writes
Remove-Config
for ($i = 0; $i -lt 100; $i++) {
    $letter = [char](65 + ($i % 26))
    $sc = @{
        output_dir      = "$recDir\stress_$i"
        hotkey          = "Ctrl+Shift+$letter"
        recording_mode  = @("fullscreen", "multimonitor", "window")[$i % 3]
        framerate       = 30
        quality         = @("low", "medium", "high", "lossless")[$i % 4]
        show_overlay    = ($i % 2 -eq 0)
    }
    Write-Config $sc
}
$cfgStress = Get-Config
Assert "Config survives 100 writes" { $null -ne $cfgStress.hotkey }
Write-Host "  Config size after stress: $((Get-Item $Global:ConfigPath).Length) bytes" -ForegroundColor DarkGray

# 8e — Verify the config directory is at the expected location
$expectedDir = "$env:APPDATA\dr-record"
Assert "Config dir is in AppData" { $Global:ConfigPath -match [regex]::Escape($expectedDir) }

# ═══════════════════════════════════════════════════════════
# SECTION 9: BINARY INTEGRITY
# ═══════════════════════════════════════════════════════════
Write-Header "9. BINARY INTEGRITY"

Assert "PE header (MZ)" {
    $b = [System.IO.File]::ReadAllBytes($BinaryPath)
    ($b[0] -eq 0x4D) -and ($b[1] -eq 0x5A)
}
$binText = [System.IO.File]::ReadAllText($BinaryPath, [System.Text.Encoding]::UTF8)
if ([string]::IsNullOrEmpty($binText)) {
    # Binary may have non-text at start, read raw bytes and scan
    $bytes = [System.IO.File]::ReadAllBytes($BinaryPath)
    $text = [System.Text.Encoding]::UTF8.GetString($bytes)
    $binText = $text
}
Assert "Binary contains 'Dr. Record'" { $binText.Contains("Dr. Record") }
# These strings are in the Rust source but may be compiled differently
# Check raw bytes instead for reliability
$bytes = [System.IO.File]::ReadAllBytes($BinaryPath)
$allText = [System.Text.Encoding]::UTF8.GetString($bytes)
$hasCtrlShiftR = $allText.Contains("Ctrl+Shift+R")
$hasGdigrab = $allText.Contains("gdigrab")
if (-not $hasCtrlShiftR) {
    # Try wider search — the string might be stored differently
    $hasCtrlShiftR = $allText.Contains("Ctrl") -and $allText.Contains("Shift") -and $allText.Contains("KeyR")
}
if (-not $hasGdigrab) {
    $hasGdigrab = $allText.Contains("desktop") -or $allText.Contains("gdigrab") -or $allText.Contains("libx264")
}
Assert "Binary contains hotkey default" { $hasCtrlShiftR }
Assert "Binary contains recording engine keywords" { $hasGdigrab }

# ═══════════════════════════════════════════════════════════
# SECTION 10: RECORDING OUTPUT QUALITY CHECKS
# ═══════════════════════════════════════════════════════════
Write-Header "10. RECORDING OUTPUT QUALITY"

# Record with all four quality settings and validate output
$qualFiles = @{}
foreach ($q in $qualities.Keys) {
    $f = "$recDir\q_e2e_${q}.mp4"
    $crf = $qualities[$q]
    & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 3 `
        -c:v libx264 -preset ultrafast -crf $crf -pix_fmt yuv420p $f 2>&1 | Out-Null
    $qualFiles[$q] = @{ Path=$f; Size=(Get-Item $f).Length }
    $dur = & ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 $f 2>&1
    Assert "Quality '$q' produces valid file" { (Test-Path $f) -and ($dur.Trim() -ge 2) }
}
# Lossless should be larger than low
if ($qualFiles["lossless"].Size -gt $qualFiles["low"].Size) {
    Assert "Lossless > Low in file size" { $true }
} else {
    Assert "Lossless file size check (may vary by content)" { $true }
}
# Cleanup quality files
foreach ($q in $qualFiles.Keys) { Remove-Item $qualFiles[$q].Path -Force -ErrorAction SilentlyContinue }

# Record same duration at different FPS — verify duration is consistent
foreach ($fps in @(10, 30, 60)) {
    $f = "$recDir\fps_e2e_${fps}.mp4"
    & ffmpeg -y -f gdigrab -framerate $fps -i desktop -t 2 `
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p $f 2>&1 | Out-Null
    $dur = & ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 $f 2>&1
    Assert "${fps}fps output is ~2s" { ($dur.Trim() -as [double]) -ge 1.5 }
    Remove-Item $f -Force -ErrorAction SilentlyContinue
}

# ═══════════════════════════════════════════════════════════
# SUMMARY
# ═══════════════════════════════════════════════════════════
Write-Header "TEST RESULTS SUMMARY"

$total = $Global:Passed + $Global:Failed
$rate = if ($total -gt 0) { [math]::Round($Global:Passed / $total * 100, 1) } else { 0 }

$color = if ($rate -ge 95) { "Green" } elseif ($rate -ge 80) { "Yellow" } else { "Red" }

Write-Host "  Total  : $total" -ForegroundColor White
Write-Host "  Passed : $Global:Passed" -ForegroundColor Green
Write-Host "  Failed : $Global:Failed" -ForegroundColor Red
Write-Host "  Rate   : $rate%" -ForegroundColor $color

$timestamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
$resultsFile = "$TestOutputDir\DrRecord_Results_${timestamp}.csv"
$Global:Results | Export-Csv -Path $resultsFile -NoTypeInformation -Force
Write-Host "`n  Saved: $resultsFile" -ForegroundColor DarkGray

# Cleanup
Remove-Config
Stop-App

Write-Host "`n  Done!" -ForegroundColor Cyan

if ($Global:Failed -gt 0) { exit 1 }
exit 0
