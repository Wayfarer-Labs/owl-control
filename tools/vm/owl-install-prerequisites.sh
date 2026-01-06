#!/bin/bash
set -e

# OWL Control - Prerequisites Installer
# This script only installs required packages and configures user permissions

echo "============================================"
echo "OWL Control - Installing Prerequisites"
echo "============================================"
echo ""

# Check if running as root
if [ "$EUID" -eq 0 ]; then
    echo "ERROR: Do not run this script as root or with sudo."
    echo "The script will prompt for sudo when needed."
    exit 1
fi

# Check virtualization support
echo "Step 1: Checking virtualization support"
echo "----------------------------------------"

if ! egrep -q '(vmx|svm)' /proc/cpuinfo; then
    echo "ERROR: CPU virtualization is not available."
    echo "Please enable VT-x (Intel) or AMD-V (AMD) in your BIOS/UEFI settings."
    exit 1
fi

echo "✓ Virtualization support detected."
echo ""

# Install packages
echo "Step 2: Installing packages"
echo "----------------------------"

PACKAGES="qemu-system libvirt-daemon-system libvirt-clients virt-manager samba"

echo "The following packages will be installed:"
echo "  $PACKAGES"
echo ""
read -p "Continue? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Installation cancelled."
    exit 0
fi

echo "Updating package list..."
sudo apt update

echo "Installing packages..."
sudo apt install -y $PACKAGES

echo "✓ Packages installed."
echo ""

# Add user to groups
echo "Step 3: Configuring user permissions"
echo "-------------------------------------"

CURRENT_USER=$(whoami)
NEEDS_RELOGIN=false

if ! groups $CURRENT_USER | grep -q "\blibvirt\b"; then
    echo "Adding $CURRENT_USER to libvirt group..."
    sudo usermod -aG libvirt $CURRENT_USER
    NEEDS_RELOGIN=true
fi

if ! groups $CURRENT_USER | grep -q "\bkvm\b"; then
    echo "Adding $CURRENT_USER to kvm group..."
    sudo usermod -aG kvm $CURRENT_USER
    NEEDS_RELOGIN=true
fi

if [ "$NEEDS_RELOGIN" = true ]; then
    echo ""
    echo "✓ User added to libvirt and kvm groups."
    echo ""
    echo "IMPORTANT: You must log out and log back in for group changes to take effect!"
    echo "After logging back in, run: virt-manager"
else
    echo "✓ User permissions already configured."
fi

echo ""

# Start services
echo "Step 4: Starting services"
echo "-------------------------"

if ! systemctl is-active --quiet libvirtd; then
    echo "Starting libvirtd..."
    sudo systemctl start libvirtd
    sudo systemctl enable libvirtd
    echo "✓ libvirtd started."
else
    echo "✓ libvirtd already running."
fi

if ! systemctl is-active --quiet smbd; then
    echo "Starting Samba..."
    sudo systemctl start smbd
    sudo systemctl enable smbd
    echo "✓ Samba started."
else
    echo "✓ Samba already running."
fi

echo ""

# Configure Samba share
echo "Step 5: Configuring Samba share"
echo "--------------------------------"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Share the owl-control root directory (two levels up from tools/vm)
SHARED_FOLDER_PATH="$(cd "$SCRIPT_DIR/../.." && pwd)"
SHARED_FOLDER_NAME="owl-control"
SMB_CONF="/etc/samba/smb.conf"

if grep -q "^\[$SHARED_FOLDER_NAME\]" "$SMB_CONF" 2>/dev/null; then
    echo "✓ Samba share already configured."
else
    echo "Adding Samba share for: $SHARED_FOLDER_PATH"
    
    SAMBA_CONFIG="
[$SHARED_FOLDER_NAME]
    path = $SHARED_FOLDER_PATH
    browseable = yes
    read only = no
    guest ok = yes
    force user = $CURRENT_USER
    create mask = 0644
    directory mask = 0755
"
    
    # Backup existing config
    echo "Creating backup of smb.conf..."
    sudo cp "$SMB_CONF" "${SMB_CONF}.backup.$(date +%Y%m%d_%H%M%S)"
    
    # Add share configuration
    echo "$SAMBA_CONFIG" | sudo tee -a "$SMB_CONF" > /dev/null
    
    # Restart Samba to apply changes
    echo "Restarting Samba..."
    sudo systemctl restart smbd
    
    echo "✓ Samba share configured."
    echo "  Windows will access it at: \\\\192.168.122.1\\owl-control"
fi

echo ""

# Download VirtIO drivers
echo "Step 6: Downloading VirtIO drivers"
echo "-----------------------------------"

VIRTIO_ISO_URL="https://fedorapeople.org/groups/virt/virtio-win/direct-downloads/archive-virtio/virtio-win-0.1.285-1/virtio-win.iso"
VIRTIO_ISO_PATH="/var/lib/libvirt/images/virtio-win.iso"

if [ -f "$VIRTIO_ISO_PATH" ]; then
    echo "✓ VirtIO drivers already downloaded."
else
    echo "Downloading virtio-win.iso (approximately 750MB)..."
    echo "This may take several minutes..."
    
    TEMP_VIRTIO="/tmp/virtio-win.iso"
    
    if command -v wget &> /dev/null; then
        wget -O "$TEMP_VIRTIO" "$VIRTIO_ISO_URL"
    elif command -v curl &> /dev/null; then
        curl -L -o "$TEMP_VIRTIO" "$VIRTIO_ISO_URL"
    else
        echo "ERROR: Neither wget nor curl is available."
        echo "Please install wget: sudo apt install wget"
        exit 1
    fi
    
    echo "Installing VirtIO ISO to $VIRTIO_ISO_PATH..."
    sudo mv "$TEMP_VIRTIO" "$VIRTIO_ISO_PATH"
    sudo chmod 644 "$VIRTIO_ISO_PATH"
    echo "✓ VirtIO drivers downloaded."
fi

echo ""

# Summary
echo "============================================"
echo "Installation Complete!"
echo "============================================"
echo ""
echo "Prerequisites are now installed."
echo ""
if [ "$NEEDS_RELOGIN" = true ]; then
    echo "NEXT STEPS:"
    echo "1. Log out and log back in (required for group permissions)"
    echo "2. See LINUX_DEV_SETUP.md for VM creation instructions"
else
    echo "NEXT STEPS:"
    echo "1. See LINUX_DEV_SETUP.md for VM creation instructions"
    echo "2. Or run: virt-manager"
fi
echo ""

# 466f724a616e6574