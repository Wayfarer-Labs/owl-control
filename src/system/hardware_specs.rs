use color_eyre::Result;
use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuSpecs {
    pub name: String,
    pub cores: usize,
    pub frequency_mhz: u64,
    pub vendor: String,
    pub brand: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GpuSpecs {
    pub name: String,
    pub vendor: String,
}
impl GpuSpecs {
    pub fn from_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();
        let vendor = if name_lower.contains("nvidia") {
            "NVIDIA"
        } else if name_lower.contains("amd") || name_lower.contains("radeon") {
            "AMD"
        } else if name_lower.contains("intel") {
            "Intel"
        } else {
            "Unknown"
        };

        Self {
            name: name.to_string(),
            vendor: vendor.to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemSpecs {
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub hostname: String,
    pub total_memory_gb: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HardwareSpecs {
    pub cpu: CpuSpecs,
    pub gpus: Vec<GpuSpecs>,
    pub system: SystemSpecs,
}

pub fn get_hardware_specs(gpus: Vec<GpuSpecs>) -> Result<HardwareSpecs> {
    let mut sys = System::new_all();
    sys.refresh_all();

    // CPU info
    let cpu_info = sys
        .cpus()
        .first()
        .ok_or_else(|| color_eyre::eyre::eyre!("No CPU information available"))?;

    let cpu_specs = CpuSpecs {
        name: cpu_info.name().to_string(),
        cores: sys.cpus().len(),
        frequency_mhz: cpu_info.frequency(),
        vendor: cpu_info.vendor_id().to_string(),
        brand: cpu_info.brand().to_string(),
    };

    // System info
    let system_specs = SystemSpecs {
        os_name: System::name().unwrap_or_else(|| "Unknown".to_string()),
        os_version: System::os_version().unwrap_or_else(|| "Unknown".to_string()),
        kernel_version: System::kernel_version().unwrap_or_else(|| "Unknown".to_string()),
        hostname: System::host_name().unwrap_or_else(|| "Unknown".to_string()),
        total_memory_gb: sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0),
    };

    Ok(HardwareSpecs {
        cpu: cpu_specs,
        gpus,
        system: system_specs,
    })
}

#[cfg(target_os = "windows")]
/// Returns the resolution of the primary monitor
pub fn get_primary_monitor_resolution() -> Option<(u32, u32)> {
    use windows::{
        Win32::{
            Foundation::POINT,
            Graphics::Gdi::{
                DEVMODEW, ENUM_CURRENT_SETTINGS, EnumDisplaySettingsW, GetMonitorInfoW,
                MONITORINFO, MONITORINFOEXW, MonitorFromPoint,
            },
        },
        core::PCWSTR,
    };

    // Get the primary monitor handle
    let primary_monitor = unsafe {
        MonitorFromPoint(
            POINT { x: 0, y: 0 },
            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTOPRIMARY,
        )
    };
    if primary_monitor.is_invalid() {
        return None;
    }

    // Get the monitor info
    let mut monitor_info = MONITORINFOEXW {
        monitorInfo: MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFOEXW>() as u32,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        GetMonitorInfoW(
            primary_monitor,
            &mut monitor_info as *mut _ as *mut MONITORINFO,
        )
    }
    .ok()
    .ok()?;

    // Get the display mode
    let mut devmode = DEVMODEW {
        dmSize: std::mem::size_of::<DEVMODEW>() as u16,
        ..Default::default()
    };
    unsafe {
        EnumDisplaySettingsW(
            PCWSTR(monitor_info.szDevice.as_ptr()),
            ENUM_CURRENT_SETTINGS,
            &mut devmode,
        )
    }
    .ok()
    .ok()?;

    Some((devmode.dmPelsWidth, devmode.dmPelsHeight))
}
