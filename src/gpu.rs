#[cfg(any(feature = "amd", feature = "intel"))]
use std::fs;

#[cfg(feature = "nvidia")]
use anyhow::Context;

#[cfg(any(feature = "nvidia", feature = "amd", feature = "intel"))]
use anyhow::Result;

#[cfg(any(feature = "amd", feature = "intel"))]
use crate::sensors;

#[cfg(feature = "nvidia")]
use nvml_wrapper::{Nvml, enum_wrappers::device::TemperatureSensor};

/// Which NVML device to report. Multi-GPU selection is not configurable yet.
#[cfg(feature = "nvidia")]
const NVML_DEVICE_INDEX: u32 = 0;

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
            device_index: NVML_DEVICE_INDEX,
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

/// A GPU whose temperature is exposed as a sysfs hwmon file
/// (AMD amdgpu, Intel i915/xe).
#[cfg(any(feature = "amd", feature = "intel"))]
pub struct SysfsGpu {
    hwmon_path: String,
    vendor: &'static str,
}

#[cfg(any(feature = "amd", feature = "intel"))]
impl SysfsGpu {
    pub fn temp(&self) -> Option<f32> {
        sensors::read_millidegrees(&self.hwmon_path, self.vendor)
    }
}

pub enum AvailableGpu {
    #[cfg(feature = "nvidia")]
    Nvidia(Box<NvidiaGpu>),
    #[cfg(feature = "amd")]
    Amd(SysfsGpu),
    #[cfg(feature = "intel")]
    Intel(SysfsGpu),
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

    // Claiming the NVIDIA backend short-circuits the AMD and Intel probes for
    // the life of the process, so only claim it if there is a device we can
    // actually read. libnvidia-ml.so.1 being present proves nothing: the card
    // may be bound to vfio, removed, or left over from a GPU swap.
    if device_count == 0 {
        anyhow::bail!("NVML initialized but reports 0 GPUs");
    }
    nvml.device_by_index(NVML_DEVICE_INDEX)
        .context("NVML reports a GPU but device 0 is not accessible")?;

    println!("Found {device_count} NVML-supported GPUs");
    Ok(AvailableGpu::Nvidia(Box::new(NvidiaGpu::new(nvml))))
}

#[cfg(feature = "amd")]
fn try_get_amd_gpu() -> Result<AvailableGpu> {
    find_sysfs_gpu(&["amdgpu"], &["amdgpu"], "AMD").map(AvailableGpu::Amd)
}

#[cfg(feature = "intel")]
fn try_get_intel_gpu() -> Result<AvailableGpu> {
    // i915 covers legacy/integrated Intel GPUs, xe covers Arc and newer
    find_sysfs_gpu(&["i915", "xe"], &["i915", "xe"], "Intel").map(AvailableGpu::Intel)
}

/// Locate a GPU temperature sensor in sysfs: first by hwmon device name,
/// then by scanning DRM devices and matching the bound driver.
#[cfg(any(feature = "amd", feature = "intel"))]
fn find_sysfs_gpu(
    hwmon_names: &[&str],
    driver_substrings: &[&str],
    vendor: &'static str,
) -> Result<SysfsGpu> {
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(name) = fs::read_to_string(path.join("name"))
                && hwmon_names.contains(&name.trim())
            {
                let temp_path = path.join("temp1_input");
                if temp_path.exists() {
                    println!("Found {vendor} GPU at: {}", temp_path.display());
                    return Ok(SysfsGpu {
                        hwmon_path: temp_path.to_string_lossy().into_owned(),
                        vendor,
                    });
                }
            }
        }
    }

    // Fallback: DRM subsystem
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let path = entry.path();
            let device_hwmon = path.join("device/hwmon");
            if let Ok(hwmon_entries) = fs::read_dir(&device_hwmon) {
                for hwmon_entry in hwmon_entries.flatten() {
                    let temp_path = hwmon_entry.path().join("temp1_input");
                    if temp_path.exists()
                        && let Ok(driver_link) = fs::read_link(path.join("device/driver"))
                    {
                        let driver = driver_link.to_string_lossy();
                        if driver_substrings.iter().any(|d| driver.contains(d)) {
                            println!("Found {vendor} GPU at: {}", temp_path.display());
                            return Ok(SysfsGpu {
                                hwmon_path: temp_path.to_string_lossy().into_owned(),
                                vendor,
                            });
                        }
                    }
                }
            }
        }
    }

    anyhow::bail!("No {vendor} GPU found")
}
