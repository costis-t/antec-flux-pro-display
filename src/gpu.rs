#[cfg(any(feature = "amd", feature = "intel"))]
use std::{fs, str::FromStr};

#[cfg(feature = "nvidia")]
use anyhow::Context;

#[cfg(any(feature = "nvidia", feature = "amd", feature = "intel"))]
use anyhow::Result;

#[cfg(feature = "nvidia")]
use nvml_wrapper::{Nvml, enum_wrappers::device::TemperatureSensor};

#[cfg(feature = "nvidia")]
pub struct NvidiaGpu {
    nvml: Nvml,
    device_index: u32,
}

#[cfg(feature = "nvidia")]
impl NvidiaGpu {
    pub fn new(nvml: Nvml) -> Self {
        Self {
            nvml,
            device_index: 0,
        }
    }

    pub fn temp(&self) -> Option<f32> {
        self.nvml
            .device_by_index(self.device_index)
            .inspect_err(|e| eprintln!("Error getting Nvidia GPU device: {e:?}"))
            .and_then(|device| device.temperature(TemperatureSensor::Gpu))
            .inspect_err(|e| eprintln!("Error getting Nvidia GPU temperature: {e:?}"))
            .map(|temp| temp as f32)
            .ok()
    }
}

#[cfg(feature = "amd")]
pub struct AmdGpu {
    hwmon_path: String,
}

#[cfg(feature = "amd")]
impl AmdGpu {
    pub fn new(hwmon_path: String) -> Self {
        Self { hwmon_path }
    }

    pub fn temp(&self) -> Option<f32> {
        fs::read_to_string(&self.hwmon_path)
            .inspect_err(|e| eprintln!("Error reading AMD GPU temp: {e}"))
            .ok()
            .and_then(|content| f32::from_str(content.trim()).ok())
            .map(|temp| temp / 1000.0)
    }
}

#[cfg(feature = "intel")]
pub struct IntelGpu {
    hwmon_path: String,
}

#[cfg(feature = "intel")]
impl IntelGpu {
    pub fn new(hwmon_path: String) -> Self {
        Self { hwmon_path }
    }

    pub fn temp(&self) -> Option<f32> {
        fs::read_to_string(&self.hwmon_path)
            .inspect_err(|e| eprintln!("Error reading Intel GPU temp: {e}"))
            .ok()
            .and_then(|content| f32::from_str(content.trim()).ok())
            .map(|temp| temp / 1000.0)
    }
}

pub enum AvailableGpu {
    #[cfg(feature = "nvidia")]
    Nvidia(Box<NvidiaGpu>),
    #[cfg(feature = "amd")]
    Amd(Box<AmdGpu>),
    #[cfg(feature = "intel")]
    Intel(Box<IntelGpu>),
    Unknown,
}

impl AvailableGpu {
    pub fn get_available_gpu() -> AvailableGpu {
        #[cfg(feature = "nvidia")]
        {
            let maybe_nvidia = try_get_nvidia_gpu()
                .inspect_err(|e| eprintln!("Failed to get Nvidia GPU. Error: {e}"));

            if let Ok(gpu) = maybe_nvidia {
                return gpu;
            }
        }

        #[cfg(feature = "amd")]
        {
            let maybe_amd =
                try_get_amd_gpu().inspect_err(|e| eprintln!("Failed to get AMD GPU. Error: {e}"));

            if let Ok(gpu) = maybe_amd {
                return gpu;
            }
        }

        #[cfg(feature = "intel")]
        {
            let maybe_intel = try_get_intel_gpu()
                .inspect_err(|e| eprintln!("Failed to get Intel GPU. Error: {e}"));

            if let Ok(gpu) = maybe_intel {
                return gpu;
            }
        }

        AvailableGpu::Unknown
    }

    pub fn temp(&self) -> Option<f32> {
        match self {
            #[cfg(feature = "nvidia")]
            AvailableGpu::Nvidia(gpu) => gpu.temp(),
            #[cfg(feature = "amd")]
            AvailableGpu::Amd(gpu) => gpu.temp(),
            #[cfg(feature = "intel")]
            AvailableGpu::Intel(gpu) => gpu.temp(),
            AvailableGpu::Unknown => None,
        }
    }
}

#[cfg(feature = "nvidia")]
fn try_get_nvidia_gpu() -> Result<AvailableGpu> {
    let nvml = Nvml::builder()
        .lib_path(std::ffi::OsStr::new("libnvidia-ml.so.1"))
        .init()
        .context("Failed to initialize NVML")?;

    let driver_version = nvml
        .sys_driver_version()
        .context("Failed to get NVML driver version")?;
    println!("NVML initialized, driver version: {driver_version}");

    let device_count = nvml
        .device_count()
        .context("Failed to get NVML device count")?;

    println!("Found {device_count} NVML-supported GPUs");
    Ok(AvailableGpu::Nvidia(Box::new(NvidiaGpu::new(nvml))))
}

#[cfg(feature = "amd")]
fn try_get_amd_gpu() -> Result<AvailableGpu> {
    // Look for AMD GPU hwmon devices
    // AMD GPUs typically expose temps via /sys/class/drm/card*/device/hwmon/hwmon*/temp1_input
    // or via /sys/class/hwmon/hwmon*/temp1_input with name "amdgpu"

    // First try the hwmon approach (more reliable)
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name_path = path.join("name");
            if let Ok(name) = fs::read_to_string(&name_path) {
                if name.trim() == "amdgpu" {
                    let temp_path = path.join("temp1_input");
                    if temp_path.exists() {
                        println!("Found AMD GPU at: {}", temp_path.display());
                        return Ok(AvailableGpu::Amd(Box::new(AmdGpu::new(
                            temp_path.to_string_lossy().to_string(),
                        ))));
                    }
                }
            }
        }
    }

    // Fallback: try DRM subsystem
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let path = entry.path();
            let device_hwmon = path.join("device/hwmon");
            if device_hwmon.exists() {
                if let Ok(hwmon_entries) = fs::read_dir(&device_hwmon) {
                    for hwmon_entry in hwmon_entries.flatten() {
                        let temp_path = hwmon_entry.path().join("temp1_input");
                        if temp_path.exists() {
                            // Verify it's an AMD GPU by checking the driver
                            let driver_path = path.join("device/driver");
                            if let Ok(driver_link) = fs::read_link(&driver_path) {
                                if driver_link.to_string_lossy().contains("amdgpu") {
                                    println!("Found AMD GPU at: {}", temp_path.display());
                                    return Ok(AvailableGpu::Amd(Box::new(AmdGpu::new(
                                        temp_path.to_string_lossy().to_string(),
                                    ))));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    anyhow::bail!("No AMD GPU found")
}

#[cfg(feature = "intel")]
fn try_get_intel_gpu() -> Result<AvailableGpu> {
    // Look for Intel GPU hwmon devices
    // Intel GPUs (including Arc) expose temps via /sys/class/hwmon/hwmon*/temp1_input
    // with name "i915" (legacy) or "xe" (newer Arc GPUs)

    // First try the hwmon approach
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            let name_path = path.join("name");
            if let Ok(name) = fs::read_to_string(&name_path) {
                let name = name.trim();
                // Check for Intel GPU drivers: i915 (legacy/integrated), xe (Arc/newer)
                if name == "i915" || name == "xe" {
                    let temp_path = path.join("temp1_input");
                    if temp_path.exists() {
                        println!("Found Intel GPU ({name}) at: {}", temp_path.display());
                        return Ok(AvailableGpu::Intel(Box::new(IntelGpu::new(
                            temp_path.to_string_lossy().to_string(),
                        ))));
                    }
                }
            }
        }
    }

    // Fallback: try DRM subsystem
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let path = entry.path();
            let device_hwmon = path.join("device/hwmon");
            if device_hwmon.exists() {
                if let Ok(hwmon_entries) = fs::read_dir(&device_hwmon) {
                    for hwmon_entry in hwmon_entries.flatten() {
                        let temp_path = hwmon_entry.path().join("temp1_input");
                        if temp_path.exists() {
                            // Verify it's an Intel GPU by checking the driver
                            let driver_path = path.join("device/driver");
                            if let Ok(driver_link) = fs::read_link(&driver_path) {
                                let driver_name = driver_link.to_string_lossy();
                                if driver_name.contains("i915") || driver_name.contains("xe") {
                                    println!("Found Intel GPU at: {}", temp_path.display());
                                    return Ok(AvailableGpu::Intel(Box::new(IntelGpu::new(
                                        temp_path.to_string_lossy().to_string(),
                                    ))));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    anyhow::bail!("No Intel GPU found")
}
