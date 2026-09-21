//! USB/PTP byte transport. Concrete backends live behind this trait so mocks
//! (and a future rusb fallback) never leak into session/preset/UI code.

use crate::fuji_raw_conv::error::FujiRawConvError;
use crate::fuji_raw_conv::ptp::PtpContainer;
use std::time::Duration;

#[cfg(test)]
pub mod mock;
#[cfg(feature = "fuji-raw-conv")]
pub mod nusb_transport;

/// Identity of a discovered imaging device before a session is opened.
#[derive(Debug, Clone)]
pub struct TransportDeviceInfo {
    pub bus_id: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial_number: Option<String>,
}

/// Lowest-level PTP I/O: send a command container, optionally a data phase,
/// then read response (and optional data-in).
pub trait PtpTransport: Send {
    fn device_info(&self) -> &TransportDeviceInfo;

    fn write_container(&mut self, container: &PtpContainer) -> Result<(), FujiRawConvError>;

    fn read_container(&mut self, timeout: Duration) -> Result<PtpContainer, FujiRawConvError>;

    /// Best-effort USB reclaim after a broken session (release/reclaim interface).
    fn reset_connection(&mut self) -> Result<(), FujiRawConvError>;

    fn close(&mut self) -> Result<(), FujiRawConvError>;
}

/// Factory used by session code / Tauri commands. Feature builds use nusb;
/// tests inject [`mock::MockTransportFactory`].
pub trait TransportFactory: Send + Sync {
    fn list_fuji_devices(&self) -> Result<Vec<TransportDeviceInfo>, FujiRawConvError>;
    fn open(&self, bus_id: &str) -> Result<Box<dyn PtpTransport>, FujiRawConvError>;
}
