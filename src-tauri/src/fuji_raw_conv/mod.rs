//! Fujifilm camera-backed RAW conversion (USB RAW CONV. / PTP).
//!
//! Feature-gated behind `fuji-raw-conv`. Builds without the feature still
//! compile and expose stub commands that return a clear error.

pub mod cache;
mod capabilities;
mod convert;
mod error;
mod platform;
pub mod preset;
mod ptp;
mod queue;
mod recipe_parse;
mod session;
pub mod transport;

#[cfg(all(test, feature = "fuji-raw-conv"))]
mod sample_raf_tests;

pub use cache::RenderStatus;
pub use capabilities::DiscoveredCamera;
#[allow(unused_imports)]
pub use error::FujiRawConvError;
pub use platform::PlatformUsbGuidance;
pub use preset::FujiRecipe;
pub use queue::{ConvertQueue, QueueJob};
pub use session::FujiSessionHandle;

use cache::{
    DEFAULT_MAX_BYTES, cache_root, cache_stats, has_entry, hash_raf_bytes, make_cache_key, purge_all,
    store_jpeg,
};
use capabilities::describe_device;
#[cfg(feature = "fuji-raw-conv")]
use convert::SessionRawConverter;
#[cfg(feature = "fuji-raw-conv")]
use convert::RawConverter;
use platform::current_platform_guidance;
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[cfg(feature = "fuji-raw-conv")]
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FujiRawConvSupportInfo {
    pub supported: bool,
    pub feature_enabled: bool,
    pub platform: PlatformUsbGuidance,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheStats {
    pub bytes: u64,
    pub entries: usize,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderJobResult {
    pub cache_key: String,
    pub status: RenderStatus,
    pub jpeg_path: Option<String>,
    pub error: Option<String>,
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
            use crate::fuji_raw_conv::transport::TransportFactory;
            use crate::fuji_raw_conv::transport::nusb_transport::NusbTransportFactory;

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

#[tauri::command]
pub async fn fuji_parse_recipe_from_raf(path: String) -> Result<FujiRecipe, String> {
    tauri::async_runtime::spawn_blocking(move || {
        recipe_parse::parse_recipe_from_raf(PathBuf::from(path).as_path())
    })
    .await
    .map_err(|e| format!("Task panicked: {e}"))?
}

#[tauri::command]
pub fn fuji_recipe_encode_roundtrip(recipe: FujiRecipe) -> Result<FujiRecipe, String> {
    let payloads = recipe
        .to_prop_payloads()
        .map_err(|e| e.to_command_error())?;
    Ok(FujiRecipe::from_prop_payloads(&payloads))
}

#[tauri::command]
pub async fn fuji_enqueue_convert(
    app_handle: tauri::AppHandle,
    source_path: String,
    recipe: FujiRecipe,
) -> Result<QueueJob, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app_handle.state::<AppState>();
        let raf = fs::read(&source_path).map_err(|e| e.to_string())?;
        let raf_hash = hash_raf_bytes(&raf);
        let cache_key = make_cache_key(&raf_hash, &recipe);
        let root = cache_root(&app_handle).map_err(|e| e.to_command_error())?;

        if has_entry(&root, &cache_key) {
            let job = QueueJob {
                id: Uuid::new_v4().to_string(),
                source_path,
                recipe,
                raf_hash,
                cache_key: cache_key.clone(),
                status: RenderStatus::Ready,
                error: None,
                created_unix_ms: now_ms(),
            };
            return Ok(job);
        }

        let job = QueueJob {
            id: Uuid::new_v4().to_string(),
            source_path,
            recipe,
            raf_hash,
            cache_key,
            status: RenderStatus::Queued,
            error: None,
            created_unix_ms: now_ms(),
        };
        state.fuji_convert_queue.enqueue(job.clone());
        Ok(job)
    })
    .await
    .map_err(|e| format!("Task panicked: {e}"))?
}

#[tauri::command]
pub fn fuji_list_queue(app_handle: tauri::AppHandle) -> Result<Vec<QueueJob>, String> {
    use tauri::Manager;
    let state = app_handle.state::<AppState>();
    Ok(state.fuji_convert_queue.list())
}

#[tauri::command]
pub async fn fuji_process_queue(app_handle: tauri::AppHandle) -> Result<Vec<RenderJobResult>, String> {
    #[cfg(feature = "fuji-raw-conv")]
    {
        use tauri::Manager;
        tauri::async_runtime::spawn_blocking(move || {
            let state = app_handle.state::<AppState>();
            let mut results = Vec::new();
            let root = cache_root(&app_handle).map_err(|e| e.to_command_error())?;

            loop {
                let Some(job) = state.fuji_convert_queue.pop_next() else {
                    break;
                };

                let mut session_guard = state.fuji_raw_conv_session.lock().unwrap();
                let Some(session) = session_guard.session.as_mut() else {
                    let mut failed = job.clone();
                    failed.status = RenderStatus::Queued;
                    failed.error = Some(
                        "No camera connected. Keep jobs queued until the camera is in USB RAW CONV. mode."
                            .into(),
                    );
                    // Put it back and stop — offline-first.
                    state.fuji_convert_queue.enqueue(failed.clone());
                    results.push(RenderJobResult {
                        cache_key: failed.cache_key,
                        status: RenderStatus::Queued,
                        jpeg_path: None,
                        error: failed.error,
                    });
                    break;
                };

                let raf = match fs::read(&job.source_path) {
                    Ok(b) => b,
                    Err(e) => {
                        results.push(RenderJobResult {
                            cache_key: job.cache_key.clone(),
                            status: RenderStatus::Failed,
                            jpeg_path: None,
                            error: Some(e.to_string()),
                        });
                        continue;
                    }
                };

                let mut converter = SessionRawConverter { session };
                match converter.convert_raf(&raf, &job.recipe) {
                    Ok(jpeg) => {
                        let meta = store_jpeg(
                            &root,
                            &job.raf_hash,
                            &job.recipe,
                            &jpeg,
                            DEFAULT_MAX_BYTES,
                        )
                        .map_err(|e| e.to_command_error())?;
                        let path = root.join(format!("{}.jpg", meta.cache_key));
                        results.push(RenderJobResult {
                            cache_key: meta.cache_key,
                            status: RenderStatus::Ready,
                            jpeg_path: Some(path.to_string_lossy().into_owned()),
                            error: None,
                        });
                    }
                    Err(err) => {
                        results.push(RenderJobResult {
                            cache_key: job.cache_key,
                            status: RenderStatus::Failed,
                            jpeg_path: None,
                            error: Some(err.to_command_error()),
                        });
                    }
                }
            }
            Ok(results)
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
pub async fn fuji_get_cached_render(
    app_handle: tauri::AppHandle,
    cache_key: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = cache_root(&app_handle).map_err(|e| e.to_command_error())?;
        let path = root.join(format!("{cache_key}.jpg"));
        if !path.is_file() {
            return Err("Cached camera render not found.".into());
        }
        Ok(path.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| format!("Task panicked: {e}"))?
}

#[tauri::command]
pub async fn fuji_cache_stats(app_handle: tauri::AppHandle) -> Result<CacheStats, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = cache_root(&app_handle).map_err(|e| e.to_command_error())?;
        let (bytes, entries) = cache_stats(&root).map_err(|e| e.to_command_error())?;
        Ok(CacheStats {
            bytes,
            entries,
            max_bytes: DEFAULT_MAX_BYTES,
        })
    })
    .await
    .map_err(|e| format!("Task panicked: {e}"))?
}

#[tauri::command]
pub async fn fuji_purge_cache(app_handle: tauri::AppHandle) -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = cache_root(&app_handle).map_err(|e| e.to_command_error())?;
        purge_all(&root).map_err(|e| e.to_command_error())
    })
    .await
    .map_err(|e| format!("Task panicked: {e}"))?
}

#[tauri::command]
pub async fn fuji_create_camera_render_version(
    app_handle: tauri::AppHandle,
    source_virtual_path: String,
    recipe: FujiRecipe,
    cache_key: String,
) -> Result<String, String> {
    // Create a virtual copy whose adjustments mark it as a camera render.
    let new_path = crate::file_management::create_virtual_copy(
        source_virtual_path.clone(),
        None,
        app_handle.clone(),
    )?;

    let (source_path, sidecar_path) =
        crate::file_management::parse_virtual_path(&new_path);
    let mut metadata = crate::exif_processing::load_sidecar(&sidecar_path);
    metadata.version = 2;

    let mut adjustments = if metadata.adjustments.is_null() {
        serde_json::json!({})
    } else {
        metadata.adjustments.clone()
    };
    if let Some(obj) = adjustments.as_object_mut() {
        obj.insert("fujiCameraRender".into(), serde_json::json!(true));
        obj.insert("fujiRecipe".into(), serde_json::to_value(&recipe).unwrap());
        obj.insert(
            "fujiRenderStatus".into(),
            serde_json::json!("ready"),
        );
        obj.insert("fujiCacheKey".into(), serde_json::json!(cache_key));
        // Scene-referred edits disabled on camera renders — zero them.
        obj.insert("exposure".into(), serde_json::json!(0.0));
        obj.insert("highlights".into(), serde_json::json!(0.0));
        obj.insert("shadows".into(), serde_json::json!(0.0));
        obj.insert("whites".into(), serde_json::json!(0.0));
        obj.insert("blacks".into(), serde_json::json!(0.0));
        obj.insert("temperature".into(), serde_json::json!(0.0));
        obj.insert("tint".into(), serde_json::json!(0.0));
        obj.insert("lutPath".into(), serde_json::Value::Null);
        obj.insert("lutIsSceneReferred".into(), serde_json::json!(false));
    }
    metadata.adjustments = adjustments;
    let json = serde_json::to_string_pretty(&metadata).map_err(|e| e.to_string())?;
    fs::write(&sidecar_path, json).map_err(|e| e.to_string())?;

    let _ = (app_handle, source_path);
    Ok(new_path)
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Shared queue handle type stored on AppState even without the USB feature.
pub type SharedConvertQueue = Arc<ConvertQueue>;

pub fn new_convert_queue() -> SharedConvertQueue {
    Arc::new(ConvertQueue::new())
}
