# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.3]

### Security

- The udev rule no longer makes the display's USB node world-writable. It was
  `99-antec-flux-pro-display.rules` with `MODE="0666"`; it is now
  `70-antec-flux-pro-display.rules` with `MODE="0660"`, `GROUP="plugdev"` and
  `TAG+="uaccess"`.

  The rename is load-bearing twice over. `uaccess` only grants the active
  seat's user an ACL if the tag is set **before** `73-seat-late.rules` runs, so
  the file must sort before it. And a stale `99-` file from an older install
  sorts *after* `70-`, so its `MODE=` assignment would win — the Debian
  maintainer scripts now remove it via `dpkg-maintscript-helper rm_conffile`.

  **Upgrading from <= 0.1.2 outside dpkg?** Remove the old rule by hand:

  ```bash
  sudo rm -f /etc/udev/rules.d/99-antec-flux-pro-display.rules \
             /lib/udev/rules.d/99-antec-flux-pro-display.rules
  getent group plugdev >/dev/null || sudo groupadd -r plugdev
  sudo udevadm control --reload-rules
  sudo udevadm trigger --subsystem-match=usb \
      --attr-match=idVendor=2022 --attr-match=idProduct=0522
  ```

  The device node should then be `crw-rw----` (0660), not `crw-rw-rw-`.

### Added

- AMD (`amdgpu`) and Intel (`i915`/`xe`, including Arc) GPU temperature via
  sysfs hwmon, with a DRM-subsystem fallback that matches the bound driver.
  **Implemented but not yet verified on real hardware** — reports welcome.
- Systemd unit hardening: `ProtectSystem=strict`, `ProtectHome`,
  `PrivateTmp`, `NoNewPrivileges`, `ProtectKernel{Tunables,Modules,Logs}`,
  `ProtectControlGroups`, `Restrict{Realtime,SUIDSGID,Namespaces}`,
  `LockPersonality`.
- CI lints every feature subset, not just `--all-features`; the GPU backends
  are feature-gated, so `--all-features` alone never compiles the cfg
  combinations users actually build.

### Changed

- **All three GPU backends are now on by default.** `default = ["nvidia"]`
  meant `cargo deb` and a stock `cargo build` produced a binary structurally
  incapable of reading an AMD or Intel GPU, and the failure surfaced as
  "Failed to get Nvidia GPU" — indistinguishable from a driver problem.
  `amd` and `intel` are pure sysfs and pull in no dependencies; `nvidia` only
  dlopens `libnvidia-ml.so.1` at runtime.
- **The daemon now exits on fatal USB errors by design** and relies on its
  supervisor to restart it with a fresh handle — a stale handle never recovers
  after a replug. The shipped systemd unit handles this (`Restart=always`).
  **OpenRC users must run it under `supervise-daemon --respawn`**; plain
  `start-stop-daemon` leaves the service dead after an unplug or suspend.
- The systemd unit is no longer a dpkg conffile, so upgrades overwrite it
  silently. Local customizations belong in a drop-in:
  `sudo systemctl edit antec-flux-pro-display`.
- The CPU sensor is identified by hwmon `name`/`temp1_label` (`k10temp` Tctl,
  `coretemp`, `zenpower`) rather than a fixed hwmon index.
- Temperatures are encoded in integer tenths, rounded rather than truncated
  between readings (45.678 now shows 45.7, not 45.6), saturated at 99.9, and
  negative/NaN readings render as "no reading".
- The interrupt OUT endpoint is resolved once at open instead of on every send.
- `polling_interval` is clamped to 100 ms – 60 s; `cpu_device` is confined to
  `/sys` with no `..`.

### Fixed

- **A missing `--config` path is now fatal.** It was probed with the same
  `exists()` check as the implicit locations, so a typo silently fell through
  to built-in defaults and the service reported `active (running)` while
  ignoring every setting it was given.
- **Removed the blind `hwmon0` CPU fallback.** It accepted
  `/sys/class/hwmon/hwmon0/temp1_input` on existence alone. hwmon numbering is
  assigned in probe order, so `hwmon0` is whatever registered first — commonly
  an NVMe drive, whose ~41 °C is plausible enough as a CPU temperature that
  nobody would notice. Set `cpu_device` explicitly if auto-detection misses
  your sensor.
- **NVML with zero usable devices no longer claims the backend.**
  `device_count` was fetched, printed and discarded, so on a host where
  `libnvidia-ml.so.1` exists but no GPU is usable (bound to vfio, removed,
  left over from a swap) the NVIDIA backend was selected anyway and
  permanently masked the AMD and Intel probes.
- **The display is blanked on every exit path.** The shutdown frame sent
  `0.0`, which is a valid in-range reading and renders as a plausible `00.0`;
  it now sends the "no reading" sentinel. It was also unreachable on the paths
  that need it most — both fatal USB exits returned from inside the loop,
  leaving the last good temperatures frozen on the display.
- A stalled endpoint (`Pipe`) is cleared in place; persistent transient
  failures escalate to an exit after 10 consecutive errors.
- `SIGTERM`/Ctrl-C is no longer held up for a full polling interval.
- The Gentoo overlay URL in the README pointed at a repository that does not
  exist (`costis.git`); the overlay's remote is `costis-overlay.git`.

### Removed

- The unused `systemstat` dependency; nothing has referenced it since CPU
  temperature moved to sysfs hwmon.

[Unreleased]: https://github.com/costis-t/antec-flux-pro-display/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/costis-t/antec-flux-pro-display/releases/tag/v0.1.3
