mod config;
mod cpu;
mod gpu;
mod sensors;
mod usb;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use clap::Parser;

use config::Config;
use cpu::default_cpu_device;
use gpu::AvailableGpu;
use usb::UsbDevice;

const SYSTEM_CONFIG_PATH: &str = "/etc/antec-flux-pro-display/config.toml";
const USER_CONFIG_PATH: &str = "~/.config/antec-flux-pro-display/config.toml";

#[derive(clap::Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[arg(short, long)]
    config: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = load_config(cli.config.as_deref())?;

    let running = Arc::new(AtomicBool::new(true));
    let device = UsbDevice::open(usb::VENDOR_ID, usb::PRODUCT_ID)?;
    let polling_interval = config.polling_interval;
    let cpu = config.cpu_device.or_else(default_cpu_device);
    let gpu = AvailableGpu::get_available_gpu();

    // Handle CTRL+C and other termination gracefully
    let run = running.clone();
    ctrlc::set_handler(move || {
        run.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    let result = poll_loop(&device, cpu.as_deref(), &gpu, polling_interval, &running);

    // Blank the display on the way out, whatever the reason we are leaving:
    // a clean shutdown, a fatal USB error, or persistent write failures. The
    // sentinel is what the display renders as "no reading" -- sending 0.0
    // would draw a plausible 00.0 and look like a real measurement. The device
    // may already be gone, so ignore errors.
    let _ = device.send_payload(None, None);

    result
}

/// Resolve and load the configuration.
///
/// An explicit `--config` path is fatal when it is missing or unreadable: the
/// user named those settings, so silently falling back to defaults yields a
/// service that reports `active (running)` while ignoring everything it was
/// told. The two implicit locations are probed, not required.
fn load_config(cli_path: Option<&str>) -> Result<Config> {
    if let Some(path) = cli_path {
        let path = PathBuf::from(shellexpand::tilde(path).to_string());
        if !path.exists() {
            anyhow::bail!("--config {}: no such file", path.display());
        }
        return read_config(&path);
    }

    for path in [
        PathBuf::from(SYSTEM_CONFIG_PATH),
        PathBuf::from(shellexpand::tilde(USER_CONFIG_PATH).to_string()),
    ] {
        if path.exists() {
            return read_config(&path);
        }
    }

    eprintln!("No config file at {SYSTEM_CONFIG_PATH} or {USER_CONFIG_PATH}, using defaults");
    Ok(Config::default())
}

fn read_config(path: &std::path::Path) -> Result<Config> {
    println!("Using config: {}", path.display());
    Ok(Config::from_file(path)
        .with_context(|| format!("Failed to read config {}", path.display()))?
        .validated())
}

/// Poll the sensors and drive the display until `running` is cleared, or until
/// a USB error makes it pointless to continue.
fn poll_loop(
    device: &UsbDevice,
    cpu: Option<&str>,
    gpu: &AvailableGpu,
    polling_interval: u64,
    running: &AtomicBool,
) -> Result<()> {
    // Escalate persistent transient errors (e.g. a wedged device that only
    // ever times out) to an exit so the service manager can restart us
    const MAX_CONSECUTIVE_ERRORS: u32 = 10;
    let mut consecutive_errors: u32 = 0;

    while running.load(Ordering::SeqCst) {
        let cpu_temp = cpu.and_then(cpu::read_temp);
        let gpu_temp = gpu.temp();

        match device.send_payload(cpu_temp, gpu_temp) {
            Ok(()) => consecutive_errors = 0,
            // The device is gone or the handle is dead; a stale handle
            // never recovers after a replug, so exit and let the service
            // manager restart us with a fresh one.
            Err(e @ (rusb::Error::NoDevice | rusb::Error::Io | rusb::Error::NotFound)) => {
                anyhow::bail!("USB write failed ({e}), exiting to be restarted");
            }
            Err(e) => {
                consecutive_errors += 1;
                eprintln!("Error writing to USB device: {e:?} ({consecutive_errors} consecutive)");
                // A stalled endpoint is recoverable in place
                if matches!(e, rusb::Error::Pipe)
                    && let Err(ce) = device.clear_halt()
                {
                    eprintln!("Failed to clear halted endpoint: {ce:?}");
                }
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    anyhow::bail!("USB writes failing persistently ({e}), exiting to be restarted");
                }
            }
        }

        // Sleep in short slices so Ctrl-C/SIGTERM takes effect promptly
        // even with long polling intervals
        let mut slept = 0;
        while running.load(Ordering::SeqCst) && slept < polling_interval {
            let slice = (polling_interval - slept).min(100);
            std::thread::sleep(Duration::from_millis(slice));
            slept += slice;
        }
    }

    Ok(())
}
