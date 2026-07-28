use std::{fs, str::FromStr};

/// Read a sysfs temperature file (millidegrees Celsius) and convert to degrees.
pub fn read_millidegrees(path: &str, label: &str) -> Option<f32> {
    fs::read_to_string(path)
        .inspect_err(|e| eprintln!("Error reading {label} temp: {e}"))
        .ok()
        .and_then(|content| f32::from_str(content.trim()).ok())
        .map(|temp| temp / 1000.0)
}
