use std::fs;

use crate::sensors;

pub fn read_temp(device: &str) -> Option<f32> {
    sensors::read_millidegrees(device, "CPU")
}

pub fn default_cpu_device() -> Option<String> {
    // Prefer hwmon devices that are actual CPU sensors: AMD k10temp exposes
    // temp1_label "Tctl"; Intel coretemp and the out-of-tree zenpower driver
    // are matched by name since their labels vary. Errors on individual
    // entries fall through to the next entry and then to the fallbacks below.
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for hwmon in entries.flatten() {
            let path = hwmon.path();
            let label_ok = fs::read_to_string(path.join("temp1_label"))
                .is_ok_and(|label| label.trim() == "Tctl");
            let name_ok = fs::read_to_string(path.join("name"))
                .is_ok_and(|name| matches!(name.trim(), "k10temp" | "coretemp" | "zenpower"));
            let temp_path = path.join("temp1_input");
            if (label_ok || name_ok) && temp_path.exists() {
                return Some(temp_path.to_string_lossy().into_owned());
            }
        }
    }

    // thermal_zone0 may be acpitz or another non-CPU sensor on some boards,
    // but it is the best remaining guess.
    if fs::read_to_string("/sys/class/thermal/thermal_zone0/temp").is_ok() {
        return Some("/sys/class/thermal/thermal_zone0/temp".to_string());
    }

    // Deliberately no hwmon0 fallback. hwmon numbering is assigned in probe
    // order, so hwmon0 is whatever registered first -- commonly an NVMe drive
    // or a chipset sensor, and on hosts with no thermal zones at all the
    // fallback above does not catch that. Reporting no reading is better than
    // confidently displaying an unrelated device's temperature as the CPU's;
    // set cpu_device explicitly if auto-detection misses your sensor.
    eprintln!(
        "Could not identify a CPU temperature sensor. Set cpu_device in the \
         config file to a /sys path (see: grep -H . /sys/class/hwmon/*/name)"
    );
    None
}
