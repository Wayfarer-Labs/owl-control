# OWL Control - Windows Development Environment Setup Script
# Run this script in PowerShell as Administrator inside the Windows VM

Write-Host "============================================" -ForegroundColor Cyan
Write-Host "OWL Control - Windows Dev Environment Setup" -ForegroundColor Cyan
Write-Host "============================================" -ForegroundColor Cyan
Write-Host ""

# Check if running as administrator
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not $isAdmin) {
    Write-Host "ERROR: This script must be run as Administrator." -ForegroundColor Red
    Write-Host "Right-click PowerShell and select 'Run as Administrator'" -ForegroundColor Yellow
    exit 1
}

# Install Chocolatey
Write-Host "Step 1: Installing Chocolatey package manager" -ForegroundColor Green
Write-Host "----------------------------------------------" -ForegroundColor Green

if (!(Get-Command choco -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Chocolatey..."
    Set-ExecutionPolicy Bypass -Scope Process -Force
    [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072
    Invoke-Expression ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
    
    # Refresh environment
    $env:ChocolateyInstall = Convert-Path "$((Get-Command choco).Path)\..\.."
    Import-Module "$env:ChocolateyInstall\helpers\chocolateyProfile.psm1"
    refreshenv
} else {
    Write-Host "Chocolatey is already installed."
}

Write-Host ""

# Install Git
Write-Host "Step 2: Installing Git for Windows" -ForegroundColor Green
Write-Host "-----------------------------------" -ForegroundColor Green

if (!(Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Git..."
    choco install git -y
    refreshenv
} else {
    Write-Host "Git is already installed."
}

Write-Host ""

# Install Rust
Write-Host "Step 3: Installing Rust toolchain" -ForegroundColor Green
Write-Host "----------------------------------" -ForegroundColor Green

if (!(Get-Command rustc -ErrorAction SilentlyContinue)) {
    Write-Host "Installing Rust via rustup..."
    
    # Download rustup-init
    $rustupInit = "$env:TEMP\rustup-init.exe"
    Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $rustupInit
    
    # Run rustup-init with default settings
    & $rustupInit -y --default-toolchain stable --profile default
    
    # Remove installer
    Remove-Item $rustupInit
    
    # Add to PATH for current session
    $env:Path += ";$env:USERPROFILE\.cargo\bin"
} else {
    Write-Host "Rust is already installed."
}

Write-Host ""

# Install Visual Studio Build Tools
Write-Host "Step 4: Installing Visual Studio Build Tools" -ForegroundColor Green
Write-Host "---------------------------------------------" -ForegroundColor Green

$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$hasBuildTools = $false

if (Test-Path $vsWhere) {
    $buildTools = & $vsWhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($buildTools) {
        Write-Host "Visual Studio Build Tools already installed at: $buildTools"
        $hasBuildTools = $true
    }
}

if (-not $hasBuildTools) {
    Write-Host "Installing Visual Studio Build Tools..."
    Write-Host "This will take several minutes. Please be patient."
    
    # Download VS Build Tools installer
    $vsInstaller = "$env:TEMP\vs_buildtools.exe"
    Invoke-WebRequest -Uri "https://aka.ms/vs/17/release/vs_buildtools.exe" -OutFile $vsInstaller
    
    # Install with C++ workload
    Write-Host "Running Visual Studio Build Tools installer..."
    Start-Process -FilePath $vsInstaller -ArgumentList "--quiet", "--wait", "--norestart", "--nocache", `
        "--add", "Microsoft.VisualStudio.Workload.VCTools", `
        "--add", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", `
        "--add", "Microsoft.VisualStudio.Component.Windows11SDK.22000" `
        -Wait
    
    Remove-Item $vsInstaller
    Write-Host "Visual Studio Build Tools installed."
} else {
    Write-Host "Build tools are already installed."
}

Write-Host ""

# Install cargo-obs-build
Write-Host "Step 5: Installing cargo-obs-build" -ForegroundColor Green
Write-Host "-----------------------------------" -ForegroundColor Green

# Refresh cargo path
$env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")

if (!(Get-Command cargo-obs-build -ErrorAction SilentlyContinue)) {
    Write-Host "Installing cargo-obs-build..."
    cargo install cargo-obs-build
} else {
    Write-Host "cargo-obs-build is already installed."
}

Write-Host ""

# Download OBS binaries
Write-Host "Step 6: Downloading OBS Studio binaries" -ForegroundColor Green
Write-Host "----------------------------------------" -ForegroundColor Green

# Check if we're in the owl-control directory
if (Test-Path ".\Cargo.toml") {
    Write-Host "Found owl-control project. Downloading OBS binaries..."
    
    # Create target directory if it doesn't exist
    $targetDir = ".\target\x86_64-pc-windows-msvc\debug"
    if (!(Test-Path $targetDir)) {
        Write-Host "Creating target directory..."
        cargo build 2>$null
    }
    
    if (Test-Path $targetDir) {
        Write-Host "Installing OBS binaries to $targetDir..."
        cargo obs-build build --out-dir $targetDir
        Write-Host "OBS binaries installed."
    } else {
        Write-Host "WARNING: Could not create target directory." -ForegroundColor Yellow
        Write-Host "You may need to run this manually after first build:" -ForegroundColor Yellow
        Write-Host "  cargo obs-build build --out-dir target\x86_64-pc-windows-msvc\debug" -ForegroundColor Yellow
    }
} else {
    Write-Host "Not in owl-control directory. Skipping OBS binary installation."
    Write-Host "Run this command from the owl-control directory after cloning:" -ForegroundColor Yellow
    Write-Host "  cargo obs-build build --out-dir target\x86_64-pc-windows-msvc\debug" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "============================================" -ForegroundColor Green
Write-Host "Setup Complete!" -ForegroundColor Green
Write-Host "============================================" -ForegroundColor Green
Write-Host ""
Write-Host "Development environment is ready. You can now:" -ForegroundColor Cyan
Write-Host "  1. Navigate to the shared folder (Z:\)" -ForegroundColor White
Write-Host "  2. Run: cargo build" -ForegroundColor White
Write-Host "  3. Run: cargo run" -ForegroundColor White
Write-Host ""
Write-Host "For more information, see CONTRIBUTING.md" -ForegroundColor Cyan
Write-Host ""
