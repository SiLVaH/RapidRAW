use serde::Serialize;
use std::fmt;

/// User-facing and programmatic errors for Fujifilm RAW conversion.
///
/// Wire/protocol codes are never shown raw to the UI when a known cause exists.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FujiRawConvError {
    pub kind: FujiRawConvErrorKind,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recovery_hint: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FujiRawConvErrorKind {
    NotInBuild,
    PlatformDriver,
    NoCamera,
    WrongUsbMode,
    BodyMismatch,
    SessionClosed,
    SessionStale,
    Transport,
    Protocol,
    UnsupportedBody,
    Cancelled,
    Other,
}

impl FujiRawConvError {
    pub fn new(kind: FujiRawConvErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            detail: None,
            recovery_hint: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_recovery(mut self, hint: impl Into<String>) -> Self {
        self.recovery_hint = Some(hint.into());
        self
    }

    pub fn not_in_build() -> Self {
        Self::new(
            FujiRawConvErrorKind::NotInBuild,
            "Fujifilm camera RAW conversion is not included in this build. Rebuild with --features fuji-raw-conv.",
        )
    }

    pub fn wrong_usb_mode() -> Self {
        Self::new(
            FujiRawConvErrorKind::WrongUsbMode,
            "Camera is not in USB RAW CONV. / BACKUP RESTORE mode.",
        )
        .with_recovery(
            "On the camera: MENU → SET UP → CONNECTION SETTING → USB MODE → USB RAW CONV./BACKUP RESTORE, then reconnect the USB cable.",
        )
    }

    pub fn body_mismatch() -> Self {
        Self::new(
            FujiRawConvErrorKind::BodyMismatch,
            "This RAF was not shot on the connected camera body. Fujifilm only converts files from the same body that captured them.",
        )
        .with_detail("PTP response 0x2002 (General Error) during RAW conversion.")
        .with_recovery(
            "Connect the camera that originally shot this RAF, or convert that body's files only.",
        )
    }

    pub fn from_ptp_response(code: u16, context: PtpErrorContext) -> Self {
        match (code, context) {
            (0x2002, PtpErrorContext::RawConversion) => Self::body_mismatch(),
            (0x2002, _) => Self::new(
                FujiRawConvErrorKind::Protocol,
                "The camera rejected the request.",
            )
            .with_detail(format!("PTP response 0x{code:04X}")),
            (0x2003, _) => Self::new(
                FujiRawConvErrorKind::SessionClosed,
                "No open PTP session with the camera.",
            )
            .with_recovery("Reconnect the camera, or use Recover Session."),
            (0x2005, PtpErrorContext::CapabilityProbe) => Self::wrong_usb_mode(),
            (0x201E, _) => Self::new(
                FujiRawConvErrorKind::SessionStale,
                "A PTP session was already open on the camera.",
            )
            .with_recovery("Use Recover Session to close the stale session and reconnect."),
            _ => Self::new(
                FujiRawConvErrorKind::Protocol,
                "Unexpected response from the camera.",
            )
            .with_detail(format!("PTP response 0x{code:04X}")),
        }
    }

    pub fn platform_driver(message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self::new(FujiRawConvErrorKind::PlatformDriver, message)
            .with_recovery(hint)
    }

    pub fn to_command_error(&self) -> String {
        let mut out = self.message.clone();
        if let Some(hint) = &self.recovery_hint {
            out.push(' ');
            out.push_str(hint);
        }
        out
    }
}

impl fmt::Display for FujiRawConvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(hint) = &self.recovery_hint {
            write!(f, " {hint}")?;
        }
        Ok(())
    }
}

impl std::error::Error for FujiRawConvError {}

/// Narrows how a PTP response code should be interpreted for the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PtpErrorContext {
    Generic,
    CapabilityProbe,
    RawConversion,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_0x2002_during_conversion_to_body_mismatch() {
        let err = FujiRawConvError::from_ptp_response(0x2002, PtpErrorContext::RawConversion);
        assert_eq!(err.kind, FujiRawConvErrorKind::BodyMismatch);
        assert!(!err.message.contains("0x2002"));
        assert!(err.message.to_lowercase().contains("same body"));
    }

    #[test]
    fn wrong_usb_mode_has_recovery_steps() {
        let err = FujiRawConvError::wrong_usb_mode();
        assert_eq!(err.kind, FujiRawConvErrorKind::WrongUsbMode);
        assert!(err.recovery_hint.as_ref().unwrap().contains("USB RAW CONV"));
    }
}
