#!/bin/bash
set -e

# OWL Control - Complete Cleanup Script
# This script removes all VMs, configurations, and optionally the installed packages

echo "============================================"
echo "OWL Control - Complete Cleanup"
echo "============================================"
echo ""
echo "This will remove:"
echo "  - All VMs named 'owl-control-vm'"
echo "  - VM disk files"
echo "  - VirtIO drivers ISO"
echo "  - Samba share configuration"
echo ""
echo "Optionally remove:"
echo "  - Installed packages (virt-manager, QEMU, etc.)"
echo ""
read -p "Are you sure you want to continue? (y/N): " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Cleanup cancelled."
    exit 0
fi

echo ""

# Remove VMs
echo "Step 1: Removing VMs"
echo "--------------------"

VM_NAME="owl-control-vm"

if virsh list --all 2>/dev/null | grep -q "$VM_NAME"; then
    echo "Found VM '$VM_NAME', removing..."
    virsh destroy "$VM_NAME" 2>/dev/null || true
    virsh undefine "$VM_NAME" --nvram --remove-all-storage 2>/dev/null || \
    virsh undefine "$VM_NAME" --nvram 2>/dev/null || \
    virsh undefine "$VM_NAME" 2>/dev/null || true
    echo "VM removed."
else
    echo "No VM found."
fi

# Kill any running QEMU processes
if pgrep -f "qemu.*$VM_NAME" >/dev/null 2>&1; then
    echo "Stopping QEMU processes..."
    sudo pkill -9 -f "qemu.*$VM_NAME" 2>/dev/null || true
    sleep 2
fi

# Remove VM files
echo "Removing VM files..."
sudo rm -f /var/lib/libvirt/images/${VM_NAME}.qcow2 2>/dev/null || true
sudo rm -rf /var/lib/libvirt/qemu/nvram/*${VM_NAME}* 2>/dev/null || true
sudo rm -rf /var/lib/libvirt/swtpm/*${VM_NAME}* 2>/dev/null || true
sudo rm -rf /var/lib/libvirt/qemu/domain-*${VM_NAME}* 2>/dev/null || true

echo "VM cleanup complete."
echo ""

# Remove VirtIO ISO
echo "Step 2: Removing VirtIO drivers"
echo "--------------------------------"

VIRTIO_ISO_PATH="/var/lib/libvirt/images/virtio-win.iso"

if [ -f "$VIRTIO_ISO_PATH" ]; then
    echo "Removing VirtIO ISO..."
    sudo rm -f "$VIRTIO_ISO_PATH"
    echo "VirtIO ISO removed."
else
    echo "VirtIO ISO not found."
fi

echo ""

# Remove Samba share
echo "Step 3: Removing Samba share"
echo "-----------------------------"

SMB_CONF="/etc/samba/smb.conf"

if grep -q "^\[owl-control\]" "$SMB_CONF" 2>/dev/null; then
    echo "Removing Samba share configuration..."
    sudo cp "$SMB_CONF" "${SMB_CONF}.backup.$(date +%Y%m%d_%H%M%S)"
    
    # Remove the [owl-control] section and all lines until the next section or EOF
    sudo sed -i '/^\[owl-control\]/,/^\[/{ /^\[owl-control\]/d; /^\[/!d; }' "$SMB_CONF"
    
    echo "Restarting Samba..."
    sudo systemctl restart smbd 2>/dev/null || true
    
    echo "Samba share configuration removed."
else
    echo "No Samba share configuration found."
fi

echo ""

# Restart libvirtd to clear state
echo "Step 4: Clearing libvirt state"
echo "-------------------------------"

echo "Restarting libvirtd..."
sudo systemctl stop libvirtd
sleep 2
sudo systemctl start libvirtd

echo "libvirt state cleared."
echo ""

# Ask about removing packages
echo "Step 5: Package removal (optional)"
echo "----------------------------------"
echo ""
echo "Do you want to remove the installed packages?"
echo "This will remove:"
echo "  - virt-manager"
echo "  - QEMU/KVM"
echo "  - libvirt"
echo "  - Samba"
echo ""
echo "WARNING: Only do this if you don't use these packages for other purposes!"
echo ""
read -p "Remove packages? (y/N): " -n 1 -r
echo

if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "Removing packages..."
    sudo apt remove -y qemu-system libvirt-daemon-system libvirt-clients virt-manager samba
    sudo apt autoremove -y
    echo "Packages removed."
else
    echo "Packages kept."
fi

echo ""

# Summary
echo "============================================"
echo "Cleanup Complete!"
echo "============================================"
echo ""
echo "The following have been removed:"
echo "  - VMs and VM disk files"
echo "  - VirtIO drivers ISO"
echo "  - Samba share configuration"

if [[ $REPLY =~ ^[Yy]$ ]]; then
    echo "  - Installed packages"
fi

echo ""
echo "Your system has been returned to its pre-installation state."
echo ""

# 466f724a616e6574