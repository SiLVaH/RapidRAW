use serde::Serialize;

#[cfg(feature = "fuji-raw-conv")]
use crate::fuji_raw_conv::error::{FujiRawConvError, FujiRawConvErrorKind};

/// OS-specific USB setup instructions shown when RAW CONV cannot open the device.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformUsbGuidance {
    pub os: String,
    pub title: String,
    pub steps: Vec<String>,
    pub requires_winusb: bool,
}

pub fn current_platform_guidance() -> PlatformUsbGuidance {
    #[cfg(target_os = "windows")]
    {
        PlatformUsbGuidance {
            os: "windows".into(),
            title: "Install WinUSB for the Fujifilm camera".into(),
            steps: vec![
                "Put the camera in USB RAW CONV. / BACKUP RESTORE mode.".into(),
                "Connect the camera with USB.".into(),
                "Open Zadig (https://zadig.akeo.ie/) and enable Options → List All Devices.".into(),
                "Select the Fujifilm camera interface (not the composite parent if listed separately).".into(),
                "Choose the WinUSB driver and click Replace Driver / Install Driver.".into(),
                "Unplug and replug the camera, then retry in RapidRAW.".into(),
            ],
            requires_winusb: true,
        }
    }
    #[cfg(target_os = "linux")]
    {
        PlatformUsbGuidance {
            os: "linux".into(),
            title: "Allow user access to the Fujifilm USB device".into(),
            steps: vec![
                "Put the camera in USB RAW CONV. / BACKUP RESTORE mode.".into(),
                "Connect the camera with USB.".into(),
                "Install a udev rule granting your user access to vendor 04cb (Fujifilm), then reload udev.".into(),
                "Example rule: SUBSYSTEM==\"usb\", ATTR{idVendor}==\"04cb\", MODE=\"0660\", GROUP=\"plugdev\"".into(),
                "Add your user to the plugdev group if needed, then unplug/replug the camera.".into(),
            ],
            requires_winusb: false,
        }
    }
    #[cfg(target_os = "macos")]
    {
        PlatformUsbGuidance {
            os: "macos".into(),
            title: "Grant USB access and use RAW CONV. mode".into(),
            steps: vec![
                "Put the camera in USB RAW CONV. / BACKUP RESTORE mode.".into(),
                "Connect the camera with USB.".into(),
                "Quit Fujifilm X RAW Studio and any other app that may claim the camera.".into(),
                "If macOS prompts for USB accessory permission, allow RapidRAW.".into(),
            ],
            requires_winusb: false,
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        PlatformUsbGuidance {
            os: std::env::consts::OS.into(),
            title: "USB RAW conversion is not supported on this platform".into(),
            steps: vec![
                "Fujifilm camera-backed RAW conversion is supported on Windows, macOS, and Linux."
                    .into(),
            ],
            requires_winusb: false,
        }
    }
}

#[cfg(feature = "fuji-raw-conv")]
pub fn permission_recovery_hint() -> String {
    current_platform_guidance().steps.join(" ")
}

#[cfg(feature = "fuji-raw-conv")]
pub fn winusb_required_error() -> FujiRawConvError {
    let guidance = current_platform_guidance();
    FujiRawConvError::platform_driver(
        "Windows cannot talk to this camera until the WinUSB driver is installed for its USB interface.",
        guidance.steps.join(" "),
    )
}

#[cfg(feature = "fuji-raw-conv")]
pub fn looks_like_missing_winusb(message: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        let lower = message.to_lowercase();
        lower.contains("winusb")
            || lower.contains("the device has no winusb")
            || lower.contains("not supported")
            || lower.contains("driver")
            || lower.contains("access is denied")
            || lower.contains("cannot find")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = message;
        false
    }
}

/// Reserved for discovery UX when the OS reports a permission/driver failure
/// instead of an empty device list.
#[cfg(feature = "fuji-raw-conv")]
#[allow(dead_code)]
pub fn map_empty_discovery_hint(had_permission_error: bool) -> Option<FujiRawConvError> {
    if had_permission_error {
        #[cfg(target_os = "windows")]
        {
            return Some(winusb_required_error());
        }
        #[cfg(not(target_os = "windows"))]
        {
            return Some(
                FujiRawConvError::new(
                    FujiRawConvErrorKind::PlatformDriver,
                    "No accessible Fujifilm camera was found.",
                )
                .with_recovery(permission_recovery_hint()),
            );
        }
    }
    None
}
