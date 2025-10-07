#Requires -Version 5.1
$ErrorActionPreference = "Stop"

# Colors for output
function Write-Status {
    param([string]$Message)
    Write-Host "[*] $Message" -ForegroundColor Green
}

function Write-Error-Custom {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

function Write-Warning-Custom {
    param([string]$Message)
    Write-Host "[WARNING] $Message" -ForegroundColor Yellow
}

Write-Host "======================================" -ForegroundColor Cyan
Write-Host "Building OWL Control Application" -ForegroundColor Cyan
Write-Host "======================================" -ForegroundColor Cyan

# Get version from git tag or use default
$VERSION = if ($env:GITHUB_REF_NAME) { $env:GITHUB_REF_NAME } else { "dev" }
Write-Status "Building version: $VERSION"

# Install uv if not already installed
Write-Status "Installing uv..."
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    irm https://astral.sh/uv/install.ps1 | iex
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
}

# Setup Python with uv
Write-Status "Setting up Python environment..."
uv python install 3.12
uv sync

# Build Rust application
Write-Status "Building Rust application..."
cargo build --release --target x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) {
    Write-Error-Custom "Rust build failed"
    exit 1
}

# Create distribution directory structure
Write-Status "Creating distribution directory..."
if (Test-Path dist) {
    Remove-Item -Path dist -Recurse -Force
}
New-Item -ItemType Directory -Force -Path dist | Out-Null
New-Item -ItemType Directory -Force -Path dist\resources\ | Out-Null

# Copy Rust binary
Write-Status "Copying Rust binary..."
$RUST_BINARY = "target\x86_64-pc-windows-msvc\release\owl-recorder.exe"
if (Test-Path $RUST_BINARY) {
    Copy-Item -Path $RUST_BINARY -Destination "dist\OWL Control.exe"
} else {
    Write-Error-Custom "Rust binary not found at $RUST_BINARY"
    # Try to find any .exe in release directory
    $FOUND_BINARY = Get-ChildItem -Path "target\x86_64-pc-windows-msvc\release" -Filter "*.exe" -File | Select-Object -First 1
    if ($FOUND_BINARY) {
        Write-Warning-Custom "Using binary: $($FOUND_BINARY.FullName)"
        Copy-Item -Path $FOUND_BINARY.FullName -Destination "dist\OWL Control.exe"
    } else {
        Write-Error-Custom "No executable found in release directory"
        exit 1
    }
}

# Copy Python environment
Write-Status "Copying Python environment..."
Copy-Item -Path vg_control -Destination dist\resources\vg_control -Recurse
Copy-Item -Path pyproject.toml -Destination dist\resources\pyproject.toml
Copy-Item -Path uv.lock -Destination dist\resources\uv.lock

# Copy uv executable
Write-Status "Copying uv executable..."
$UV_PATH = (Get-Command uv -ErrorAction SilentlyContinue).Source
if ($UV_PATH -and (Test-Path $UV_PATH)) {
    Copy-Item -Path $UV_PATH -Destination dist\resources\uv.exe
} else {
    # Try common locations
    $UV_LOCATIONS = @(
        "$env:USERPROFILE\.cargo\bin\uv.exe",
        "$env:LOCALAPPDATA\Programs\uv\uv.exe"
    )
    $UV_FOUND = $false
    foreach ($location in $UV_LOCATIONS) {
        if (Test-Path $location) {
            Copy-Item -Path $location -Destination dist\resources\uv.exe
            $UV_FOUND = $true
            break
        }
    }
    if (-not $UV_FOUND) {
        Write-Warning-Custom "uv executable not found"
    }
}

# Build OBS plugin
Write-Status "Building OBS plugin..."
try {
    # cargo obs-build --out-dir target\x86_64-pc-windows-msvc\release\
    cargo obs-build --out-dir dist\
    if ($LASTEXITCODE -eq 0) {
        Write-Status "OBS plugin built successfully"
    } else {
        Write-Warning-Custom "OBS plugin build failed, continuing..."
    }
} catch {
    Write-Warning-Custom "OBS plugin build command not found, continuing..."
}

# Copy OBS binaries if they exist
# Write-Status "Copying OBS binaries..."
# if (Test-Path "target\x86_64-pc-windows-msvc\release") {
#     Copy-Item -Path "target\x86_64-pc-windows-msvc\release\obs-build\*" -Destination dist\resources\obs\ -Recurse -Force
#     Write-Status "OBS binaries copied"
# } else {
#     Write-Warning-Custom "No OBS binaries found at target\obs-build"
# }

# Copy additional resources
Write-Status "Copying additional resources..."
if (Test-Path README.md) {
    Copy-Item -Path README.md -Destination dist\resources\README.md
}
if (Test-Path LICENSE) {
    Copy-Item -Path LICENSE -Destination dist\resources\LICENSE
}

# Clean up Python cache files
Write-Status "Cleaning up Python cache files..."
Get-ChildItem -Path dist\resources\vg_control -Recurse -Directory -Filter "__pycache__" -ErrorAction SilentlyContinue | Remove-Item -Recurse -Force
Get-ChildItem -Path dist\resources\vg_control -Recurse -File -Filter "*.pyc" -ErrorAction SilentlyContinue | Remove-Item -Force

# Create installer with NSIS if available
if (Get-Command makensis -ErrorAction SilentlyContinue) {
    Write-Status "Creating NSIS installer..."
    if (Test-Path "installer.nsi") {
        makensis /DVERSION="$VERSION" installer.nsi
        if ($LASTEXITCODE -eq 0) {
            Write-Status "Installer created successfully"
        } else {
            Write-Warning-Custom "NSIS installer creation failed"
        }
    } else {
        Write-Warning-Custom "installer.nsi not found, skipping installer creation"
    }
} else {
    Write-Warning-Custom "NSIS not installed, skipping installer creation"
}

Write-Status "Build completed successfully!"
Write-Host "======================================" -ForegroundColor Cyan
Write-Host "Output directory: dist\" -ForegroundColor Cyan
Write-Host "======================================" -ForegroundColor Cyan