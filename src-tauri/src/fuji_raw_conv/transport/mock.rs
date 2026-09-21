use super::{PtpTransport, TransportDeviceInfo, TransportFactory};
use crate::fuji_raw_conv::error::{FujiRawConvError, FujiRawConvErrorKind};
use crate::fuji_raw_conv::ptp::{PtpContainer, codes, container_type};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Recorded operations for assertions in unit tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockOp {
    Write { code: u16, transaction_id: u32 },
    Read,
    Reset,
    Close,
}

#[derive(Debug, Default)]
struct MockInner {
    ops: Vec<MockOp>,
    /// Queued responses for `read_container` (FIFO).
    inbound: VecDeque<PtpContainer>,
    closed: bool,
    fail_next_write: Option<FujiRawConvError>,
}

#[derive(Clone, Default)]
pub struct MockTransportHandle {
    inner: Arc<Mutex<MockInner>>,
}

impl MockTransportHandle {
    pub fn push_response(&self, container: PtpContainer) {
        self.inner.lock().unwrap().inbound.push_back(container);
    }

    #[allow(dead_code)] // Handy for upcoming A2/A3 mock scripts.
    pub fn push_ok_response(&self, code: u16, transaction_id: u32) {
        self.push_response(PtpContainer {
            type_: container_type::RESPONSE,
            code: codes::response::OK,
            transaction_id,
            params: Vec::new(),
            // Echo the operation code is not required; OK is enough.
            data: Vec::new(),
        });
        let _ = code;
    }

    pub fn ops(&self) -> Vec<MockOp> {
        self.inner.lock().unwrap().ops.clone()
    }

    pub fn was_closed(&self) -> bool {
        self.inner.lock().unwrap().closed
    }
}

pub struct MockTransport {
    info: TransportDeviceInfo,
    handle: MockTransportHandle,
}

impl MockTransport {
    pub fn new(info: TransportDeviceInfo, handle: MockTransportHandle) -> Self {
        Self { info, handle }
    }
}

impl PtpTransport for MockTransport {
    fn device_info(&self) -> &TransportDeviceInfo {
        &self.info
    }

    fn write_container(&mut self, container: &PtpContainer) -> Result<(), FujiRawConvError> {
        let mut inner = self.handle.inner.lock().unwrap();
        if let Some(err) = inner.fail_next_write.take() {
            return Err(err);
        }
        inner.ops.push(MockOp::Write {
            code: container.code,
            transaction_id: container.transaction_id,
        });
        Ok(())
    }

    fn read_container(&mut self, _timeout: Duration) -> Result<PtpContainer, FujiRawConvError> {
        let mut inner = self.handle.inner.lock().unwrap();
        inner.ops.push(MockOp::Read);
        inner.inbound.pop_front().ok_or_else(|| {
            FujiRawConvError::new(
                FujiRawConvErrorKind::Transport,
                "Mock transport has no queued response.",
            )
        })
    }

    fn reset_connection(&mut self) -> Result<(), FujiRawConvError> {
        self.handle.inner.lock().unwrap().ops.push(MockOp::Reset);
        Ok(())
    }

    fn close(&mut self) -> Result<(), FujiRawConvError> {
        let mut inner = self.handle.inner.lock().unwrap();
        inner.ops.push(MockOp::Close);
        inner.closed = true;
        Ok(())
    }
}

pub struct MockTransportFactory {
    pub devices: Vec<TransportDeviceInfo>,
    pub handle: MockTransportHandle,
}

impl TransportFactory for MockTransportFactory {
    fn list_fuji_devices(&self) -> Result<Vec<TransportDeviceInfo>, FujiRawConvError> {
        Ok(self.devices.clone())
    }

    fn open(&self, bus_id: &str) -> Result<Box<dyn PtpTransport>, FujiRawConvError> {
        let info = self
            .devices
            .iter()
            .find(|d| d.bus_id == bus_id)
            .cloned()
            .ok_or_else(|| {
                FujiRawConvError::new(FujiRawConvErrorKind::NoCamera, "Mock device not found.")
            })?;
        Ok(Box::new(MockTransport::new(info, self.handle.clone())))
    }
}
