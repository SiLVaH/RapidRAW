//! RAF → camera JPEG conversion round-trip.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::fuji_raw_conv::error::{FujiRawConvError, FujiRawConvErrorKind, PtpErrorContext};
use crate::fuji_raw_conv::preset::{FujiRecipe, prop};
use crate::fuji_raw_conv::ptp::codes;
use crate::fuji_raw_conv::session::PtpSession;

const UPLOAD_TIMEOUT: Duration = Duration::from_secs(120);
const POLL_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Mockable conversion backend.
pub trait RawConverter: Send {
    fn write_recipe(&mut self, recipe: &FujiRecipe) -> Result<(), FujiRawConvError>;
    fn convert_raf(&mut self, raf: &[u8], recipe: &FujiRecipe) -> Result<Vec<u8>, FujiRawConvError>;
}

pub struct SessionRawConverter<'a> {
    pub session: &'a mut PtpSession,
}

impl<'a> RawConverter for SessionRawConverter<'a> {
    fn write_recipe(&mut self, recipe: &FujiRecipe) -> Result<(), FujiRawConvError> {
        write_recipe_to_session(self.session, recipe)
    }

    fn convert_raf(&mut self, raf: &[u8], recipe: &FujiRecipe) -> Result<Vec<u8>, FujiRawConvError> {
        convert_raf_with_session(self.session, raf, recipe)
    }
}

pub fn write_recipe_to_session(
    session: &mut PtpSession,
    recipe: &FujiRecipe,
) -> Result<(), FujiRawConvError> {
    let payloads = recipe.to_prop_payloads()?;
    for (prop_id, payload) in payloads {
        // Skip known camera-rejected / read-only edges softly? Prefer hard fail
        // except colour on mono (already omitted) and colour temp when not WB CT.
        session.set_device_prop_raw(prop_id, payload, PtpErrorContext::Generic)?;
    }
    Ok(())
}

pub fn convert_raf_with_session(
    session: &mut PtpSession,
    raf: &[u8],
    recipe: &FujiRecipe,
) -> Result<Vec<u8>, FujiRawConvError> {
    write_recipe_to_session(session, recipe)?;
    send_raf(session, raf)?;
    trigger_conversion(session)?;
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
        .map_err(|err| map_body_mismatch(err))?;

    session
        .transact_command_with_data_out(
            codes::fuji::OP_SEND_OBJECT,
            &[],
            raf.to_vec(),
            PtpErrorContext::RawConversion,
            UPLOAD_TIMEOUT,
        )
        .map_err(|err| map_body_mismatch(err))?;
    Ok(())
}

fn trigger_conversion(session: &mut PtpSession) -> Result<(), FujiRawConvError> {
    session.set_device_prop_raw(
        codes::fuji::PROP_START_RAW_CONVERSION,
        0u16.to_le_bytes().to_vec(),
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
                let (_resp, jpeg) = session.transact_command_with_data_in(
                    codes::op::GET_OBJECT,
                    &[handle],
                    PtpErrorContext::RawConversion,
                )?;
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

const DEFAULT_CMD_TIMEOUT: Duration = Duration::from_secs(10);

fn build_raf_object_info(size: u32) -> Vec<u8> {
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
    // PTP string: length byte (chars including NUL) + UTF-16LE
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

/// Read recipe properties currently on the camera (D18E–D1A5).
#[allow(dead_code)]
pub fn read_recipe_from_session(session: &mut PtpSession) -> Result<FujiRecipe, FujiRawConvError> {
    let mut payloads = BTreeMap::new();
    for id in prop::IMAGE_SIZE..=prop::UNKNOWN_D1A5 {
        match session.get_device_prop_raw(id, PtpErrorContext::Generic) {
            Ok(bytes) => {
                payloads.insert(id, bytes);
            }
            Err(_) => continue,
        }
    }
    Ok(FujiRecipe::from_prop_payloads(&payloads))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_info_contains_raf_format() {
        let info = build_raf_object_info(12345);
        assert_eq!(&info[4..6], &codes::fuji::OBJECT_FORMAT_RAF.to_le_bytes());
        assert_eq!(&info[8..12], &12345u32.to_le_bytes());
    }

    struct MockConverter {
        pub last_recipe: Option<FujiRecipe>,
        pub jpeg: Vec<u8>,
    }

    impl RawConverter for MockConverter {
        fn write_recipe(&mut self, recipe: &FujiRecipe) -> Result<(), FujiRawConvError> {
            self.last_recipe = Some(recipe.clone());
            Ok(())
        }

        fn convert_raf(
            &mut self,
            _raf: &[u8],
            recipe: &FujiRecipe,
        ) -> Result<Vec<u8>, FujiRawConvError> {
            self.write_recipe(recipe)?;
            Ok(self.jpeg.clone())
        }
    }

    #[test]
    fn mock_converter_round_trip() {
        let mut c = MockConverter {
            last_recipe: None,
            jpeg: b"FAKEJPEG".to_vec(),
        };
        let recipe = FujiRecipe::default();
        let out = c.convert_raf(b"RAF", &recipe).unwrap();
        assert_eq!(out, b"FAKEJPEG");
        assert!(c.last_recipe.is_some());
    }
}
