//! nusb-backed PTP bulk transport.
//!
//! This is the only module allowed to import `nusb`. Session and preset code
//! talk exclusively through [`super::PtpTransport`].

use super::{PtpTransport, TransportDeviceInfo, TransportFactory};
use crate::fuji_raw_conv::error::{FujiRawConvError, FujiRawConvErrorKind};
use crate::fuji_raw_conv::platform;
use crate::fuji_raw_conv::ptp::container::HEADER_SIZE;
use crate::fuji_raw_conv::ptp::{PtpContainer, codes};
use nusb::descriptors::TransferType;
use nusb::transfer::{Bulk, Direction, In, Out};
use nusb::{Device, DeviceInfo, Interface, MaybeFuture};
use std::io::{Read, Write};
use std::time::Duration;

const READ_CHUNK: usize = 64 * 1024;

pub struct NusbTransportFactory;

impl TransportFactory for NusbTransportFactory {
    fn list_fuji_devices(&self) -> Result<Vec<TransportDeviceInfo>, FujiRawConvError> {
        let devices = list_devices_raw()?;
        Ok(devices
            .into_iter()
            .filter(|d| d.vendor_id() == codes::FUJI_VENDOR_ID)
            .map(device_info_from_nusb)
            .collect())
    }

    fn open(&self, bus_id: &str) -> Result<Box<dyn PtpTransport>, FujiRawConvError> {
        let info = list_devices_raw()?
            .into_iter()
            .find(|d| d.vendor_id() == codes::FUJI_VENDOR_ID && d.bus_id() == bus_id)
            .ok_or_else(|| {
                FujiRawConvError::new(
                    FujiRawConvErrorKind::NoCamera,
                    "No Fujifilm camera found on the selected USB port.",
                )
                .with_recovery("Check the cable and confirm the camera is powered on.")
            })?;

        let transport = NusbTransport::open(info)?;
        Ok(Box::new(transport))
    }
}

fn list_devices_raw() -> Result<Vec<DeviceInfo>, FujiRawConvError> {
    nusb::list_devices()
        .wait()
        .map(|iter| iter.collect())
        .map_err(map_nusb_list_error)
}

fn device_info_from_nusb(info: DeviceInfo) -> TransportDeviceInfo {
    TransportDeviceInfo {
        bus_id: info.bus_id().to_string(),
        vendor_id: info.vendor_id(),
        product_id: info.product_id(),
        manufacturer: info.manufacturer_string().map(|s| s.to_string()),
        product: info.product_string().map(|s| s.to_string()),
        serial_number: info.serial_number().map(|s| s.to_string()),
    }
}

fn map_nusb_list_error(err: nusb::Error) -> FujiRawConvError {
    let msg = err.to_string();
    if platform::looks_like_missing_winusb(&msg) {
        return platform::winusb_required_error().with_detail(msg);
    }
    FujiRawConvError::new(
        FujiRawConvErrorKind::Transport,
        "Failed to list USB devices.",
    )
    .with_detail(msg)
}

fn map_nusb_open_error(err: nusb::Error) -> FujiRawConvError {
    let msg = err.to_string();
    if platform::looks_like_missing_winusb(&msg) {
        return platform::winusb_required_error().with_detail(msg);
    }
    let lower = msg.to_lowercase();
    if lower.contains("access") || lower.contains("permission") {
        return FujiRawConvError::new(
            FujiRawConvErrorKind::PlatformDriver,
            "Permission denied opening the camera USB interface.",
        )
        .with_detail(msg)
        .with_recovery(platform::permission_recovery_hint());
    }
    if lower.contains("busy") || lower.contains("claimed") {
        return FujiRawConvError::new(
            FujiRawConvErrorKind::Transport,
            "The camera USB interface is busy. Another application may be using it.",
        )
        .with_detail(msg)
        .with_recovery("Quit X RAW Studio, tethering software, or other USB tools and try again.");
    }
    FujiRawConvError::new(
        FujiRawConvErrorKind::Transport,
        "Failed to open the camera USB interface.",
    )
    .with_detail(msg)
}

struct NusbTransport {
    info: TransportDeviceInfo,
    /// Kept alive so the interface claim remains valid.
    _device: Device,
    interface: Interface,
    ep_out_addr: u8,
    ep_in_addr: u8,
}

impl NusbTransport {
    fn open(device_info: DeviceInfo) -> Result<Self, FujiRawConvError> {
        let info = device_info_from_nusb(device_info.clone());
        let device = device_info.open().wait().map_err(map_nusb_open_error)?;

        if let Err(err) = device.set_configuration(1).wait() {
            log::debug!("set_configuration(1) failed (may already be active): {err}");
        }

        let interface = device
            .claim_interface(0)
            .wait()
            .map_err(map_nusb_open_error)?;

        let (ep_out_addr, ep_in_addr) = find_bulk_endpoints(&interface)?;

        Ok(Self {
            info,
            _device: device,
            interface,
            ep_out_addr,
            ep_in_addr,
        })
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), FujiRawConvError> {
        let endpoint = self
            .interface
            .endpoint::<Bulk, Out>(self.ep_out_addr)
            .map_err(|e| {
                FujiRawConvError::new(
                    FujiRawConvErrorKind::Transport,
                    "Failed to claim bulk OUT endpoint.",
                )
                .with_detail(e.to_string())
            })?;
        let mut writer = endpoint.writer(READ_CHUNK);
        writer.write_all(bytes).map_err(|e| {
            FujiRawConvError::new(FujiRawConvErrorKind::Transport, "USB bulk write failed.")
                .with_detail(e.to_string())
        })?;
        writer.flush().map_err(|e| {
            FujiRawConvError::new(FujiRawConvErrorKind::Transport, "USB bulk flush failed.")
                .with_detail(e.to_string())
        })?;
        Ok(())
    }

    fn read_exact_container(&mut self, timeout: Duration) -> Result<PtpContainer, FujiRawConvError> {
        let endpoint = self
            .interface
            .endpoint::<Bulk, In>(self.ep_in_addr)
            .map_err(|e| {
                FujiRawConvError::new(
                    FujiRawConvErrorKind::Transport,
                    "Failed to claim bulk IN endpoint.",
                )
                .with_detail(e.to_string())
            })?;
        let mut reader = endpoint.reader(READ_CHUNK).with_read_timeout(timeout);

        let mut header = [0u8; HEADER_SIZE];
        reader.read_exact(&mut header).map_err(|e| {
            FujiRawConvError::new(
                FujiRawConvErrorKind::Transport,
                "USB bulk read failed while waiting for a PTP header.",
            )
            .with_detail(e.to_string())
        })?;

        let length = PtpContainer::declared_length(&header).ok_or_else(|| {
            FujiRawConvError::new(
                FujiRawConvErrorKind::Protocol,
                "Could not parse PTP container length.",
            )
        })?;
        if length < HEADER_SIZE {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::Protocol,
                "PTP container length is smaller than the header.",
            ));
        }

        let mut buf = Vec::with_capacity(length);
        buf.extend_from_slice(&header);
        if length > HEADER_SIZE {
            buf.resize(length, 0);
            reader.read_exact(&mut buf[HEADER_SIZE..]).map_err(|e| {
                FujiRawConvError::new(
                    FujiRawConvErrorKind::Transport,
                    "USB bulk read failed while reading a PTP payload.",
                )
                .with_detail(e.to_string())
            })?;
        }

        PtpContainer::unpack(&buf)
    }
}

impl PtpTransport for NusbTransport {
    fn device_info(&self) -> &TransportDeviceInfo {
        &self.info
    }

    fn write_container(&mut self, container: &PtpContainer) -> Result<(), FujiRawConvError> {
        self.write_all(&container.pack())
    }

    fn read_container(&mut self, timeout: Duration) -> Result<PtpContainer, FujiRawConvError> {
        self.read_exact_container(timeout)
    }

    fn reset_connection(&mut self) -> Result<(), FujiRawConvError> {
        // Soft reset is handled by Session::recover (close + reopen via factory).
        Ok(())
    }

    fn close(&mut self) -> Result<(), FujiRawConvError> {
        Ok(())
    }
}

fn find_bulk_endpoints(interface: &Interface) -> Result<(u8, u8), FujiRawConvError> {
    let alt = interface.descriptor().ok_or_else(|| {
        FujiRawConvError::wrong_usb_mode().with_detail(
            "No active USB interface descriptor found on the camera.",
        )
    })?;

    let mut ep_out = None;
    let mut ep_in = None;
    for ep in alt.endpoints() {
        if ep.transfer_type() != TransferType::Bulk {
            continue;
        }
        match ep.direction() {
            Direction::Out => ep_out = Some(ep.address()),
            Direction::In => ep_in = Some(ep.address()),
        }
    }

    match (ep_out, ep_in) {
        (Some(out), Some(inn)) => Ok((out, inn)),
        _ => Err(FujiRawConvError::wrong_usb_mode().with_detail(
            "Camera USB interface has no bulk IN/OUT pair. This usually means it is not in USB RAW CONV. mode.",
        )),
    }
}
