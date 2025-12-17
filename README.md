# Antec Flux Pro Display

A Linux service that displays CPU and GPU temperatures on the [Antec Flux Pro](https://www.antec.com/product/case/flux-pro) case's built-in display.

## Features

- **CPU temperature** - Auto-detected from `/sys/class/hwmon/` or `/sys/class/thermal/`
- **NVIDIA GPU** - Via NVML (requires nvidia-drivers)
- **AMD GPU** - Via sysfs (amdgpu driver)
- **Intel GPU** - Via sysfs (i915/xe drivers, including Arc)
- **Systemd & OpenRC** service integration

## Installation

### Gentoo (recommended)

Add the overlay and install:

```bash
# Add overlay
sudo eselect repository add costis git https://github.com/costis-t/costis-overlay.git
sudo emerge --sync costis

# Install (NVIDIA enabled by default)
sudo emerge app-misc/antec-flux-pro-display

# Or with specific GPU support
sudo USE="nvidia amd intel" emerge app-misc/antec-flux-pro-display

# Start service
sudo systemctl enable --now antec-flux-pro-display
```

### From Source

Requires Rust toolchain (`cargo`, `rustc`). Install via your distro's package manager or [rustup.rs](https://rustup.rs).

```bash
# Clone and build
git clone https://github.com/costis-t/antec-flux-pro-display.git
cd antec-flux-pro-display
cargo build --release --features "nvidia,amd,intel"

# Install udev rules
sudo cp packaging/udev/99-antec-flux-pro-display.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger

# Run
./target/release/antec-flux-pro-display
```

## Configuration

Config file location (in order of priority):
1. `--config` CLI argument
2. `/etc/antec-flux-pro-display/config.toml`
3. `~/.config/antec-flux-pro-display/config.toml`

```toml
# CPU temperature device (auto-detected if not set)
# cpu_device = "/sys/class/hwmon/hwmon0/temp1_input"

# Polling interval in milliseconds (100-60000)
polling_interval = 1000
```

## Service Management

```bash
# systemd
sudo systemctl status antec-flux-pro-display
journalctl -u antec-flux-pro-display -f

# OpenRC
sudo rc-service antec-flux-pro-display status
```

## Troubleshooting

```bash
# Check USB device is connected
ls -la /dev/bus/usb/*/

# Check udev rules applied (should show plugdev group)
ls -la /dev/bus/usb/*/* | grep plugdev
```

## License

[GPL-3.0](LICENSE)

Based on work by [nishtahir](https://github.com/nishtahir/antec-flux-pro-display) and [AKoskovich](https://github.com/AKoskovich/antec_flux_pro_display_service).