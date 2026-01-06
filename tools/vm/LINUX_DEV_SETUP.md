# Linux Development Setup for OWL Control

OWL Control targets Windows exclusively due to its dependencies on Windows-specific APIs, OBS Studio integration, and raw input capture. However, Linux developers can efficiently develop and test using a Windows VM.

This guide provides a semi-automated setup for QEMU/KVM with shared folders, allowing you to edit code on your Linux workstation and seamlessly test in Windows.

![Successful build and run inside windows VM](./owl_it_works.jpg)

## Overview

The setup process involves:
1. Running the main setup script (owl-dev-setup.sh) to install prerequisites and configure Samba
2. Manually creating and configuring the VM in virt-manager
3. Installing Windows 11 in the VM
4. Setting up the Windows development environment
5. Building and testing the application

## Quick Start

```bash
git clone https://github.com/Wayfarer-Labs/owl-control.git
cd owl-control/tools/vm
./owl-dev-setup.sh
```

Select option 1 to install prerequisites, then follow the detailed steps below for VM creation and setup.

## Prerequisites

- Debian 13 (or similar Debian-based distribution)
- Enough free disk space for the VM (used the default 128GB)
- 8GB+ RAM recommended (VM will use 8GB by default)
- CPU with virtualization support (Intel VT-x or AMD-V)
   - Allocate enough CPU in order to ensure compile time is reasonable
- Windows 11 ISO file (see below)

## Step 1: Install Prerequisites

Clone the repository and run the setup script:

```bash
git clone https://github.com/Wayfarer-Labs/owl-control.git
cd owl-control/tools/vm
./owl-dev-setup.sh
```

This will open an interactive menu. Select option 1 to install prerequisites.

The installation will:
- Check for CPU virtualization support
- Install QEMU/KVM, libvirt, and virt-manager
- Install and configure Samba for file sharing
- Add your user to required groups (libvirt, kvm)
- Start necessary services (libvirtd, smbd)
- Download VirtIO drivers ISO (virtio-win.iso)
- Configure a Samba share for the owl-control directory

After the script completes, you may need to log out and back in for group membership changes to take effect.

## Step 2: Set Up Samba Password

Before proceeding, set up a Samba password for your user account:

```bash
sudo smbpasswd -a <your-username>
```

You will be prompted to enter a password. Choose something you can remember, as you will need it later when mapping the network drive in Windows.

## Step 3: Obtain Windows 11 ISO

Microsoft no longer provides free evaluation VMs, so you will need to download a Windows 11 ISO:

1. Visit the [Windows 11 download page](https://www.microsoft.com/software-download/windows11)
2. Download the ISO (approximately 6GB)
3. Place it in a known location (e.g., ~/Downloads/Win11_English_x64.iso)

Note: You can use Windows 11 without activation for development and testing. Some personalization features will be limited, but all development tools work fine.

## Step 4: Create Virtual Machine in virt-manager

Open virt-manager and create a new VM:

```bash
virt-manager
```

### VM Creation Steps

1. Click the "Create a new virtual machine" button (top left)

2. Step 1 of 5: Choose installation method
   - Select "Local install media (ISO image or CDROM)"
   - Click "Forward"

3. Step 2 of 5: Choose ISO
   - Click "Browse" and locate your Windows 11 ISO
   - Click "Forward"

4. Step 3 of 5: Memory and CPU
   - Memory: 8192 MB (8GB) is recommended
   - CPUs: 4 is recommended
   - Click "Forward"

5. Step 4 of 5: Storage
   - Select "Enable storage for this virtual machine"
   - Select "Create a disk image for the virtual machine"
   - Disk size: 128 GB recommended
   - Click "Forward"

6. Step 5 of 5: Name and configuration
   - Name: owl-control-vm (or your preferred name)
   - IMPORTANT: Check "Customize configuration before install"
   - Click "Finish"

### Add VirtIO Drivers CD

Before starting the VM, you need to attach the VirtIO drivers ISO:

1. In the VM configuration window, click "Add Hardware" (bottom left)
2. Select "Storage"
3. Select "Select or create custom storage"
4. Click "Manage" and browse to `/var/lib/libvirt/images/virtio-win.iso`
5. Change "Device type" to "CDROM device"
6. Click "Finish"

### Start Installation

1. Click "Begin Installation" (top left)
2. VERY IMPORTANT: When the window appears and says "Press any key to boot from CD or DVD", press any key immediately
3. If you miss it, the VM will fail to boot. Just restart the VM and try again.

## Step 5: Install Windows 11

Follow the Windows installation wizard:

### Initial Setup

1. Language settings: Defaults are fine, click "Next"
2. Keyboard settings: Defaults are fine, click "Next"
3. Setup Option: Select "Install Windows 11", click "I agree everything will be deleted", then "Next"

### Product Key and Edition

1. Product Key: Click "I don't have a product key" (bottom left)
2. Select Image: Choose any edition (Windows 11 Pro, Pro Education, etc.), click "Next"
3. Accept license terms, click "Next"

### Disk Setup

1. You should see the default disk you created earlier
2. Click "Next"
3. Click "Install"

The installation will take some time depending on your hardware allocation. Be patient.

### Windows Out-of-Box Experience (OOBE)

After installation completes, Windows will guide you through initial setup:

1. Select your country, click "Next"
2. Select keyboard layout, click "Next"
3. Skip extra keyboard layout

Wait a moment for Windows to finish preparations.

4. Name your device whatever you want, click "Next"
5. Choose "Set up for personal use", click "Next"

Note: This step will show "Downloading..." and can take a very long time. The progress from 97% to 100% is extremely slow but it is not stuck. Be patient.

### Microsoft Account Setup

Unfortunately, Windows 11 requires a Microsoft account for setup:

1. Click "Sign in with a Microsoft account"
2. Sign in with your Microsoft account credentials
3. Create a PIN when prompted

### Privacy Settings

1. Choose privacy settings (recommend turning everything off), click "Next"
2. When prompted to restore from a previous PC, select "Set up as a new PC" (you need to click this twice for some reason)
3. "Use your mobile device from your PC": Click "Skip"
4. "Back up your phone's photos": Click "Skip"
5. "Always have access to your recent browsing data": Click "Not Now"
6. Decline Microsoft 365 offer
7. Decline OneDrive storage offer
8. Unselect your email account, click "Next"
9. "Join Game Pass": Click "Skip"

You should now see the Windows desktop.

## Step 6: Install VirtIO Guest Tools

The VirtIO guest tools provide proper drivers for networking, graphics, and improved performance:

1. Open File Explorer
2. Navigate to the drive that has the virtio-win.iso mounted (usually D: or E:)
3. Run `virtio-win-guest-tools.exe`
4. Follow the installation wizard
5. After clicking "Finished", Restart the Windows VM

After reboot, clipboard sharing between the host and VM should work, and network performance will be significantly improved.

## Step 7: Map the Shared Folder

- A lot of commands are going to rely on PowerShell as an Admin, for conveinience, it is very handy to pin it to the task bar so all one needs to do is right-click and select "Run as Administrator".

- Open PowerShell as Administrator (right-click Start menu, select "Windows PowerShell (Admin)"):

```powershell
net use Z: \\192.168.122.1\owl-control /persistent:yes
```

Note: `192.168.122.1` is the default IP address that libvirt assigns to the host machine on the virtual network. This is standard for all libvirt installations and does not need to be configured.

You will be prompted for credentials:
- Username: Your Linux username
- Password: The Samba password you set earlier with smbpasswd

After entering the correct credentials, you should see "The command completed successfully."

Verify the share is accessible:

```powershell
cd Z:
dir
```

You should see the contents of your owl-control directory.

## Step 8: Set Up Windows Development Environment

Before running the setup script, you need to allow PowerShell scripts to execute:

```powershell
Set-ExecutionPolicy -ExecutionPolicy Bypass -Scope Process -Force
```

Now run the Windows setup script:

```powershell
cd Z:\tools\vm
.\owl-windows-dev-setup.ps1
```

This script will install:
- Chocolatey package manager
- Git for Windows
- Rust toolchain (rustup)
- Visual Studio Build Tools with C++ workload
- CMake (required for building dependencies)

The installation process was fairly brief.

After the script completes, restart Windows.

### Configure Visual Studio Build Tools

After reboot, you need to ensure all required components are installed:

1. Change display resolution to 1920x1080 for better visibility, failure to do so will result in come windows being strangely cut off and being non-functional
2. Open Visual Studio Installer from the Start menu
3. Click "Modify" on Build Tools 2022
4. Verify that "Windows 11 SDK" is checked
5. If not, check it and click "Modify"
6. After doing so I still had an issue with the build and that was resolved by using the Visual Studio installer GUI and selecting "repair", followed by another restart

### Install Additional Dependencies

Open PowerShell as Administrator and install remaining dependencies:

```powershell
# Install CMake via Chocolatey
choco install cmake -y
```

Close and reopen PowerShell after CMake installation.

## Step 9: Clone Repository Locally

Because Windows security restrictions prevent executing files directly from network shares, you need to clone the repository to a local directory:

```powershell
cd C:\
mkdir projects
cd projects
git clone Z:\ owl-control
cd owl-control
```

I encoutered an issue because for some reason it did not copy hidden files by default which mean .git is missing, once .get was in the directory it worked.

This creates a local copy on the C: drive where builds can execute properly.

## Step 10: Install OBS Dependencies

The application requires OBS Studio libraries. Install them using cargo-obs-build:

```powershell
# Install cargo-obs-build
cargo install cargo-obs-build

# Download and install OBS binaries
cargo obs-build build --out-dir target\x86_64-pc-windows-msvc\debug
```

These commmands for whatever reason did not create all of the needed outputs, but that was resolved by running:

```powershell
cargo-obs-build.exe build --out-dir target\x86_64-pc-windows-msvc\debug 
```
- Change debug to release if you desire that version of the build.

This downloads the required OBS DLLs and data files and places them in the correct location for the application.

## Step 11: Build and Test

Now you can build and run the application:

```powershell
cargo build
cargo run
```

For optimized release builds:

```powershell
cargo build --release
cargo run --release
```

If the application starts and shows the UI window, congratulations! Your development environment is working.

## Development Workflow

With the setup complete, you can follow this workflow:

### Option A: Edit on Linux, Build on Windows

1. Edit source files on your Linux workstation using your preferred editor
2. Changes are immediately visible on the Z: drive in Windows
3. In Windows VM, copy changed files to your local C:\projects\owl-control directory:
   ```powershell
   # Copy specific changed files
   copy Z:\src\main.rs C:\projects\owl-control\src\main.rs
   
   # Or copy entire src directory
   xcopy Z:\src\*.* C:\projects\owl-control\src\ /E /Y
   ```
4. Build and test in C:\projects\owl-control

### Option B: Edit and Build in Windows

1. Edit files directly in C:\projects\owl-control using your preferred Windows editor
2. Build and test locally
3. When ready to commit, copy changes back to Z: drive:
   ```powershell
   xcopy C:\projects\owl-control\src\*.* Z:\src\ /E /Y
   ```
4. Commit from your Linux host

### Option C: Use Git in Windows

1. Work entirely in C:\projects\owl-control
2. Use Git for Windows to commit and push changes
3. Pull changes on your Linux host when needed

## VM Management

### Starting the VM

```bash
# From command line
virsh start owl-control-vm

# Or use virt-manager GUI
virt-manager
```

### Stopping the VM

```bash
# Graceful shutdown
virsh shutdown owl-control-vm

# Force stop (if needed)
virsh destroy owl-control-vm
```

### Accessing the VM Console

```bash
virt-manager
# Then double-click the VM name
```

## Helper Scripts

Three helper scripts are provided for managing your setup:

### owl-dev-setup.sh (Main Entry Point)

Interactive menu for common operations:
- Option 1: Install prerequisites (runs owl-install-prerequisites.sh)
- Option 2: View setup instructions
- Option 3: Clean up VM (runs owl-cleanup.sh)
- Option 4: Exit

This is the main script you should use. It provides a convenient interface for managing your development environment.

```bash
./owl-dev-setup.sh
```

### owl-install-prerequisites.sh

Installs and configures all Linux-side prerequisites:
- QEMU/KVM and virtualization tools
- Samba file sharing
- Downloads VirtIO drivers

This script is idempotent and can be run multiple times safely. It is called by owl-dev-setup.sh option 1, but can also be run directly if needed.

### owl-cleanup.sh

Completely removes the VM and all associated files:
- Stops and undefines the VM
- Deletes disk images (60-128GB freed)
- Removes NVRAM and TPM data
- Cleans up libvirt state

This script is called by owl-dev-setup.sh option 3, but can also be run directly:

```bash
./owl-cleanup.sh
```

Note: This does NOT remove Samba configuration or installed packages, only VM-specific files.

## Troubleshooting

### Common Issues

**PowerShell execution policy errors:**
Always run PowerShell as Administrator and use:
```powershell
Set-ExecutionPolicy -ExecutionPolicy Bypass -Scope Process -Force
```

**Missing link.exe or kernel32.lib errors:**
- Visual Studio Build Tools is not fully installed
- Open Visual Studio Installer and ensure Windows 11 SDK is installed
- Try the "Repair" option in Visual Studio Installer

**CMake not found:**
- Install via Chocolatey: `choco install cmake -y`
- Close and reopen PowerShell after installation

**Access denied when running cargo build:**
- You are trying to build on the Z: network drive
- Clone the repository to C:\projects\owl-control and build there

**Git repository errors (git describe failed):**
- The .git directory was not copied
- Use `git clone Z:\ owl-control` to properly clone with git metadata
- Or initialize a new repo: `git init && git add . && git commit -m "initial"`

**OBS DLL errors (STATUS_DLL_NOT_FOUND):**
- Run: `cargo-obs-build.exe build --out-dir target\x86_64-pc-windows-msvc\debug`
- Ensure the command completes successfully

**Display issues in virt-manager or Visual Studio Installer:**
- Increase Windows display resolution to 1920x1080 or higher
- Use fullscreen mode in virt-manager (F11 key)

**Random unexplained issues:**
- Try restarting the Windows VM
- Many Windows issues resolve after a reboot

### Virtualization Not Available

If you get errors about KVM/virtualization:

```bash
# Check if virtualization is enabled
egrep -c '(vmx|svm)' /proc/cpuinfo
# Should return a number > 0

# If it returns 0, enable VT-x/AMD-V in your BIOS/UEFI
```

### Network Issues

**Cannot ping 192.168.122.1 from Windows:**
1. Verify libvirt network is running:
   ```bash
   virsh net-list --all
   virsh net-start default
   ```
2. Check firewall on Linux host

**Samba share not accessible:**
1. Verify Samba is running:
   ```bash
   sudo systemctl status smbd
   ```
2. Check share configuration:
   ```bash
   sudo testparm -s | grep owl-control
   ```
3. Ensure Samba password is set:
   ```bash
   sudo smbpasswd -a <your-username>
   ```

### Performance Issues

**Slow VM performance:**
- Ensure VirtIO guest tools are installed
- Verify KVM acceleration is enabled:
  ```bash
  virsh dumpxml owl-control-vm | grep kvm
  ```
- Allocate more RAM/CPU in virt-manager if available

**Slow builds:**
- Windows Defender may be scanning build artifacts
- Add exclusion for C:\projects\owl-control in Windows Security settings

## Tips and Best Practices

- Pin PowerShell to the taskbar for quick access
- Always run PowerShell as Administrator when building
- Keep the VM running during active development to save boot time
- Take VM snapshots before major changes (virt-manager supports this)
- Regularly sync changes between Linux and Windows to avoid losing work

## Additional Resources

- [QEMU/KVM Documentation](https://www.linux-kvm.org/page/Documents)
- [libvirt Documentation](https://libvirt.org/docs.html)
- [Windows on QEMU Guide](https://wiki.archlinux.org/title/QEMU#Windows)
- [OWL Control Contributing Guide](./CONTRIBUTING.md)

## Questions?

If you encounter issues with this setup:

- Open an issue on [GitHub Issues](https://github.com/Wayfarer-Labs/owl-control/issues)
- Join the discussion in the Discord server
- Tag the issue with @legume.enthusiast69
