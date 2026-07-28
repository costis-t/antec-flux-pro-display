use anyhow::Result;
use serde_derive::Deserialize;
use std::{fs, path::Path};

#[derive(Deserialize, Debug)]
#[serde(default)]
pub struct Config {
    pub cpu_device: Option<String>,
    pub polling_interval: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cpu_device: None,
            polling_interval: 1000,
        }
    }
}

impl Config {
    /// Load the configuration from the TOML file located at `path`
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(toml::from_str(&fs::read_to_string(path)?)?)
    }

    /// Validate and sanitize config values
    pub fn validated(mut self) -> Self {
        // Polling interval validation:
        // - Min 100ms: prevents CPU spinning, USB flooding
        // - Max 60s: ensures display stays reasonably updated
        // - Default 1000ms: good balance of responsiveness vs resource usage
        if self.polling_interval < 100 {
            eprintln!(
                "Warning: polling_interval {}ms too low (min 100ms), using 100ms",
                self.polling_interval
            );
            self.polling_interval = 100;
        } else if self.polling_interval > 60000 {
            eprintln!(
                "Warning: polling_interval {}ms too high (max 60s), using 60000ms",
                self.polling_interval
            );
            self.polling_interval = 60000;
        }

        // CPU device path validation:
        // - Must be under /sys/ (sysfs) to prevent arbitrary file reads
        // - Must exist and be readable
        // - Should contain "temp" to be a temperature sensor
        if let Some(ref path) = self.cpu_device {
            let valid = path.starts_with("/sys/")
                && !path.contains("..")
                && std::path::Path::new(path).exists();

            if !valid {
                eprintln!(
                    "Warning: cpu_device '{}' invalid or not found, using auto-detection",
                    path
                );
                self.cpu_device = None;
            } else if !path.contains("temp") {
                eprintln!(
                    "Warning: cpu_device '{}' doesn't look like a temperature sensor",
                    path
                );
                // Allow it but warn - user might know what they're doing
            }
        }

        self
    }
}
