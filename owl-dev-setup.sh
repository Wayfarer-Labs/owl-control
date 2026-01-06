#!/bin/bash

# OWL Control - Development Environment Setup
# Main entry point for Linux developers

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

show_menu() {
    clear
    echo "============================================"
    echo "OWL Control - Development Setup"
    echo "============================================"
    echo ""
    echo "This tool helps you set up a Windows VM for"
    echo "developing OWL Control on Linux."
    echo ""
    echo "Options:"
    echo ""
    echo "  1) Install prerequisites"
    echo "     Installs packages, configures Samba,"
    echo "     downloads VirtIO drivers"
    echo ""
    echo "  2) View setup instructions"
    echo "     Shows next steps for VM creation"
    echo ""
    echo "  3) Clean up everything"
    echo "     Removes VMs, files, and configurations"
    echo ""
    echo "  4) Exit"
    echo ""
    echo "============================================"
    echo ""
}

view_instructions() {
    clear
    echo "============================================"
    echo "VM Setup Instructions"
    echo "============================================"
    echo ""
    echo "Prerequisites have been installed. Next steps:"
    echo ""
    echo "1. Download Windows 11 ISO"
    echo "   Visit: https://www.microsoft.com/software-download/windows11"
    echo ""
    echo "2. Open virt-manager"
    echo "   Run: virt-manager"
    echo ""
    echo "3. Create new VM"
    echo "   - Click 'Create a new virtual machine'"
    echo "   - Select 'Local install media'"
    echo "   - Browse to your Windows 11 ISO"
    echo "   - Choose 'Windows 11' as OS"
    echo "   - Allocate at least 4GB RAM, 4 CPUs"
    echo "   - Create 60GB disk"
    echo "   - Check 'Customize configuration before install'"
    echo ""
    echo "4. Add VirtIO drivers CD"
    echo "   - Click 'Add Hardware'"
    echo "   - Select 'Storage'"
    echo "   - Device type: CDROM"
    echo "   - Browse to: /var/lib/libvirt/images/virtio-win.iso"
    echo ""
    echo "5. Configure boot"
    echo "   - Go to 'Boot Options'"
    echo "   - Enable 'CDROM' and move it to top"
    echo "   - Set Firmware to 'UEFI x86_64'"
    echo ""
    echo "6. Install Windows"
    echo "   - Start the VM"
    echo "   - When asked where to install:"
    echo "     * Click 'Load driver'"
    echo "     * Browse VirtIO CD to vioscsi/w11/amd64"
    echo "     * Install driver"
    echo "   - Complete Windows installation"
    echo ""
    echo "7. After Windows boots"
    echo "   - Open VirtIO CD drive"
    echo "   - Run 'virtio-win-guest-tools.exe'"
    echo "   - Reboot"
    echo ""
    echo "8. Map shared folder (in Windows)"
    echo "   Open PowerShell and run:"
    echo "   net use Z: \\\\192.168.122.1\\owl-control /persistent:yes"
    echo ""
    echo "9. Install dev tools (in Windows)"
    echo "   In PowerShell as Administrator:"
    echo "   cd Z:\\"
    echo "   .\\windows-setup.ps1"
    echo ""
    echo "For detailed instructions, see: LINUX_DEV_SETUP.md"
    echo ""
    echo "============================================"
    echo ""
    read -p "Press Enter to return to menu..."
}

run_install() {
    clear
    echo "============================================"
    echo "Running Prerequisites Installation"
    echo "============================================"
    echo ""
    
    if [ ! -f "$SCRIPT_DIR/owl-install-prerequisites.sh" ]; then
        echo "ERROR: owl-install-prerequisites.sh not found!"
        read -p "Press Enter to return to menu..."
        return
    fi
    
    bash "$SCRIPT_DIR/owl-install-prerequisites.sh"
    
    echo ""
    read -p "Press Enter to return to menu..."
}

run_cleanup() {
    clear
    echo "============================================"
    echo "Running Complete Cleanup"
    echo "============================================"
    echo ""
    
    if [ ! -f "$SCRIPT_DIR/owl-cleanup.sh" ]; then
        echo "ERROR: owl-cleanup.sh not found!"
        read -p "Press Enter to return to menu..."
        return
    fi
    
    bash "$SCRIPT_DIR/owl-cleanup.sh"
    
    echo ""
    read -p "Press Enter to return to menu..."
}

# Main loop
while true; do
    show_menu
    read -p "Select an option (1-4): " choice
    
    case $choice in
        1)
            run_install
            ;;
        2)
            view_instructions
            ;;
        3)
            run_cleanup
            ;;
        4)
            echo ""
            echo "Exiting. Good luck with your development!"
            echo ""
            exit 0
            ;;
        *)
            echo ""
            echo "Invalid option. Please select 1-4."
            sleep 2
            ;;
    esac
done

# 466f724a616e6574