use std::time::Duration;

use anyhow::Result;

pub const VENDOR_ID: u16 = 0x2022;
pub const PRODUCT_ID: u16 = 0x0522;

/// Digit bytes the display renders as "no reading".
const NO_READING: (u8, u8, u8) = (238, 238, 238);

pub struct UsbDevice {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    endpoint: u8,
}

impl UsbDevice {
    pub fn open(vendor_id: u16, product_id: u16) -> Result<Self> {
        match rusb::open_device_with_vid_pid(vendor_id, product_id) {
            Some(handle) => {
                // Detach the kernel driver if it is attached
                if handle.kernel_driver_active(0).unwrap_or(false)
                    && let Err(e) = handle.detach_kernel_driver(0)
                {
                    eprintln!(
                        "Warning: failed to detach kernel driver from interface 0: {e:?}; \
                        claiming the interface may fail with Busy"
                    );
                }
                // Claim the interface so we can communicate with the device
                handle
                    .claim_interface(0)
                    .map_err(|e| anyhow::anyhow!("Error claiming interface: {e:?}"))?;

                // Find the interrupt OUT endpoint (do this once, not every send)
                let endpoint = handle
                    .device()
                    .config_descriptor(0)
                    .ok()
                    .and_then(|config| {
                        config
                            .interfaces()
                            .flat_map(|iface| iface.descriptors())
                            .flat_map(|desc| desc.endpoint_descriptors())
                            .find(|ep| {
                                ep.transfer_type() == rusb::TransferType::Interrupt
                                    && ep.direction() == rusb::Direction::Out
                            })
                            .map(|ep| ep.address())
                    })
                    .unwrap_or(0x03);

                eprintln!("USB device opened, endpoint: 0x{:02x}", endpoint);
                Ok(Self { handle, endpoint })
            }
            None => {
                // Check if device is visible but inaccessible (permission issue)
                let device_visible = rusb::devices()
                    .map(|devices| {
                        devices.iter().any(|d| {
                            d.device_descriptor()
                                .map(|desc| {
                                    desc.vendor_id() == VENDOR_ID && desc.product_id() == PRODUCT_ID
                                })
                                .unwrap_or(false)
                        })
                    })
                    .unwrap_or(false);

                if device_visible {
                    anyhow::bail!(
                        "Permission denied accessing USB device {VENDOR_ID:04x}:{PRODUCT_ID:04x}. \
                        Please ensure udev rules are properly configured."
                    )
                } else {
                    anyhow::bail!(
                        "USB device not found. Is it connected? \
                        Looking for device {VENDOR_ID:04x}:{PRODUCT_ID:04x}"
                    )
                }
            }
        }
    }

    /// Recover a stalled endpoint (CLEAR_FEATURE / ENDPOINT_HALT)
    pub fn clear_halt(&self) -> rusb::Result<()> {
        self.handle.clear_halt(self.endpoint)
    }

    /// Send a temperature frame. Errors are returned so the caller can decide
    /// which are fatal (e.g. device unplugged) and which are transient.
    pub fn send_payload(&self, cpu_temp: Option<f32>, gpu_temp: Option<f32>) -> rusb::Result<()> {
        let payload = generate_payload(cpu_temp, gpu_temp);
        self.handle
            .write_interrupt(self.endpoint, &payload, Duration::from_millis(1000))
            .map(|_| ())
    }
}

fn generate_payload(cpu_temp: Option<f32>, gpu_temp: Option<f32>) -> [u8; 12] {
    let cpu = encode_temperature(cpu_temp);
    let gpu = encode_temperature(gpu_temp);

    let mut payload = [
        85, 170, 1, 1, 6, // Header
        cpu.0, cpu.1, cpu.2, gpu.0, gpu.1, gpu.2, 0, // Checksum placeholder
    ];

    // Calculate checksum (sum of first 11 bytes)
    payload[11] = payload[..11]
        .iter()
        .fold(0u8, |acc, &b| acc.wrapping_add(b));
    payload
}

fn encode_temperature(temp: Option<f32>) -> (u8, u8, u8) {
    match temp {
        Some(t) if t >= 0.0 => {
            // The display has three digits: tens, ones, tenths. Work in
            // integer tenths so digits carry correctly, round in-between
            // readings (45.678 -> 45.7) instead of truncating, and saturate
            // at 99.9 since values >= 100 are not representable.
            let tenths = ((t * 10.0).round() as u32).min(999);
            (
                (tenths / 100) as u8,
                (tenths / 10 % 10) as u8,
                (tenths % 10) as u8,
            )
        }
        // No reading, or a negative/NaN value from a misbehaving sensor
        _ => NO_READING,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_generate_payload() {
        let actual = generate_payload(Some(24.0), Some(16.0));
        let expected = vec![85, 170, 1, 1, 6, 2, 4, 0, 1, 6, 0, 20];
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_generate_payload_with_no_gpu() {
        let actual = generate_payload(Some(24.0), None);
        let expected = vec![85, 170, 1, 1, 6, 2, 4, 0, 238, 238, 238, 215];
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_encode_temperature_saturates_above_display_range() {
        // >= 100 C must not produce digit bytes outside 0-9
        assert_eq!(encode_temperature(Some(100.5)), (9, 9, 9));
        assert_eq!(encode_temperature(Some(105.3)), (9, 9, 9));
        assert_eq!(encode_temperature(Some(f32::INFINITY)), (9, 9, 9));
    }

    #[test]
    fn test_shutdown_frame_blanks_both_readings() {
        // main() blanks the display on every exit path with (None, None).
        // Some(0.0) is not equivalent: it encodes as a plausible 00.0 and
        // reads as a real measurement rather than "daemon stopped".
        let blank = generate_payload(None, None);
        assert_eq!(&blank[5..11], &[238, 238, 238, 238, 238, 238]);
        assert_ne!(blank, generate_payload(Some(0.0), Some(0.0)));

        // The checksum must still be self-consistent for the blank frame
        assert_eq!(
            blank[11],
            blank[..11].iter().fold(0u8, |acc, &b| acc.wrapping_add(b))
        );
    }

    #[test]
    fn test_encode_temperature_rejects_invalid_readings() {
        assert_eq!(encode_temperature(Some(-5.0)), NO_READING);
        assert_eq!(encode_temperature(Some(f32::NAN)), NO_READING);
        assert_eq!(encode_temperature(None), NO_READING);
    }

    #[test]
    fn test_encode_temperature_rounds_to_nearest_tenth() {
        // per-digit truncation would show 45.6 for a 45678 millidegree reading
        assert_eq!(encode_temperature(Some(45.678)), (4, 5, 7));
        assert_eq!(encode_temperature(Some(99.9)), (9, 9, 9));
        assert_eq!(encode_temperature(Some(0.0)), (0, 0, 0));
    }
}
