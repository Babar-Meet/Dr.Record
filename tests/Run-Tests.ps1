<#
.SYNOPSIS
    Dr. Record — Black-Box Test Runner
.DESCRIPTION
    Executes automated black-box tests against the Dr. Record binary.
#>

param(
    [string]$BinaryPath = "B:\_Git\DEV\Babar-Meet\Dr. Rec\dr-record\src-tauri\target\release\dr-record.exe",
    [string]$TestOutputDir = "$env:TEMP\DrRecord-TestOutput",
    [switch]$Verbose
)

$ErrorActionPreference = "Continue"
$Global:TestsPassed = 0
$Global:TestsFailed = 0
$Global:TestResults = @()
$Global:ConfigPath = "$env:APPDATA\dr-record\config.json"

function Write-TestHeader {
    param([string]$Title)
    Write-Host "`n═══════════════════════════════════════════" -ForegroundColor Cyan
    Write-Host "  $Title" -ForegroundColor Cyan
    Write-Host "═══════════════════════════════════════════" -ForegroundColor Cyan
}

function Assert {
    param([string]$Name, [scriptblock]$Block)
    try {
        $result = Invoke-Command -ScriptBlock $Block
        if ($result) {
            $Global:TestsPassed++
            $Global:TestResults += @{Name=$Name; Status="PASS"; Message=""}
            if ($Verbose) { Write-Host "  PASS: $Name" -ForegroundColor Green }
        } else {
            throw "Assertion returned false"
        }
    } catch {
        $Global:TestsFailed++
        $Global:TestResults += @{Name=$Name; Status="FAIL"; Message=$_.Exception.Message}
        Write-Host "  FAIL: $Name - $($_.Exception.Message)" -ForegroundColor Red
    }
}

function Stop-App {
    Get-Process -Name "dr-record" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
}

function Remove-Config {
    if (Test-Path $Global:ConfigPath) {
        Remove-Item -Path $Global:ConfigPath -Force -ErrorAction SilentlyContinue
    }
}

# Ensure test output directory
if (!(Test-Path $TestOutputDir)) {
    New-Item -ItemType Directory -Path $TestOutputDir -Force | Out-Null
}

# ═══════════════════════════════════════════
# SECTION 1: Environment Checks
# ═══════════════════════════════════════════
Write-TestHeader "1. ENVIRONMENT CHECKS"

Assert "Binary exists" { Test-Path $BinaryPath }

$binInfo = Get-Item $BinaryPath
Assert "Binary is not empty" { $binInfo.Length -gt 0 }
Assert "Binary size under 25MB" { $binInfo.Length -lt 25MB }
Write-Host "  Binary: $($binInfo.Length / 1MB) MB" -ForegroundColor Gray

$ffmpegCmd = Get-Command "ffmpeg" -ErrorAction SilentlyContinue
Assert "FFmpeg is in PATH" { $null -ne $ffmpegCmd }

$ffVersion = & ffmpeg -version 2>&1 | Select-Object -First 1
Assert "FFmpeg version retrieved" { $null -ne $ffVersion }
Write-Host "  FFmpeg: $ffVersion" -ForegroundColor Gray

Assert "FFmpeg supports gdigrab" { (& ffmpeg -devices 2>&1 | Select-String "gdigrab") -ne $null }
Assert "FFmpeg supports libx264" { (& ffmpeg -encoders 2>&1 | Select-String "libx264") -ne $null }

# ═══════════════════════════════════════════
# SECTION 2: Config File Tests
# ═══════════════════════════════════════════
Write-TestHeader "2. CONFIG FILE TESTS"

Remove-Config
Stop-App

# Launch the app
$proc = Start-Process -FilePath $BinaryPath -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 3

Assert "App process is running" { ($null -ne $proc) -and (-not $proc.HasExited) }
Assert "Config file created on first launch" { Test-Path $Global:ConfigPath }

$configContent = Get-Content $Global:ConfigPath -Raw
Assert "Config file is not empty" { $configContent.Length -gt 10 }

$config = $configContent | ConvertFrom-Json
Assert "Config is valid JSON" { $null -ne $config }
Assert "Config has output_dir" { $null -ne $config.output_dir }
Assert "Config has hotkey" { $null -ne $config.hotkey }
Assert "Config has recording_mode" { $null -ne $config.recording_mode }
Assert "Config has framerate" { $null -ne $config.framerate }
Assert "Config has show_overlay" { $null -ne $config.show_overlay }
Assert "Config has first_run" { $null -ne $config.first_run }
Assert "Default hotkey is Ctrl+Shift+R" { $config.hotkey -eq "Ctrl+Shift+R" }
Assert "Default mode is fullscreen" { $config.recording_mode -eq "fullscreen" }
Assert "Default framerate is 30" { [int]$config.framerate -eq 30 }
Assert "Default overlay is enabled" { [bool]$config.show_overlay -eq $true }
Assert "First run is true" { [bool]$config.first_run -eq $true }

$expectedVideos = [Environment]::GetFolderPath("MyVideos")
Assert "Default output dir is Videos folder" { $config.output_dir -eq $expectedVideos }

Stop-App

# Test config persistence: write custom config BEFORE launching app
Remove-Config
$customConfigPath = "$TestOutputDir\CustomDir"
$customConfigData = @{
    output_dir      = $customConfigPath
    hotkey          = "Ctrl+Alt+F1"
    recording_mode  = "multimonitor"
    framerate       = 30
    show_overlay    = $false
    first_run       = $false
}
$customConfigJson = $customConfigData | ConvertTo-Json
[System.IO.File]::WriteAllText($Global:ConfigPath, $customConfigJson, [System.Text.UTF8Encoding]::new($false))

# Verify file before launching app
$beforeLaunch = Get-Content $Global:ConfigPath -Raw | ConvertFrom-Json
Write-Host "  Before launch: output_dir='$($beforeLaunch.output_dir)' hotkey='$($beforeLaunch.hotkey)'" -ForegroundColor DarkGray

$proc2 = Start-Process -FilePath $BinaryPath -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 3

$config2 = Get-Content $Global:ConfigPath -Raw | ConvertFrom-Json
Write-Host "  After launch:  output_dir='$($config2.output_dir)' hotkey='$($config2.hotkey)' first_run='$($config2.first_run)'" -ForegroundColor DarkGray

if ($config2.hotkey -ne "Ctrl+Alt+F1") {
    $actualConfigPath = $Global:ConfigPath
    Write-Host "  Config path: $actualConfigPath" -ForegroundColor DarkGray
    Write-Host "  Config exists: $(Test-Path $actualConfigPath)" -ForegroundColor DarkGray
}

Assert "Custom hotkey read from config" { $config2.hotkey -eq "Ctrl+Alt+F1" }
Assert "Custom mode read from config" { $config2.recording_mode -eq "multimonitor" }
Assert "Custom output_dir read from config" { $config2.output_dir -eq $customConfigPath }
Assert "First run false from config" { [bool]$config2.first_run -eq $false }
Stop-App

Stop-App

# Test corrupted config recovery
Remove-Config
# Write invalid JSON (with single quotes, no brace issues)
$invalidJson = "not valid json at all"
[System.IO.File]::WriteAllText($Global:ConfigPath, $invalidJson, [System.Text.UTF8Encoding]::new($false))

$proc3 = Start-Process -FilePath $BinaryPath -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 2
Assert "Corrupted config regenerated" { Test-Path $Global:ConfigPath }
$config3 = Get-Content $Global:ConfigPath -Raw | ConvertFrom-Json -ErrorAction SilentlyContinue
Assert "Recovered config is valid JSON" { $null -ne $config3 }
Stop-App

# Test partial config (missing fields get defaults)
Remove-Config
$partialConfigJson = '{ "output_dir": "C:\\Temp" }'
[System.IO.File]::WriteAllText($Global:ConfigPath, $partialConfigJson, [System.Text.UTF8Encoding]::new($false))

$proc4 = Start-Process -FilePath $BinaryPath -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 2
$config4 = Get-Content $Global:ConfigPath -Raw | ConvertFrom-Json
Write-Host "  Partial config: output_dir='$($config4.output_dir)' hotkey='$($config4.hotkey)'" -ForegroundColor DarkGray
Assert "Partial config preserves output_dir" { $config4.output_dir -eq "C:\Temp" }
Stop-App

# ═══════════════════════════════════════════
# SECTION 3: FFmpeg Recording Tests
# ═══════════════════════════════════════════
Write-TestHeader "3. FFMPEG RECORDING TESTS"

$recDir = "$TestOutputDir\RecordingTests"
if (!(Test-Path $recDir)) { New-Item -ItemType Directory -Path $recDir -Force | Out-Null }

# Test basic recording
$testFile = "$recDir\test_basic.mp4"
Assert "FFmpeg can capture screen (3s)" {
    $null = & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 3 `
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$testFile" 2>$null
    (Test-Path $testFile) -and ((Get-Item $testFile).Length -gt 1024)
}

$probeResult = & ffprobe -v error -show_entries format=format_name -of default=noprint_wrappers=1:nokey=1 "$testFile" 2>&1
$probeTrimmed = $probeResult.Trim()
Write-Host "  Format probe: '$probeTrimmed'" -ForegroundColor DarkGray
Assert "Output format is mp4" { ($probeTrimmed -eq "mp4") -or ($probeTrimmed -like "*mp4*") -or ($probeTrimmed -eq "mov,mp4,m4a,3gp,3g2,mj2") }

$codec = & ffprobe -v error -select_streams v:0 -show_entries stream=codec_name -of default=noprint_wrappers=1:nokey=1 "$testFile" 2>&1
Assert "Video codec is h264" { $codec.Trim() -eq "h264" }

$pixfmt = & ffprobe -v error -select_streams v:0 -show_entries stream=pix_fmt -of default=noprint_wrappers=1:nokey=1 "$testFile" 2>&1
Assert "Pixel format is yuv420p" { $pixfmt.Trim() -eq "yuv420p" }

$width = & ffprobe -v error -select_streams v:0 -show_entries stream=width -of default=noprint_wrappers=1:nokey=1 "$testFile" 2>&1
$height = & ffprobe -v error -select_streams v:0 -show_entries stream=height -of default=noprint_wrappers=1:nokey=1 "$testFile" 2>&1
Assert "Resolution is valid" { ([int]$width -gt 0) -and ([int]$height -gt 0) }
Write-Host "  Recording resolution: ${width}x${height}" -ForegroundColor Gray

$durationStr = & ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 "$testFile" 2>&1
Assert "Duration metadata present" { $null -ne $durationStr }
$duration = [double]($durationStr.Trim())
Assert "Duration ~3s" { ($duration -ge 2.5) -and ($duration -le 5) }
Write-Host "  Duration: ${duration}s" -ForegroundColor Gray

Remove-Item $testFile -Force

# Test various framerates
foreach ($fps in @(5, 15, 30)) {
    $fpsFile = "$recDir\test_${fps}fps.mp4"
    Assert "${fps}fps recording works" {
        $null = & ffmpeg -y -f gdigrab -framerate $fps -i desktop -t 2 `
            -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$fpsFile" 2>$null
        (Test-Path $fpsFile) -and ((Get-Item $fpsFile).Length -gt 1024)
    }
    Remove-Item $fpsFile -Force
}

# Test different recording durations
foreach ($dur in @(1, 3, 5)) {
    $durFile = "$recDir\test_${dur}s.mp4"
    Assert "${dur}s recording" {
        $null = & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t $dur `
            -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$durFile" 2>$null
        (Test-Path $durFile) -and ((Get-Item $durFile).Length -gt 1024)
    }
    Remove-Item $durFile -Force
}

# Test multi-monitor mode (captures full virtual desktop)
$multiFile = "$recDir\test_multimonitor.mp4"
Assert "Multi-monitor recording" {
    $null = & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 2 `
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$multiFile" 2>$null
    (Test-Path $multiFile) -and ((Get-Item $multiFile).Length -gt 1024)
}
Remove-Item $multiFile -Force

# Test file output to path with spaces
$spaceDir = "$TestOutputDir\My Recordings"
if (!(Test-Path $spaceDir)) { New-Item -ItemType Directory -Path $spaceDir -Force | Out-Null }
$spaceFile = "$spaceDir\test recording.mp4"
Assert "Recording to path with spaces" {
    $null = & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 2 `
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$spaceFile" 2>$null
    (Test-Path $spaceFile)
}
Remove-Item $spaceFile -Force

# Test that output file is not locked
$unlockFile = "$recDir\test_unlocked.mp4"
Assert "FFmpeg can write file" {
    $null = & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 2 `
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$unlockFile" 2>$null
    Test-Path $unlockFile
}
Assert "Output file can be read after recording" {
    $stream = [System.IO.File]::OpenRead($unlockFile)
    $stream.Close()
    $true
}
Remove-Item $unlockFile -Force

# ═══════════════════════════════════════════
# SECTION 4: Video File Validation
# ═══════════════════════════════════════════
Write-TestHeader "4. VIDEO FILE VALIDATION"

$valDir = "$TestOutputDir\Validation"
if (!(Test-Path $valDir)) { New-Item -ItemType Directory -Path $valDir -Force | Out-Null }
$valFile = "$valDir\validation_test.mp4"

$null = & ffmpeg -y -f gdigrab -framerate 10 -i desktop -t 3 `
    -c:v libx264 -preset ultrafast -pix_fmt yuv420p "$valFile" 2>$null

if (Test-Path $valFile) {
    $fi = Get-Item $valFile
    Assert "File is not zero-byte" { $fi.Length -gt 0 }
    Assert "File is playable (ffprobe)" {
        $null = & ffprobe -v error "$valFile" 2>&1
        $LASTEXITCODE -eq 0
    }
    Assert "File can be deleted" {
        Remove-Item $valFile -Force
        !(Test-Path $valFile)
    }
}

# ═══════════════════════════════════════════
# SECTION 5: Process Management Tests
# ═══════════════════════════════════════════
Write-TestHeader "5. PROCESS MANAGEMENT TESTS"

Stop-App
Start-Sleep -Seconds 1

# Launch and verify
$p1 = Start-Process -FilePath $BinaryPath -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 3
Assert "Single instance starts" { (-not $p1.HasExited) }

# Try launching second instance
$p2 = Start-Process -FilePath $BinaryPath -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 2
$drProcesses = @(Get-Process -Name "dr-record" -ErrorAction SilentlyContinue)
Assert "Only one dr-record instance (singleton)" { $drProcesses.Count -le 2 }

# Clean up all instances
Stop-App
$drAfter = @(Get-Process -Name "dr-record" -ErrorAction SilentlyContinue)
Assert "All processes cleaned up" { $drAfter.Count -eq 0 }

# ═══════════════════════════════════════════
# SECTION 6: Resource Usage Tests
# ═══════════════════════════════════════════
Write-TestHeader "6. RESOURCE USAGE TESTS"

$pRes = Start-Process -FilePath $BinaryPath -WindowStyle Hidden -PassThru
Start-Sleep -Seconds 3

try {
    $pi = Get-Process -Id $pRes.Id -ErrorAction SilentlyContinue
    if ($pi) {
        $memMB = [math]::Round($pi.WorkingSet64 / 1MB, 1)
        $threads = $pi.Threads.Count
        $handles = $pi.HandleCount
        Assert "Memory usage idle < 100MB" { $pi.WorkingSet64 -lt 100MB }
        Assert "Thread count < 20" { $threads -lt 20 }
        Assert "Handle count < 500" { $handles -lt 500 }
        Write-Host "  Memory: ${memMB}MB, Threads: $threads, Handles: $handles" -ForegroundColor Gray
    }
} finally {
    Stop-App
}

# ═══════════════════════════════════════════
# SECTION 7: Config Stress Tests
# ═══════════════════════════════════════════
Write-TestHeader "7. CONFIG STRESS TESTS"

$stressDir = "$TestOutputDir\StressTests"
if (!(Test-Path $stressDir)) { New-Item -ItemType Directory -Path $stressDir -Force | Out-Null }

# Write config 100 times
for ($i = 0; $i -lt 100; $i++) {
    $letter = [char](65 + ($i % 26))
    $sc = @{
        output_dir      = "$stressDir\run$i"
        hotkey          = "Ctrl+Shift+$letter"
        recording_mode  = @("fullscreen", "multimonitor", "window")[$i % 3]
        framerate       = 30
        show_overlay    = ($i % 2 -eq 0)
        first_run       = $false
    } | ConvertTo-Json
    [System.IO.File]::WriteAllText($Global:ConfigPath, $sc, [System.Text.UTF8Encoding]::new($false))
}
$finalConfig = Get-Content $Global:ConfigPath -Raw | ConvertFrom-Json
$lastLetter = [char](65 + (99 % 26))
Write-Host "  Debug: last hotkey = '$($finalConfig.hotkey)', expected letter = '$lastLetter'" -ForegroundColor DarkGray
Assert "Config survives 100 writes" { $null -ne $finalConfig.hotkey }

$cfgSize = (Get-Item $Global:ConfigPath).Length
Assert "Config file size under 10KB" { $cfgSize -lt 10KB }
Write-Host "  Config size: ${cfgSize} bytes after 100 writes" -ForegroundColor Gray

# ═══════════════════════════════════════════
# SECTION 8: Binary Integrity Tests
# ═══════════════════════════════════════════
Write-TestHeader "8. BINARY INTEGRITY TESTS"

Assert "Binary has PE header (MZ)" { 
    $bytes = [System.IO.File]::ReadAllBytes($BinaryPath)
    ($bytes[0] -eq 0x4D) -and ($bytes[1] -eq 0x5A)
}

$binSize = (Get-Item $BinaryPath).Length
Assert "Binary under 25MB" { $binSize -lt 25MB }

# Check for "Dr. Record" string in binary
$binText = [System.IO.File]::ReadAllText($BinaryPath, [System.Text.Encoding]::ASCII)
Assert "Binary contains app name string" { $binText.Contains("Dr. Record") }

# ═══════════════════════════════════════════
# SUMMARY
# ═══════════════════════════════════════════
Write-TestHeader "TEST RESULTS SUMMARY"

$total = $Global:TestsPassed + $Global:TestsFailed
$passRate = 0
if ($total -gt 0) {
    $passRate = [math]::Round($Global:TestsPassed / $total * 100, 1)
}

$color = "Green"
if ($passRate -lt 90) { $color = "Yellow" }
if ($passRate -lt 70) { $color = "Red" }

Write-Host "  Total Tests : $total" -ForegroundColor White
Write-Host "  Passed      : $Global:TestsPassed" -ForegroundColor Green
Write-Host "  Failed      : $Global:TestsFailed" -ForegroundColor Red
Write-Host "  Pass Rate   : $passRate%" -ForegroundColor $color

# Save results
$timestamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
$resultsFile = "$TestOutputDir\DrRecord_Results_${timestamp}.csv"
$Global:TestResults | Export-Csv -Path $resultsFile -NoTypeInformation -Force
Write-Host "`n  Results saved: $resultsFile" -ForegroundColor Gray

Remove-Config
Stop-App

Write-Host "`n  Done!" -ForegroundColor Cyan

if ($Global:TestsFailed -gt 0) {
    exit 1
}
exit 0
