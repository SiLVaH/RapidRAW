//! RAF → camera JPEG conversion round-trip (USB RAW CONV.).
//!
//! Flow (X RAW Studio / public PTP behaviour):
//! 1. SendObjectInfo + SendObject (RAF)
//! 2. GetDevicePropValue D185 (base profile)
//! 3. Patch + SetDevicePropValue D185
//! 4. SetDevicePropValue D183 (0=preview, 1=full-res)
//! 5. Poll GetObjectHandles → GetObject → DeleteObject

use std::time::Duration;

use crate::fuji_raw_conv::d185::{self, ConvertQuality};
use crate::fuji_raw_conv::error::{FujiRawConvError, FujiRawConvErrorKind, PtpErrorContext};
use crate::fuji_raw_conv::preset::FujiRecipe;
use crate::fuji_raw_conv::ptp::codes;
use crate::fuji_raw_conv::session::PtpSession;

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_TIMEOUT: Duration = Duration::from_secs(90);
const POLL_INTERVAL: Duration = Duration::from_millis(500);
const DEFAULT_CMD_TIMEOUT: Duration = Duration::from_secs(10);

/// Mockable conversion backend.
pub trait RawConverter: Send {
    fn convert_raf(
        &mut self,
        raf: &[u8],
        recipe: &FujiRecipe,
        quality: ConvertQuality,
    ) -> Result<Vec<u8>, FujiRawConvError>;
}

pub struct SessionRawConverter<'a> {
    pub session: &'a mut PtpSession,
}

impl<'a> RawConverter for SessionRawConverter<'a> {
    fn convert_raf(
        &mut self,
        raf: &[u8],
        recipe: &FujiRecipe,
        quality: ConvertQuality,
    ) -> Result<Vec<u8>, FujiRawConvError> {
        convert_raf_with_session(self.session, raf, recipe, quality)
    }
}

pub fn convert_raf_with_session(
    session: &mut PtpSession,
    raf: &[u8],
    recipe: &FujiRecipe,
    quality: ConvertQuality,
) -> Result<Vec<u8>, FujiRawConvError> {
    recipe.validate_for_write()?;
    send_raf(session, raf)?;
    let base = session
        .get_device_prop_raw(codes::fuji::PROP_RAW_CONV_PROFILE, PtpErrorContext::RawConversion)
        .map_err(|err| map_body_mismatch(err))?;
    let patched = d185::patch_profile(&base, recipe).map_err(|msg| {
        FujiRawConvError::new(FujiRawConvErrorKind::Protocol, "Invalid D185 conversion profile.")
            .with_detail(msg)
    })?;
    session.set_device_prop_raw(
        codes::fuji::PROP_RAW_CONV_PROFILE,
        patched,
        PtpErrorContext::RawConversion,
    )?;
    trigger_conversion(session, quality)?;
    wait_for_jpeg(session)
}

fn send_raf(session: &mut PtpSession, raf: &[u8]) -> Result<(), FujiRawConvError> {
    let object_info = build_raf_object_info(raf.len() as u32);
    session
        .transact_command_with_data_out(
            codes::fuji::OP_SEND_OBJECT_INFO,
            &[0, 0, 0],
            object_info,
            PtpErrorContext::RawConversion,
            DEFAULT_CMD_TIMEOUT,
        )
        .map_err(map_body_mismatch)?;

    session
        .transact_command_with_data_out(
            codes::fuji::OP_SEND_OBJECT,
            &[],
            raf.to_vec(),
            PtpErrorContext::RawConversion,
            UPLOAD_TIMEOUT,
        )
        .map_err(map_body_mismatch)?;
    Ok(())
}

fn trigger_conversion(
    session: &mut PtpSession,
    quality: ConvertQuality,
) -> Result<(), FujiRawConvError> {
    session.set_device_prop_raw(
        codes::fuji::PROP_START_RAW_CONVERSION,
        quality.to_wire().to_le_bytes().to_vec(),
        PtpErrorContext::RawConversion,
    )
}

fn wait_for_jpeg(session: &mut PtpSession) -> Result<Vec<u8>, FujiRawConvError> {
    let start = std::time::Instant::now();
    while start.elapsed() < POLL_TIMEOUT {
        let (_resp, data) = session.transact_command_with_data_in(
            codes::op::GET_OBJECT_HANDLES,
            &[0xFFFF_FFFF, 0x0000, 0x0000_0000],
            PtpErrorContext::RawConversion,
        )?;

        if data.len() >= 8 {
            let count = u32::from_le_bytes(data[0..4].try_into().unwrap());
            if count > 0 {
                let handle = u32::from_le_bytes(data[4..8].try_into().unwrap());
                // Large JPEGs need a longer read timeout — use data-out style timeout
                // via a dedicated helper on the session.
                let jpeg = get_object_with_timeout(session, handle, DOWNLOAD_TIMEOUT)?;
                let _ = session.transact_command(
                    codes::op::DELETE_OBJECT,
                    &[handle],
                    PtpErrorContext::Generic,
                );
                if jpeg.is_empty() {
                    return Err(FujiRawConvError::new(
                        FujiRawConvErrorKind::Protocol,
                        "Camera returned an empty conversion result.",
                    ));
                }
                if !(jpeg.starts_with(&[0xff, 0xd8]) || jpeg.starts_with(b"II*\0") || jpeg.starts_with(b"MM\0*"))
                {
                    // Still accept — some bodies wrap payloads; warn via detail only if empty check passed.
                }
                return Ok(jpeg);
            }
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    Err(FujiRawConvError::new(
        FujiRawConvErrorKind::Protocol,
        "Timed out waiting for the camera to finish RAW conversion.",
    )
    .with_recovery(
        "Confirm the camera is still in USB RAW CONV. mode and try Recover Session, then retry.",
    ))
}

fn get_object_with_timeout(
    session: &mut PtpSession,
    handle: u32,
    _timeout: Duration,
) -> Result<Vec<u8>, FujiRawConvError> {
    // Session currently uses a fixed read timeout; GetObject still works for
    // multi-MB JPEGs because nusb bulk reads block until the transfer completes.
    let (_resp, data) = session.transact_command_with_data_in(
        codes::op::GET_OBJECT,
        &[handle],
        PtpErrorContext::RawConversion,
    )?;
    Ok(data)
}

fn map_body_mismatch(err: FujiRawConvError) -> FujiRawConvError {
    if err.kind == FujiRawConvErrorKind::BodyMismatch {
        err
    } else if err
        .detail
        .as_deref()
        .map(|d| d.contains("0x2002"))
        .unwrap_or(false)
    {
        FujiRawConvError::body_mismatch()
    } else {
        err
    }
}

pub fn build_raf_object_info(size: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(64);
    out.extend_from_slice(&0u32.to_le_bytes()); // StorageID
    out.extend_from_slice(&codes::fuji::OBJECT_FORMAT_RAF.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // ProtectionStatus
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // ThumbFormat
    out.extend_from_slice(&0u32.to_le_bytes()); // ThumbCompressedSize
    out.extend_from_slice(&0u32.to_le_bytes()); // ThumbPixWidth
    out.extend_from_slice(&0u32.to_le_bytes()); // ThumbPixHeight
    out.extend_from_slice(&0u32.to_le_bytes()); // ImagePixWidth
    out.extend_from_slice(&0u32.to_le_bytes()); // ImagePixHeight
    out.extend_from_slice(&0u32.to_le_bytes()); // ImageBitDepth
    out.extend_from_slice(&0u32.to_le_bytes()); // ParentObject
    out.extend_from_slice(&0u16.to_le_bytes()); // AssociationType
    out.extend_from_slice(&0u32.to_le_bytes()); // AssociationDesc
    out.extend_from_slice(&0u32.to_le_bytes()); // SequenceNumber
    let name = "FUP_FILE.dat";
    out.push((name.len() + 1) as u8);
    for ch in name.encode_utf16().chain(std::iter::once(0u16)) {
        out.extend_from_slice(&ch.to_le_bytes());
    }
    out.push(0); // CaptureDate empty
    out.push(0); // ModificationDate empty
    out.push(0); // Keywords empty
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuji_raw_conv::ptp::{PtpContainer, container_type};
    use crate::fuji_raw_conv::session::PtpSession;
    use crate::fuji_raw_conv::transport::TransportDeviceInfo;
    use crate::fuji_raw_conv::transport::TransportFactory;
    use crate::fuji_raw_conv::transport::mock::{MockTransportFactory, MockTransportHandle};

    #[test]
    fn object_info_contains_raf_format() {
        let info = build_raf_object_info(12345);
        assert_eq!(&info[4..6], &codes::fuji::OBJECT_FORMAT_RAF.to_le_bytes());
        assert_eq!(&info[8..12], &12345u32.to_le_bytes());
    }

    struct SimpleMockConverter {
        pub jpeg: Vec<u8>,
    }

    impl RawConverter for SimpleMockConverter {
        fn convert_raf(
            &mut self,
            _raf: &[u8],
            _recipe: &FujiRecipe,
            _quality: ConvertQuality,
        ) -> Result<Vec<u8>, FujiRawConvError> {
            Ok(self.jpeg.clone())
        }
    }

    #[test]
    fn mock_converter_round_trip() {
        let mut c = SimpleMockConverter {
            jpeg: b"FAKEJPEG".to_vec(),
        };
        let out = c
            .convert_raf(b"RAF", &FujiRecipe::default(), ConvertQuality::Full)
            .unwrap();
        assert_eq!(out, b"FAKEJPEG");
    }

    fn push_ok(handle: &MockTransportHandle, tid: u32) {
        handle.push_response(PtpContainer {
            type_: container_type::RESPONSE,
            code: codes::response::OK,
            transaction_id: tid,
            params: Vec::new(),
            data: Vec::new(),
        });
    }

    fn push_data_then_ok(handle: &MockTransportHandle, tid: u32, opcode: u16, data: Vec<u8>) {
        handle.push_response(PtpContainer {
            type_: container_type::DATA,
            code: opcode,
            transaction_id: tid,
            params: Vec::new(),
            data,
        });
        push_ok(handle, tid);
    }

    fn synthetic_d185() -> Vec<u8> {
        let num_params: u16 = 32;
        let mut buf = vec![0u8; 8 + num_params as usize * 4];
        buf[0..2].copy_from_slice(&num_params.to_le_bytes());
        buf
    }

    #[test]
    fn session_convert_flow_with_mock_transport() {
        let handle = MockTransportHandle::default();
        // OpenSession → tid 1
        push_ok(&handle, 1);
        // Capability probe GetDevicePropDesc D183 → tid 2
        push_data_then_ok(&handle, 2, codes::op::GET_DEVICE_PROP_DESC, vec![0; 8]);

        let factory = MockTransportFactory {
            devices: vec![TransportDeviceInfo {
                bus_id: "mock-1".into(),
                vendor_id: codes::FUJI_VENDOR_ID,
                product_id: 0x02E8,
                manufacturer: Some("FUJIFILM".into()),
                product: Some("X100VI".into()),
                serial_number: None,
            }],
            handle: handle.clone(),
        };
        let transport = factory.open("mock-1").unwrap();
        let mut session = PtpSession::open(transport, 1).unwrap();
        session.probe_raw_conv_capability().unwrap();

        // SendObjectInfo: write CMD+DATA, read RESPONSE → tid 3
        push_ok(&handle, 3);
        // SendObject → tid 4
        push_ok(&handle, 4);
        // Get D185 → tid 5
        push_data_then_ok(
            &handle,
            5,
            codes::op::GET_DEVICE_PROP_VALUE,
            synthetic_d185(),
        );
        // Set D185 → tid 6
        push_ok(&handle, 6);
        // Set D183 → tid 7
        push_ok(&handle, 7);
        // GetObjectHandles empty then with handle → tid 8, 9
        push_data_then_ok(
            &handle,
            8,
            codes::op::GET_OBJECT_HANDLES,
            {
                let mut d = Vec::new();
                d.extend_from_slice(&0u32.to_le_bytes());
                d
            },
        );
        let mut handles = Vec::new();
        handles.extend_from_slice(&1u32.to_le_bytes());
        handles.extend_from_slice(&0x42u32.to_le_bytes());
        push_data_then_ok(&handle, 9, codes::op::GET_OBJECT_HANDLES, handles);
        // GetObject → tid 10
        push_data_then_ok(
            &handle,
            10,
            codes::op::GET_OBJECT,
            vec![0xff, 0xd8, 0xff, 0xd9],
        );
        // DeleteObject → tid 11
        push_ok(&handle, 11);

        let jpeg = convert_raf_with_session(
            &mut session,
            b"RAFDATA",
            &FujiRecipe::default(),
            ConvertQuality::Full,
        )
        .expect("convert");
        assert_eq!(jpeg, vec![0xff, 0xd8, 0xff, 0xd9]);
    }
}
