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

# Extract version from Cargo.toml
Write-Status "Extracting version from Cargo.toml..."
$CARGO_TOML_PATH = "Cargo.toml"
if (-not (Test-Path $CARGO_TOML_PATH)) {
    Write-Error-Custom "Cargo.toml not found"
    exit 1
}

# Extract version from package.version field
$CARGO_VERSION = Select-String -Path $CARGO_TOML_PATH -Pattern '^version\s*=\s*"([^"]+)"' | ForEach-Object { $_.Matches[0].Groups[1].Value }
if (-not $CARGO_VERSION) {
    Write-Error-Custom "Could not extract version from Cargo.toml"
    exit 1
}

Write-Status "Found version in Cargo.toml: $CARGO_VERSION"

# Check if git tag exists and matches current HEAD
$TAG_NAME = "v$CARGO_VERSION"
$CURRENT_COMMIT = git rev-parse HEAD

# Try to get the tag commit, gracefully handle if tag doesn't exist
$TAG_COMMIT = git rev-parse "refs/tags/$TAG_NAME" 2>$null
$TAG_COMMAND_SUCCESS = $LASTEXITCODE -eq 0
$TAG_EXISTS = $TAG_COMMAND_SUCCESS -and $TAG_COMMIT

if ($TAG_EXISTS -and ($TAG_COMMIT -eq $CURRENT_COMMIT)) {
    # Tag exists and matches current commit
    $VERSION = $TAG_NAME
    Write-Status "Git tag $TAG_NAME exists and matches current commit"
}
else {
    # Tag doesn't exist or doesn't match current commit - assume dev commit
    $VERSION = "$TAG_NAME-dev"
    if (-not $TAG_EXISTS) {
        Write-Status "Git tag $TAG_NAME does not exist, assuming dev commit"
    }
    else {
        Write-Status "Git tag $TAG_NAME exists but does not match current commit, assuming dev commit"
    }
}

Write-Status "Building version: $VERSION"

# Create VERSION_RAW by stripping v prefix and any - suffix from VERSION
$VERSION_RAW = $VERSION -replace '^v', '' -replace '-.*$', ''
Write-Status "Raw version (for NSIS): $VERSION_RAW"

# Download VC Redistributable
Write-Status "Downloading Visual C++ Redistributable..."
New-Item -ItemType Directory -Force -Path build-resources/downloads | Out-Null
$vcRedistPath = "build-resources/downloads/vc_redist.x64.exe"
if (-not (Test-Path $vcRedistPath)) {
    $ProgressPreference = 'SilentlyContinue'
    Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vc_redist.x64.exe" -OutFile $vcRedistPath
    Write-Status "VC Redistributable downloaded"
}
else {
    Write-Status "VC Redistributable already exists, skipping download"
}

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

# Copy assets
Write-Status "Copying assets..."
Copy-Item -Path assets -Destination dist\assets -Recurse

# Install OBS dependency
Write-Status "Installing OBS dependencies..."
cargo install cargo-obs-build
if ($LASTEXITCODE -eq 0) {
    Write-Status "cargo-obs-build installed successfully"
}
else {
    Write-Error-Custom "cargo-obs-build installation failed"
    exit 1
}

cargo obs-build --out-dir dist\
if ($LASTEXITCODE -eq 0) {
    Write-Status "OBS dependencies installed successfully"
}
else {
    Write-Error-Custom "OBS dependencies install failed"
    exit 1
}

.\build-resources\scripts\clean_obs_build.ps1

# Copy Rust binary
Write-Status "Copying Rust binary..."
$RUST_BINARY = "target\x86_64-pc-windows-msvc\release\owl-control.exe"
if (Test-Path $RUST_BINARY) {
    Copy-Item -Path $RUST_BINARY -Destination "dist\OWL Control.exe"
}
else {
    Write-Error-Custom "Rust binary not found at $RUST_BINARY"
    # Try to find any .exe in release directory
    $FOUND_BINARY = Get-ChildItem -Path "target\x86_64-pc-windows-msvc\release" -Filter "*.exe" -File | Select-Object -First 1
    if ($FOUND_BINARY) {
        Write-Warning-Custom "Using binary: $($FOUND_BINARY.FullName)"
        Copy-Item -Path $FOUND_BINARY.FullName -Destination "dist\OWL Control.exe"
    }
    else {
        Write-Error-Custom "No executable found in release directory"
        exit 1
    }
}


# Copy additional resources
Write-Status "Copying additional resources..."
if (Test-Path README.md) {
    Copy-Item -Path README.md -Destination dist\README.md
}
if (Test-Path LICENSE) {
    Copy-Item -Path LICENSE -Destination dist\LICENSE
}

# Create installer with NSIS if available
Write-Status "Looking for NSIS makensis..."

# First, try to find makensis in PATH
$NSIS_PATH = $null
try {
    $makensisInPath = Get-Command "makensis" -ErrorAction Stop
    $NSIS_PATH = $makensisInPath.Source
    Write-Status "Found makensis in PATH: $NSIS_PATH"
}
catch {
    # If not found in PATH, try the default installation directory
    $defaultNsisPath = "C:\Program Files (x86)\NSIS\Bin\makensis.exe"
    if (Test-Path $defaultNsisPath) {
        $NSIS_PATH = $defaultNsisPath
        Write-Status "Found makensis in default directory: $NSIS_PATH"
    }
    else {
        Write-Warning-Custom "makensis not found in PATH or default directory ($defaultNsisPath)"
        exit 1
    }
}

Write-Status "Creating NSIS installer using: $NSIS_PATH"
& $NSIS_PATH /DVERSION="$VERSION" /DVERSION_RAW="$VERSION_RAW" build-resources/installer.nsi
if ($LASTEXITCODE -eq 0) {
    Write-Status "Installer created successfully"
}
else {
    Write-Error-Custom "NSIS installer creation failed"
    exit 1
}

Write-Status "Build completed successfully!"
Write-Host "======================================" -ForegroundColor Cyan
Write-Host "Output directory: dist\" -ForegroundColor Cyan
Write-Host "======================================" -ForegroundColor Cyan