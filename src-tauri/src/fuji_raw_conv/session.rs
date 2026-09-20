//! PTP session lifecycle: transaction IDs, Open/CloseSession, Drop cleanup,
//! and an explicit recover path after a broken conversation.

use crate::fuji_raw_conv::error::{FujiRawConvError, FujiRawConvErrorKind, PtpErrorContext};
use crate::fuji_raw_conv::ptp::{PtpContainer, codes, container_type};
use crate::fuji_raw_conv::transport::{PtpTransport, TransportDeviceInfo, TransportFactory};
use std::time::Duration;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_SESSION_ID: u32 = 1;

/// Active PTP session owning the transport.
pub struct PtpSession {
    transport: Box<dyn PtpTransport>,
    next_transaction_id: u32,
    session_id: u32,
    open: bool,
    /// Prevents CloseSession during intentional shutdown / recover.
    suppress_drop_close: bool,
}

impl PtpSession {
    pub fn open(
        transport: Box<dyn PtpTransport>,
        session_id: u32,
    ) -> Result<Self, FujiRawConvError> {
        let mut session = Self {
            transport,
            next_transaction_id: 1,
            session_id,
            open: false,
            suppress_drop_close: false,
        };
        session.open_session_inner()?;
        Ok(session)
    }

    pub fn device_info(&self) -> &TransportDeviceInfo {
        self.transport.device_info()
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn next_transaction_id(&mut self) -> u32 {
        let id = self.next_transaction_id;
        self.next_transaction_id = self.next_transaction_id.wrapping_add(1).max(1);
        id
    }

    /// Send a command with no data phase and return the response container.
    pub fn transact_command(
        &mut self,
        opcode: u16,
        params: &[u32],
        context: PtpErrorContext,
    ) -> Result<PtpContainer, FujiRawConvError> {
        if !self.open {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::SessionClosed,
                "No open PTP session with the camera.",
            )
            .with_recovery("Reconnect the camera, or use Recover Session."));
        }

        let tid = self.next_transaction_id();
        let cmd = PtpContainer::command(opcode, tid, params);
        self.transport.write_container(&cmd)?;

        let response = self.read_until_response(tid)?;
        if response.code != codes::response::OK {
            return Err(FujiRawConvError::from_ptp_response(response.code, context));
        }
        Ok(response)
    }

    /// Command that expects a data-in phase before the response (e.g. GetDeviceInfo).
    pub fn transact_command_with_data_in(
        &mut self,
        opcode: u16,
        params: &[u32],
        context: PtpErrorContext,
    ) -> Result<(PtpContainer, Vec<u8>), FujiRawConvError> {
        if !self.open {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::SessionClosed,
                "No open PTP session with the camera.",
            ));
        }

        let tid = self.next_transaction_id();
        let cmd = PtpContainer::command(opcode, tid, params);
        self.transport.write_container(&cmd)?;

        let mut data_payload = Vec::new();
        loop {
            let container = self.transport.read_container(DEFAULT_TIMEOUT)?;
            if container.transaction_id != tid {
                return Err(FujiRawConvError::new(
                    FujiRawConvErrorKind::Protocol,
                    "PTP transaction ID mismatch.",
                )
                .with_detail(format!(
                    "expected {tid}, got {}",
                    container.transaction_id
                )));
            }
            match container.type_ {
                container_type::DATA => {
                    data_payload = container.data;
                }
                container_type::RESPONSE => {
                    if container.code != codes::response::OK {
                        return Err(FujiRawConvError::from_ptp_response(container.code, context));
                    }
                    return Ok((container, data_payload));
                }
                other => {
                    return Err(FujiRawConvError::new(
                        FujiRawConvErrorKind::Protocol,
                        "Unexpected PTP container during data-in transaction.",
                    )
                    .with_detail(format!("type=0x{other:04X}")));
                }
            }
        }
    }

    /// Command with a data-out phase (SetDevicePropValue, SendObject, …).
    pub fn transact_command_with_data_out(
        &mut self,
        opcode: u16,
        params: &[u32],
        payload: Vec<u8>,
        context: PtpErrorContext,
        timeout: Duration,
    ) -> Result<PtpContainer, FujiRawConvError> {
        if !self.open {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::SessionClosed,
                "No open PTP session with the camera.",
            ));
        }

        let tid = self.next_transaction_id();
        let cmd = PtpContainer::command(opcode, tid, params);
        self.transport.write_container(&cmd)?;
        let data = PtpContainer::data(opcode, tid, payload);
        self.transport.write_container(&data)?;

        // Some cameras may insert an empty DATA-in; accept RESPONSE.
        let mut attempts = 0;
        loop {
            let container = self.transport.read_container(timeout)?;
            if container.transaction_id != tid {
                return Err(FujiRawConvError::new(
                    FujiRawConvErrorKind::Protocol,
                    "PTP transaction ID mismatch.",
                )
                .with_detail(format!(
                    "expected {tid}, got {}",
                    container.transaction_id
                )));
            }
            match container.type_ {
                container_type::RESPONSE => {
                    if container.code != codes::response::OK {
                        return Err(FujiRawConvError::from_ptp_response(container.code, context));
                    }
                    return Ok(container);
                }
                container_type::DATA => {
                    attempts += 1;
                    if attempts > 2 {
                        return Err(FujiRawConvError::new(
                            FujiRawConvErrorKind::Protocol,
                            "Unexpected DATA containers after data-out command.",
                        ));
                    }
                }
                other => {
                    return Err(FujiRawConvError::new(
                        FujiRawConvErrorKind::Protocol,
                        "Unexpected PTP container during data-out transaction.",
                    )
                    .with_detail(format!("type=0x{other:04X}")));
                }
            }
        }
    }

    pub fn set_device_prop_raw(
        &mut self,
        prop_id: u16,
        payload: Vec<u8>,
        context: PtpErrorContext,
    ) -> Result<(), FujiRawConvError> {
        self.transact_command_with_data_out(
            codes::op::SET_DEVICE_PROP_VALUE,
            &[u32::from(prop_id)],
            payload,
            context,
            DEFAULT_TIMEOUT,
        )?;
        Ok(())
    }

    pub fn get_device_prop_raw(
        &mut self,
        prop_id: u16,
        context: PtpErrorContext,
    ) -> Result<Vec<u8>, FujiRawConvError> {
        let (_resp, data) = self.transact_command_with_data_in(
            codes::op::GET_DEVICE_PROP_VALUE,
            &[u32::from(prop_id)],
            context,
        )?;
        Ok(data)
    }

    fn read_until_response(&mut self, tid: u32) -> Result<PtpContainer, FujiRawConvError> {
        let container = self.transport.read_container(DEFAULT_TIMEOUT)?;
        if container.transaction_id != tid {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::Protocol,
                "PTP transaction ID mismatch.",
            )
            .with_detail(format!(
                "expected {tid}, got {}",
                container.transaction_id
            )));
        }
        if container.type_ != container_type::RESPONSE {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::Protocol,
                "Expected a PTP response container.",
            )
            .with_detail(format!("type=0x{:04X}", container.type_)));
        }
        Ok(container)
    }

    fn open_session_inner(&mut self) -> Result<(), FujiRawConvError> {
        let tid = self.next_transaction_id();
        let cmd = PtpContainer::command(codes::op::OPEN_SESSION, tid, &[self.session_id]);
        self.transport.write_container(&cmd)?;
        let response = self.read_until_response(tid)?;
        match response.code {
            codes::response::OK => {
                self.open = true;
                Ok(())
            }
            codes::response::SESSION_ALREADY_OPEN => Err(FujiRawConvError::from_ptp_response(
                response.code,
                PtpErrorContext::Generic,
            )),
            other => Err(FujiRawConvError::from_ptp_response(
                other,
                PtpErrorContext::CapabilityProbe,
            )),
        }
    }

    pub fn close_session(&mut self) -> Result<(), FujiRawConvError> {
        if !self.open {
            return Ok(());
        }
        let tid = self.next_transaction_id();
        let cmd = PtpContainer::command(codes::op::CLOSE_SESSION, tid, &[]);
        // Best-effort: even if write/read fails, mark closed so Drop does not loop.
        let write_result = self.transport.write_container(&cmd);
        let read_result = self.transport.read_container(DEFAULT_TIMEOUT);
        self.open = false;
        write_result?;
        let response = read_result?;
        if response.code != codes::response::OK && response.code != codes::response::SESSION_NOT_OPEN
        {
            return Err(FujiRawConvError::from_ptp_response(
                response.code,
                PtpErrorContext::Generic,
            ));
        }
        Ok(())
    }

    /// Probe for Fujifilm RAW conversion device properties.
    pub fn probe_raw_conv_capability(&mut self) -> Result<(), FujiRawConvError> {
        // Prefer GetDevicePropDesc on StartRawConversion; failure ⇒ wrong USB mode.
        match self.transact_command_with_data_in(
            codes::op::GET_DEVICE_PROP_DESC,
            &[u32::from(codes::fuji::PROP_START_RAW_CONVERSION)],
            PtpErrorContext::CapabilityProbe,
        ) {
            Ok(_) => Ok(()),
            Err(err) if err.kind == FujiRawConvErrorKind::WrongUsbMode => Err(err),
            Err(err) if err.kind == FujiRawConvErrorKind::Protocol => {
                // Fallback: try RawConvProfile prop.
                match self.transact_command_with_data_in(
                    codes::op::GET_DEVICE_PROP_DESC,
                    &[u32::from(codes::fuji::PROP_RAW_CONV_PROFILE)],
                    PtpErrorContext::CapabilityProbe,
                ) {
                    Ok(_) => Ok(()),
                    Err(_) => Err(FujiRawConvError::wrong_usb_mode().with_detail(err.to_string())),
                }
            }
            Err(err) => Err(err),
        }
    }

    /// Take ownership of the transport after closing the PTP session (for recover).
    pub fn into_transport(mut self) -> Box<dyn PtpTransport> {
        self.suppress_drop_close = true;
        let _ = self.close_session();
        // Manually extract transport without running Drop close again.
        let mut transport = std::mem::replace(
            &mut self.transport,
            Box::new(ClosedTransportPlaceholder {
                info: TransportDeviceInfo {
                    bus_id: String::new(),
                    vendor_id: 0,
                    product_id: 0,
                    manufacturer: None,
                    product: None,
                    serial_number: None,
                },
            }),
        );
        let _ = transport.close();
        // Prevent Drop from touching placeholder meaningfully.
        self.open = false;
        transport
    }
}

impl Drop for PtpSession {
    fn drop(&mut self) {
        if self.suppress_drop_close {
            return;
        }
        if self.open {
            // Best-effort CloseSession so an aborted conversation does not leave
            // the camera wedged until power-cycle.
            let tid = self.next_transaction_id;
            self.next_transaction_id = self.next_transaction_id.wrapping_add(1).max(1);
            let cmd = PtpContainer::command(codes::op::CLOSE_SESSION, tid, &[]);
            let _ = self.transport.write_container(&cmd);
            let _ = self.transport.read_container(Duration::from_secs(2));
            self.open = false;
        }
        let _ = self.transport.close();
    }
}

/// Placeholder so `into_transport` can mem::replace the Box.
struct ClosedTransportPlaceholder {
    info: TransportDeviceInfo,
}

impl PtpTransport for ClosedTransportPlaceholder {
    fn device_info(&self) -> &TransportDeviceInfo {
        &self.info
    }
    fn write_container(&mut self, _: &PtpContainer) -> Result<(), FujiRawConvError> {
        Ok(())
    }
    fn read_container(&mut self, _: Duration) -> Result<PtpContainer, FujiRawConvError> {
        Err(FujiRawConvError::new(
            FujiRawConvErrorKind::SessionClosed,
            "Transport placeholder.",
        ))
    }
    fn reset_connection(&mut self) -> Result<(), FujiRawConvError> {
        Ok(())
    }
    fn close(&mut self) -> Result<(), FujiRawConvError> {
        Ok(())
    }
}

/// App-level handle stored in `AppState`.
pub struct FujiSessionHandle {
    pub session: Option<PtpSession>,
    pub bus_id: Option<String>,
}

impl FujiSessionHandle {
    pub fn new() -> Self {
        Self {
            session: None,
            bus_id: None,
        }
    }

    pub fn connect_with_factory(
        &mut self,
        factory: &dyn TransportFactory,
        bus_id: &str,
    ) -> Result<&TransportDeviceInfo, FujiRawConvError> {
        // Drop any previous session first (CloseSession via Drop).
        self.session = None;
        self.bus_id = None;

        let transport = factory.open(bus_id)?;
        let mut session = PtpSession::open(transport, DEFAULT_SESSION_ID)?;
        if let Err(err) = session.probe_raw_conv_capability() {
            // Leave camera tidy before surfacing the mode error.
            let _ = session.close_session();
            return Err(err);
        }
        self.bus_id = Some(bus_id.to_string());
        self.session = Some(session);
        Ok(self.session.as_ref().unwrap().device_info())
    }

    pub fn disconnect(&mut self) {
        if let Some(mut session) = self.session.take() {
            let _ = session.close_session();
        }
        self.bus_id = None;
    }

    /// Explicit recovery after a broken PTP exchange: close, reset USB if
    /// possible, reopen session on the same bus id.
    pub fn recover_with_factory(
        &mut self,
        factory: &dyn TransportFactory,
    ) -> Result<(), FujiRawConvError> {
        let bus_id = self.bus_id.clone().ok_or_else(|| {
            FujiRawConvError::new(
                FujiRawConvErrorKind::NoCamera,
                "No camera session to recover. Connect a camera first.",
            )
        })?;

        if let Some(session) = self.session.take() {
            let mut transport = session.into_transport();
            let _ = transport.reset_connection();
            let _ = transport.close();
        }

        self.connect_with_factory(factory, &bus_id)?;
        Ok(())
    }
}

impl Default for FujiSessionHandle {
    fn default() -> Self {
        Self::new()
    }
}

// Make the handle Sync-friendly for AppState Mutex (transport is Send).
unsafe impl Send for FujiSessionHandle {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuji_raw_conv::ptp::codes;
    use crate::fuji_raw_conv::transport::mock::{MockOp, MockTransportFactory, MockTransportHandle};
    use crate::fuji_raw_conv::transport::TransportDeviceInfo;

    fn sample_device() -> TransportDeviceInfo {
        TransportDeviceInfo {
            bus_id: "bus-1".into(),
            vendor_id: codes::FUJI_VENDOR_ID,
            product_id: 0x0305,
            manufacturer: Some("FUJIFILM".into()),
            product: Some("X100VI".into()),
            serial_number: Some("SN".into()),
        }
    }

    fn queue_open_and_probe(handle: &MockTransportHandle) {
        // OpenSession response
        handle.push_response(PtpContainer {
            type_: container_type::RESPONSE,
            code: codes::response::OK,
            transaction_id: 1,
            params: vec![],
            data: vec![],
        });
        // GetDevicePropDesc data + response for StartRawConversion
        handle.push_response(PtpContainer {
            type_: container_type::DATA,
            code: codes::op::GET_DEVICE_PROP_DESC,
            transaction_id: 2,
            params: vec![],
            data: vec![0x01, 0x02],
        });
        handle.push_response(PtpContainer {
            type_: container_type::RESPONSE,
            code: codes::response::OK,
            transaction_id: 2,
            params: vec![],
            data: vec![],
        });
    }

    #[test]
    fn open_assigns_monotonic_transaction_ids_and_drop_closes() {
        let handle = MockTransportHandle::default();
        queue_open_and_probe(&handle);
        // CloseSession on Drop
        handle.push_response(PtpContainer {
            type_: container_type::RESPONSE,
            code: codes::response::OK,
            transaction_id: 3,
            params: vec![],
            data: vec![],
        });

        let factory = MockTransportFactory {
            devices: vec![sample_device()],
            handle: handle.clone(),
        };
        {
            let mut app = FujiSessionHandle::new();
            app.connect_with_factory(&factory, "bus-1").unwrap();
            assert!(app.session.as_ref().unwrap().is_open());
        }

        let ops = handle.ops();
        assert!(
            ops.iter().any(|op| matches!(
                op,
                MockOp::Write {
                    code: c,
                    ..
                } if *c == codes::op::OPEN_SESSION
            )),
            "expected OpenSession write, got {ops:?}"
        );
        assert!(
            ops.iter().any(|op| matches!(
                op,
                MockOp::Write {
                    code: c,
                    ..
                } if *c == codes::op::CLOSE_SESSION
            )),
            "expected CloseSession on Drop, got {ops:?}"
        );
        assert!(handle.was_closed());
    }

    #[test]
    fn recover_reopens_session() {
        let handle = MockTransportHandle::default();
        queue_open_and_probe(&handle);
        // close during recover (into_transport)
        handle.push_response(PtpContainer {
            type_: container_type::RESPONSE,
            code: codes::response::OK,
            transaction_id: 3,
            params: vec![],
            data: vec![],
        });
        // reopen
        queue_open_and_probe(&handle);

        let factory = MockTransportFactory {
            devices: vec![sample_device()],
            handle: handle.clone(),
        };
        let mut app = FujiSessionHandle::new();
        app.connect_with_factory(&factory, "bus-1").unwrap();
        app.recover_with_factory(&factory).unwrap();
        assert!(app.session.as_ref().unwrap().is_open());
    }
}
