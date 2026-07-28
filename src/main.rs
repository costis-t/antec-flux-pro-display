mod config;
mod cpu;
mod gpu;
mod sensors;
mod usb;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{path::PathBuf, time::Duration};

use anyhow::Result;
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

    // Determine config path: CLI arg > system config > user config
    let config_path = if let Some(ref path) = cli.config {
        PathBuf::from(shellexpand::tilde(path).to_string())
    } else if PathBuf::from(SYSTEM_CONFIG_PATH).exists() {
        PathBuf::from(SYSTEM_CONFIG_PATH)
    } else {
        PathBuf::from(shellexpand::tilde(USER_CONFIG_PATH).to_string())
    };

    // Load config or use defaults if not found (don't try to create - may not have write perms)
    let config = if config_path.exists() {
        println!("Using config: {}", config_path.display());
        Config::from_file(&config_path)?.validated()
    } else {
        eprintln!(
            "Config file not found at: {}, using defaults",
            config_path.display()
        );
        Config::default()
    };

    let running = Arc::new(AtomicBool::new(true));
    let device = UsbDevice::open(usb::VENDOR_ID, usb::PRODUCT_ID)?;
    let cpu = config.cpu_device.or_else(default_cpu_device);
    let gpu = AvailableGpu::get_available_gpu();

    // Handle CTRL+C and other termination gracefully
    let run = running.clone();
    ctrlc::set_handler(move || {
        run.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Escalate persistent transient errors (e.g. a wedged device that only
    // ever times out) to an exit so the service manager can restart us
    const MAX_CONSECUTIVE_ERRORS: u32 = 10;
    let mut consecutive_errors: u32 = 0;

    // Loop until the program is terminated
    while running.load(Ordering::SeqCst) {
        let cpu_temp = cpu.as_deref().and_then(cpu::read_temp);
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
        while running.load(Ordering::SeqCst) && slept < config.polling_interval {
            let slice = (config.polling_interval - slept).min(100);
            std::thread::sleep(Duration::from_millis(slice));
            slept += slice;
        }
    }

    // Finally, set the temps to zero before exiting as a "daemon stopped"
    // indicator (the device may already be gone, so ignore errors)
    let _ = device.send_payload(Some(0.0), Some(0.0));

    Ok(())
}
