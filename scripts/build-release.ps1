<#
.SYNOPSIS
    Dr. Record — Full Release Build Script
.DESCRIPTION
    Downloads FFmpeg, builds the frontend and Tauri app, and produces
    MSI + NSIS installers with FFmpeg bundled.
#>

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $PSScriptRoot
$ResourceDir = "$ProjectRoot\resources\ffmpeg"

Write-Host "╔══════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║   Dr. Record — Release Build Script      ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════╝" -ForegroundColor Cyan

# ─── Step 1: Download FFmpeg ──────────────────────────────────────────
Write-Host "`n[1/4] Downloading FFmpeg..." -ForegroundColor Yellow
New-Item -ItemType Directory -Path $ResourceDir -Force | Out-Null

$ffmpegExe = "$ResourceDir\ffmpeg.exe"
if (-not (Test-Path $ffmpegExe)) {
    $url = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"
    $zip = "$env:TEMP\ffmpeg-essentials.zip"

    Write-Host "  Downloading from gyan.dev (essentials build)..."
    Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing -TimeoutSec 300

    Write-Host "  Extracting ffmpeg.exe..."
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($zip)
    $entry = $archive.Entries | Where-Object { $_.Name -eq "ffmpeg.exe" } | Select-Object -First 1
    if ($entry) {
        [System.IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $ffmpegExe, $true)
        Write-Host "  FFmpeg extracted: $( (Get-Item $ffmpegExe).Length / 1MB ) MB" -ForegroundColor Green
    } else {
        throw "Could not find ffmpeg.exe in the downloaded zip"
    }
    $archive.Dispose()
    Remove-Item $zip -Force
} else {
    Write-Host "  FFmpeg already present: $( (Get-Item $ffmpegExe).Length / 1MB ) MB" -ForegroundColor Green
}

# ─── Step 2: Install npm dependencies ──────────────────────────────────
Write-Host "`n[2/4] Installing npm dependencies..." -ForegroundColor Yellow
Set-Location $ProjectRoot
npm install

# ─── Step 3: Build frontend ──────────────────────────────────────────
Write-Host "`n[3/4] Building frontend with Vite..." -ForegroundColor Yellow
npm run build

# ─── Step 4: Build Tauri app + installer ─────────────────────────────
Write-Host "`n[4/4] Building Tauri app + installer..." -ForegroundColor Yellow
npm run tauri build

# ─── Done ────────────────────────────────────────────────────────────
Write-Host "`n══════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "  BUILD COMPLETE" -ForegroundColor Green
Write-Host "══════════════════════════════════════════" -ForegroundColor Cyan

$bundles = Get-ChildItem "$ProjectRoot\src-tauri\target\release\bundle" -Recurse -Include "*.exe","*.msi"
foreach ($b in $bundles) {
    $size = [math]::Round($b.Length / 1MB, 1)
    Write-Host "  $($b.Name)  —  ${size}MB" -ForegroundColor Green
}

Set-Location $ProjectRoot
