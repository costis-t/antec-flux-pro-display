mod config;
mod cpu;
mod gpu;
mod usb;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use clap::Parser;

use config::{Config, FromConfigFile};
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
        Config::from_config_file(&config_path)?.validated()
    } else {
        eprintln!("Config file not found at: {}, using defaults", config_path.display());
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

    // Loop until the program is terminated
    while running.load(Ordering::SeqCst) {
        let cpu_temp = &cpu.as_ref().and_then(|path| cpu::read_temp(path));
        let gpu_temp = &gpu.temp();

        device.send_payload(cpu_temp, gpu_temp);
        std::thread::sleep(Duration::from_millis(config.polling_interval));
    }

    // Finally, set the temps to zero before exiting
    device.send_payload(&Some(0.0), &Some(0.0));

    Ok(())
}
