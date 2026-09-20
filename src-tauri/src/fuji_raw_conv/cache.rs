//! On-disk cache for camera-rendered JPEGs.
//! Key = blake3(RAF bytes) + recipe content hash. Size-capped with manual purge.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::fuji_raw_conv::error::{FujiRawConvError, FujiRawConvErrorKind};
use crate::fuji_raw_conv::preset::FujiRecipe;

/// Default soft limit for the camera-render cache (512 MiB).
pub const DEFAULT_MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderStatus {
    None,
    Queued,
    Stale,
    Ready,
    Failed,
}

impl Default for RenderStatus {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEntryMeta {
    pub cache_key: String,
    pub raf_hash: String,
    pub recipe_hash: String,
    pub bytes: u64,
    pub created_unix_ms: u64,
}

pub fn cache_root(app_handle: &AppHandle) -> Result<PathBuf, FujiRawConvError> {
    let root = app_handle
        .path()
        .app_cache_dir()
        .map_err(|e| {
            FujiRawConvError::new(
                FujiRawConvErrorKind::Other,
                "Could not resolve the app cache directory.",
            )
            .with_detail(e.to_string())
        })?
        .join("fuji_raw_conv");
    if !root.exists() {
        fs::create_dir_all(&root).map_err(|e| {
            FujiRawConvError::new(
                FujiRawConvErrorKind::Other,
                "Could not create the Fujifilm render cache directory.",
            )
            .with_detail(e.to_string())
        })?;
    }
    Ok(root)
}

pub fn hash_raf_bytes(raf: &[u8]) -> String {
    blake3::hash(raf).to_hex().to_string()
}

pub fn make_cache_key(raf_hash: &str, recipe: &FujiRecipe) -> String {
    format!("{}_{}", &raf_hash[..16.min(raf_hash.len())], &recipe.content_hash()[..16])
}

pub fn jpeg_path(root: &Path, cache_key: &str) -> PathBuf {
    root.join(format!("{cache_key}.jpg"))
}

pub fn meta_path(root: &Path, cache_key: &str) -> PathBuf {
    root.join(format!("{cache_key}.json"))
}

pub fn store_jpeg(
    root: &Path,
    raf_hash: &str,
    recipe: &FujiRecipe,
    jpeg: &[u8],
    max_bytes: u64,
) -> Result<CacheEntryMeta, FujiRawConvError> {
    let cache_key = make_cache_key(raf_hash, recipe);
    let jpeg_file = jpeg_path(root, &cache_key);
    fs::write(&jpeg_file, jpeg).map_err(|e| {
        FujiRawConvError::new(
            FujiRawConvErrorKind::Other,
            "Failed to write camera render to cache.",
        )
        .with_detail(e.to_string())
    })?;

    let meta = CacheEntryMeta {
        cache_key: cache_key.clone(),
        raf_hash: raf_hash.to_string(),
        recipe_hash: recipe.content_hash(),
        bytes: jpeg.len() as u64,
        created_unix_ms: now_ms(),
    };
    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| {
        FujiRawConvError::new(FujiRawConvErrorKind::Other, "Failed to serialize cache meta.")
            .with_detail(e.to_string())
    })?;
    fs::write(meta_path(root, &cache_key), meta_json).map_err(|e| {
        FujiRawConvError::new(
            FujiRawConvErrorKind::Other,
            "Failed to write camera render cache metadata.",
        )
        .with_detail(e.to_string())
    })?;

    enforce_size_limit(root, max_bytes)?;
    Ok(meta)
}

pub fn load_jpeg(root: &Path, cache_key: &str) -> Result<Vec<u8>, FujiRawConvError> {
    let path = jpeg_path(root, cache_key);
    fs::read(&path).map_err(|e| {
        FujiRawConvError::new(
            FujiRawConvErrorKind::Other,
            "Cached camera render not found.",
        )
        .with_detail(e.to_string())
    })
}

pub fn has_entry(root: &Path, cache_key: &str) -> bool {
    jpeg_path(root, cache_key).is_file()
}

pub fn purge_all(root: &Path) -> Result<u64, FujiRawConvError> {
    let mut removed = 0u64;
    if !root.exists() {
        return Ok(0);
    }
    for entry in fs::read_dir(root).map_err(|e| {
        FujiRawConvError::new(FujiRawConvErrorKind::Other, "Failed to list render cache.")
            .with_detail(e.to_string())
    })? {
        let entry = entry.map_err(|e| {
            FujiRawConvError::new(FujiRawConvErrorKind::Other, "Failed to read cache entry.")
                .with_detail(e.to_string())
        })?;
        let meta = entry.metadata().ok();
        let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        fs::remove_file(entry.path()).ok();
        removed = removed.saturating_add(size);
    }
    Ok(removed)
}

pub fn cache_stats(root: &Path) -> Result<(u64, usize), FujiRawConvError> {
    let mut bytes = 0u64;
    let mut count = 0usize;
    if !root.exists() {
        return Ok((0, 0));
    }
    for entry in fs::read_dir(root).map_err(|e| {
        FujiRawConvError::new(FujiRawConvErrorKind::Other, "Failed to list render cache.")
            .with_detail(e.to_string())
    })? {
        let entry = entry.map_err(|e| {
            FujiRawConvError::new(FujiRawConvErrorKind::Other, "Failed to read cache entry.")
                .with_detail(e.to_string())
        })?;
        if entry
            .path()
            .extension()
            .and_then(|e| e.to_str())
            == Some("jpg")
        {
            count += 1;
            bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    Ok((bytes, count))
}

fn enforce_size_limit(root: &Path, max_bytes: u64) -> Result<(), FujiRawConvError> {
    let mut entries: Vec<(u64, PathBuf, PathBuf, u64)> = Vec::new();
    let mut total = 0u64;
    for entry in fs::read_dir(root).map_err(|e| {
        FujiRawConvError::new(FujiRawConvErrorKind::Other, "Failed to list render cache.")
            .with_detail(e.to_string())
    })? {
        let entry = entry.map_err(|e| {
            FujiRawConvError::new(FujiRawConvErrorKind::Other, "Failed to read cache entry.")
                .with_detail(e.to_string())
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jpg") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let meta_file = meta_path(root, &stem);
        let created = fs::read_to_string(&meta_file)
            .ok()
            .and_then(|s| serde_json::from_str::<CacheEntryMeta>(&s).ok())
            .map(|m| m.created_unix_ms)
            .unwrap_or(0);
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        total += size;
        entries.push((created, path, meta_file, size));
    }

    if total <= max_bytes {
        return Ok(());
    }

    entries.sort_by_key(|(created, _, _, _)| *created);
    for (_, jpg, meta, size) in entries {
        if total <= max_bytes {
            break;
        }
        let _ = fs::remove_file(jpg);
        let _ = fs::remove_file(meta);
        total = total.saturating_sub(size);
    }
    Ok(())
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn store_and_load_round_trip() {
        let dir = tempdir().unwrap();
        let recipe = FujiRecipe::default();
        let raf_hash = hash_raf_bytes(b"fake-raf");
        let meta = store_jpeg(dir.path(), &raf_hash, &recipe, b"jpeg-bytes", 1024).unwrap();
        let loaded = load_jpeg(dir.path(), &meta.cache_key).unwrap();
        assert_eq!(loaded, b"jpeg-bytes");
        assert!(has_entry(dir.path(), &meta.cache_key));
    }

    #[test]
    fn purge_clears_cache() {
        let dir = tempdir().unwrap();
        let recipe = FujiRecipe::default();
        let raf_hash = hash_raf_bytes(b"raf");
        store_jpeg(dir.path(), &raf_hash, &recipe, b"abc", 1024).unwrap();
        let removed = purge_all(dir.path()).unwrap();
        assert!(removed >= 3);
        assert_eq!(cache_stats(dir.path()).unwrap(), (0, 0));
    }
}
