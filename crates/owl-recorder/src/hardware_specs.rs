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

pub fn get_hardware_specs() -> Result<HardwareSpecs> {
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

    // GPU info (basic - sysinfo doesn't have detailed GPU support)
    let mut gpus = Vec::new();

    // Try to get GPU info from Windows registry or system calls
    #[cfg(target_os = "windows")]
    {
        match get_windows_gpu_info() {
            Ok(gpu_list) => gpus.extend(gpu_list),
            Err(e) => {
                tracing::warn!("Failed to get GPU info: {}", e);
                // Add a placeholder if we can't detect GPU
                gpus.push(GpuSpecs {
                    name: "Unknown GPU".to_string(),
                    vendor: "Unknown".to_string(),
                });
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // For non-Windows, add placeholder
        gpus.push(GpuSpecs {
            name: "Unknown GPU".to_string(),
            vendor: "Unknown".to_string(),
        });
    }

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
fn get_windows_gpu_info() -> Result<Vec<GpuSpecs>> {
    use std::process::Command;

    // Try to get GPU info using wmic
    let output = Command::new("wmic")
        .args([
            "path",
            "win32_VideoController",
            "get",
            "name",
            "/format:csv",
        ])
        .output()?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut gpus = Vec::new();

    for line in output_str.lines().skip(2) {
        // Skip header lines
        let line = line.trim();
        if !line.is_empty() && line.contains(',') {
            // CSV format: Node,Name
            if let Some(gpu_name) = line.split(',').nth(1) {
                let gpu_name = gpu_name.trim();
                if !gpu_name.is_empty() {
                    let vendor = if gpu_name.to_lowercase().contains("nvidia") {
                        "NVIDIA"
                    } else if gpu_name.to_lowercase().contains("amd")
                        || gpu_name.to_lowercase().contains("radeon")
                    {
                        "AMD"
                    } else if gpu_name.to_lowercase().contains("intel") {
                        "Intel"
                    } else {
                        "Unknown"
                    };

                    gpus.push(GpuSpecs {
                        name: gpu_name.to_string(),
                        vendor: vendor.to_string(),
                    });
                }
            }
        }
    }

    if gpus.is_empty() {
        // Fallback: try a simpler wmic command
        let output = Command::new("wmic")
            .args(["path", "win32_VideoController", "get", "name"])
            .output()?;

        let output_str = String::from_utf8_lossy(&output.stdout);
        for line in output_str.lines().skip(1) {
            // Skip header
            let line = line.trim();
            if !line.is_empty() && line != "Name" {
                let vendor = if line.to_lowercase().contains("nvidia") {
                    "NVIDIA"
                } else if line.to_lowercase().contains("amd")
                    || line.to_lowercase().contains("radeon")
                {
                    "AMD"
                } else if line.to_lowercase().contains("intel") {
                    "Intel"
                } else {
                    "Unknown"
                };

                gpus.push(GpuSpecs {
                    name: line.to_string(),
                    vendor: vendor.to_string(),
                });
            }
        }
    }

    Ok(gpus)
}
