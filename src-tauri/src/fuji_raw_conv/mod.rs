//! Fujifilm camera-backed RAW conversion (USB RAW CONV. / PTP).
//!
//! Feature-gated behind `fuji-raw-conv`. Builds without the feature still
//! compile and expose stub commands that return a clear error.

mod capabilities;
mod error;
mod platform;
mod ptp;
mod session;
pub mod transport;

pub use capabilities::DiscoveredCamera;
#[allow(unused_imports)] // re-exported for callers / later phases
pub use error::FujiRawConvError;
pub use platform::PlatformUsbGuidance;
pub use session::FujiSessionHandle;

#[cfg(feature = "fuji-raw-conv")]
use crate::AppState;
#[cfg(feature = "fuji-raw-conv")]
use capabilities::describe_device;
use platform::current_platform_guidance;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FujiRawConvSupportInfo {
    pub supported: bool,
    pub feature_enabled: bool,
    pub platform: PlatformUsbGuidance,
}

#[tauri::command]
pub fn is_fuji_raw_conv_supported() -> FujiRawConvSupportInfo {
    FujiRawConvSupportInfo {
        supported: cfg!(feature = "fuji-raw-conv"),
        feature_enabled: cfg!(feature = "fuji-raw-conv"),
        platform: current_platform_guidance(),
    }
}

#[tauri::command]
pub fn fuji_raw_conv_platform_guidance() -> PlatformUsbGuidance {
    current_platform_guidance()
}

#[tauri::command]
pub async fn fuji_list_cameras() -> Result<Vec<DiscoveredCamera>, String> {
    #[cfg(feature = "fuji-raw-conv")]
    {
        tauri::async_runtime::spawn_blocking(|| {
            use crate::fuji_raw_conv::transport::nusb_transport::NusbTransportFactory;
            use crate::fuji_raw_conv::transport::TransportFactory;

            let factory = NusbTransportFactory;
            let devices = factory.list_fuji_devices().map_err(|e| e.to_command_error())?;
            Ok(devices
                .into_iter()
                .map(|d| {
                    describe_device(
                        d.bus_id,
                        d.vendor_id,
                        d.product_id,
                        d.manufacturer,
                        d.product,
                        d.serial_number,
                    )
                })
                .collect())
        })
        .await
        .map_err(|e| format!("Task panicked: {e}"))?
    }
    #[cfg(not(feature = "fuji-raw-conv"))]
    {
        Err(FujiRawConvError::not_in_build().to_command_error())
    }
}

#[tauri::command]
pub async fn fuji_connect(
    app_handle: tauri::AppHandle,
    bus_id: String,
) -> Result<DiscoveredCamera, String> {
    #[cfg(feature = "fuji-raw-conv")]
    {
        use tauri::Manager;
        tauri::async_runtime::spawn_blocking(move || {
            use crate::fuji_raw_conv::transport::nusb_transport::NusbTransportFactory;

            let state = app_handle.state::<AppState>();
            let factory = NusbTransportFactory;
            let mut handle = state.fuji_raw_conv_session.lock().unwrap();
            let info = handle
                .connect_with_factory(&factory, &bus_id)
                .map_err(|e| e.to_command_error())?;
            Ok(describe_device(
                info.bus_id.clone(),
                info.vendor_id,
                info.product_id,
                info.manufacturer.clone(),
                info.product.clone(),
                info.serial_number.clone(),
            ))
        })
        .await
        .map_err(|e| format!("Task panicked: {e}"))?
    }
    #[cfg(not(feature = "fuji-raw-conv"))]
    {
        let _ = (app_handle, bus_id);
        Err(FujiRawConvError::not_in_build().to_command_error())
    }
}

#[tauri::command]
pub async fn fuji_disconnect(app_handle: tauri::AppHandle) -> Result<(), String> {
    #[cfg(feature = "fuji-raw-conv")]
    {
        use tauri::Manager;
        tauri::async_runtime::spawn_blocking(move || {
            let state = app_handle.state::<AppState>();
            let mut handle = state.fuji_raw_conv_session.lock().unwrap();
            handle.disconnect();
            Ok(())
        })
        .await
        .map_err(|e| format!("Task panicked: {e}"))?
    }
    #[cfg(not(feature = "fuji-raw-conv"))]
    {
        let _ = app_handle;
        Err(FujiRawConvError::not_in_build().to_command_error())
    }
}

#[tauri::command]
pub async fn fuji_recover_session(app_handle: tauri::AppHandle) -> Result<(), String> {
    #[cfg(feature = "fuji-raw-conv")]
    {
        use tauri::Manager;
        tauri::async_runtime::spawn_blocking(move || {
            use crate::fuji_raw_conv::transport::nusb_transport::NusbTransportFactory;

            let state = app_handle.state::<AppState>();
            let factory = NusbTransportFactory;
            let mut handle = state.fuji_raw_conv_session.lock().unwrap();
            handle
                .recover_with_factory(&factory)
                .map_err(|e| e.to_command_error())
        })
        .await
        .map_err(|e| format!("Task panicked: {e}"))?
    }
    #[cfg(not(feature = "fuji-raw-conv"))]
    {
        let _ = app_handle;
        Err(FujiRawConvError::not_in_build().to_command_error())
    }
}
