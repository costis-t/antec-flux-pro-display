# Antec Flux Pro Display

A Linux service that displays CPU and GPU temperatures on the [Antec Flux Pro](https://www.antec.com/product/case/flux-pro) case's built-in display.

Many thanks to [nishtahir](https://github.com/nishtahir/antec-flux-pro-display), his [work](https://nishtahir.com/building-an-ubuntu-service-for-my-antec-flux-display/) with [Ghida](https://ghidralite.com/), and [AKoskovich](https://github.com/AKoskovich/antec_flux_pro_display_service) for the original work.

## Features
Tested on Gentoo (systemd) with an NVIDIA GPU. AMD and Intel support is
implemented but not yet verified on real hardware — reports welcome.

- **CPU temperature** - Auto-detected from `/sys/class/hwmon/` (k10temp/coretemp/zenpower) or `/sys/class/thermal/`
- **NVIDIA GPU** - via NVML (requires nvidia-drivers)
- **AMD GPU** - via sysfs (amdgpu driver)
- **Intel GPU** - via sysfs (i915/xe drivers, including Arc)
- **Systemd** - service integration (this repo ships a systemd unit; the Gentoo overlay package also provides an OpenRC init script)

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

# Remove udev rules from previous versions (they made the device world-writable)
sudo rm -f /etc/udev/rules.d/99-antec-flux-pro-display.rules \
           /lib/udev/rules.d/99-antec-flux-pro-display.rules

# The rule grants access via the plugdev group; create it if missing
getent group plugdev >/dev/null || sudo groupadd -r plugdev

# Install udev rules
sudo cp packaging/udev/70-antec-flux-pro-display.rules /etc/udev/rules.d/
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

Note: the packaged systemd service runs with `ProtectHome=true`, so it only
reads `/etc/antec-flux-pro-display/config.toml`.

All keys are optional; defaults are used for anything omitted.

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

# OpenRC (init script provided by the Gentoo overlay package)
sudo rc-service antec-flux-pro-display status
```

The daemon deliberately exits on fatal USB errors (e.g. the display is
unplugged) so its supervisor can restart it with a fresh device handle. The
shipped systemd unit does this automatically (`Restart=always`); OpenRC users
should run it under `supervise-daemon` with `--respawn`. A manual terminal run
will simply exit on unplug — just start it again.

To customize the packaged systemd unit, use a drop-in
(`sudo systemctl edit antec-flux-pro-display`) rather than editing the unit
file under `/lib`, which is overwritten on upgrade.

## Troubleshooting

```bash
# Check USB device is connected
lsusb -d 2022:0522

# Check udev rules applied: the device node should be mode 0660 with an
# ACL for your seat (group shows plugdev only if that group exists)
ls -la $(lsusb -d 2022:0522 | awk '{printf "/dev/bus/usb/%s/%03d", $2, $4}')

# For manual (non-root) runs without a seat ACL, join plugdev:
# getent group plugdev >/dev/null || sudo groupadd -r plugdev
# sudo usermod -aG plugdev $USER   (then log out and back in)

# If the mode shows 0666, a stale rules file from an older version is
# still installed and overrides the current one — remove it:
# sudo rm -f /etc/udev/rules.d/99-antec-flux-pro-display.rules \
#            /lib/udev/rules.d/99-antec-flux-pro-display.rules
# then reload rules and re-trigger.
```

On headless NVIDIA systems the hardened systemd unit
(`ProtectKernelModules=true`) prevents NVML from auto-loading the `nvidia`
kernel module; if GPU temperature is missing, load it at boot via
`/etc/modules-load.d/nvidia.conf`.

## License

[GPL-3.0](LICENSE)

Based on work by [nishtahir](https://github.com/nishtahir/antec-flux-pro-display) and [AKoskovich](https://github.com/AKoskovich/antec_flux_pro_display_service).