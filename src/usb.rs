use std::time::Duration;

use anyhow::Result;

pub const VENDOR_ID: u16 = 0x2022;
pub const PRODUCT_ID: u16 = 0x0522;

pub struct UsbDevice {
    handle: rusb::DeviceHandle<rusb::GlobalContext>,
    endpoint: u8,
}

impl UsbDevice {
    pub fn open(vendor_id: u16, product_id: u16) -> Result<Self> {
        match rusb::open_device_with_vid_pid(vendor_id, product_id) {
            Some(handle) => {
                // Detach the kernel driver if it is attached
                if handle.kernel_driver_active(0).unwrap_or(false) {
                    handle.detach_kernel_driver(0).unwrap_or(());
                }
                // Claim the interface so we can communicate with the device
                handle.claim_interface(0).map_err(|e| {
                    anyhow::anyhow!("Error claiming interface: {e:?}")
                })?;
                
                // Find the interrupt OUT endpoint (do this once, not every send)
                let endpoint = handle.device()
                    .config_descriptor(0)
                    .ok()
                    .and_then(|config| {
                        config.interfaces()
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

    pub fn send_payload(&self, cpu_temp: &Option<f32>, gpu_temp: &Option<f32>) {
        let payload = generate_payload(cpu_temp, gpu_temp);

        if let Err(e) = self.handle.write_interrupt(self.endpoint, &payload, Duration::from_millis(1000)) {
            eprintln!("Error writing to USB device: {e:?}");
        }
    }
}

fn generate_payload(cpu_temp: &Option<f32>, gpu_temp: &Option<f32>) -> [u8; 12] {
    let cpu = encode_temperature(cpu_temp);
    let gpu = encode_temperature(gpu_temp);
    
    let mut payload = [
        85, 170, 1, 1, 6,  // Header
        cpu.0, cpu.1, cpu.2,
        gpu.0, gpu.1, gpu.2,
        0,  // Checksum placeholder
    ];
    
    // Calculate checksum (sum of first 11 bytes)
    payload[11] = payload[..11].iter().fold(0u8, |acc, &b| acc.wrapping_add(b));
    payload
}

fn encode_temperature(temp: &Option<f32>) -> (u8, u8, u8) {
    if let Some(temp) = temp {
        let ones = (temp / 10.0) as u8;
        let tens = (temp % 10.0) as u8;
        let tenths = ((temp * 10.0) % 10.0) as u8;
        return (ones, tens, tenths);
    }
    (238, 238, 238)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_generate_payload() {
        let actual = generate_payload(&Some(24.0), &Some(16.0));
        let expected = vec![85, 170, 1, 1, 6, 2, 4, 0, 1, 6, 0, 20];
        assert_eq!(expected, actual);
    }

    #[test]
    fn test_generate_payload_with_no_gpu() {
        let actual = generate_payload(&Some(24.0), &None);
        let expected = vec![85, 170, 1, 1, 6, 2, 4, 0, 238, 238, 238, 215];
        assert_eq!(expected, actual);
    }
}
