//! Best-effort Fujifilm recipe extraction from RAF maker notes / EXIF.
//!
//! RAF containers are not plain TIFF; we pull the embedded JPEG preview
//! (header offsets at 0x54/0x58, big-endian) and parse its EXIF + Fujifilm
//! MakerNote IFD. FilmMode values are EXIF codes and must be mapped to the
//! PTP film-simulation IDs used by RAW CONV.

use std::collections::HashMap;
use std::fs;
use std::io::Cursor;
use std::path::Path;

use crate::fuji_raw_conv::preset::{
    EffectStrength, FujiRecipe, GrainPreset, film_sim, wb_mode,
};

/// Parse a starting recipe from a RAF on disk.
pub fn parse_recipe_from_raf(path: &Path) -> Result<FujiRecipe, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(parse_recipe_from_raf_bytes(&bytes))
}

pub fn parse_recipe_from_raf_bytes(bytes: &[u8]) -> FujiRecipe {
    let mut recipe = FujiRecipe::default();

    let jpeg = extract_raf_preview_jpeg(bytes).unwrap_or_else(|| bytes.to_vec());

    if let Ok(exif) = read_exif_map(&jpeg) {
        apply_exif_hints(&mut recipe, &exif);
    }

    if let Some(mn) = extract_fuji_makernote(&jpeg) {
        apply_fuji_makernote(&mut recipe, &mn);
    }

    recipe
}

/// Fujifilm RAF v2: JPEG preview offset/length at big-endian u32 @ 0x54 / 0x58.
fn extract_raf_preview_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    if bytes.len() < 0x5c || !bytes.starts_with(b"FUJIFILMCCD-RAW ") {
        // Already a JPEG, or unknown container — try as-is.
        if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            return Some(bytes.to_vec());
        }
        return None;
    }
    let offset = u32::from_be_bytes(bytes[0x54..0x58].try_into().ok()?) as usize;
    let length = u32::from_be_bytes(bytes[0x58..0x5c].try_into().ok()?) as usize;
    if offset == 0 || length == 0 || offset.saturating_add(length) > bytes.len() {
        return None;
    }
    let slice = &bytes[offset..offset + length];
    if slice.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(slice.to_vec())
    } else {
        None
    }
}

fn extract_fuji_makernote(jpeg: &[u8]) -> Option<Vec<u8>> {
    // MakerNote payload begins with "FUJIFILM" + LE u32 IFD offset (usually 12).
    let sig = b"FUJIFILM\x0c\x00\x00\x00";
    let idx = jpeg.windows(sig.len()).position(|w| w == sig)?;
    // Include enough trailing bytes for IFD + inline values (preview MakerNotes are small).
    let end = (idx + 64 * 1024).min(jpeg.len());
    Some(jpeg[idx..end].to_vec())
}

fn apply_fuji_makernote(recipe: &mut FujiRecipe, mn: &[u8]) {
    let tags = parse_fuji_makernote_shorts(mn);
    if let Some(film) = tags.get(&0x1401).copied() {
        if let Some(ptp) = exif_film_mode_to_ptp(film) {
            recipe.film_simulation = ptp;
        }
    }
    // Prefer DevelopmentDynamicRange (100/200/400) when present.
    if let Some(dr) = tags.get(&0x1403).copied() {
        if matches!(dr, 100 | 200 | 400) {
            recipe.dynamic_range = dr;
        }
    } else if let Some(dr) = tags.get(&0x1400).copied() {
        recipe.dynamic_range = match dr {
            3 => 400,
            1 => 100,
            other if matches!(other, 100 | 200 | 400) => other,
            _ => recipe.dynamic_range,
        };
    }

    // Tone / effect tags are signed-ish in ExifTool; 0 is neutral/off on these samples.
    if let Some(v) = tags.get(&0x1041).copied() {
        recipe.highlight_tone = fuji_tone_tag_to_stops(v);
    }
    if let Some(v) = tags.get(&0x1040).copied() {
        recipe.shadow_tone = fuji_tone_tag_to_stops(v);
    }
    if let Some(v) = tags.get(&0x100f).copied() {
        // Clarity stored as signed; treat 0 as neutral.
        recipe.clarity = fuji_tone_tag_to_stops(v);
    }
    if let Some(v) = tags.get(&0x1047).copied() {
        recipe.grain = grain_from_exif(v, tags.get(&0x104c).copied().unwrap_or(0));
    }
    if let Some(v) = tags.get(&0x1048).copied() {
        recipe.color_chrome = effect_from_exif(v);
    }
    if let Some(v) = tags.get(&0x104e).copied() {
        recipe.color_chrome_fx_blue = effect_from_exif(v);
    }
}

fn fuji_tone_tag_to_stops(raw: u16) -> f32 {
    // ExifTool uses 0 = 0, ± values / special encodings; keep 0→0 and clamp others lightly.
    if raw == 0 {
        0.0
    } else if raw <= 8 {
        raw as f32
    } else if raw >= 0xfff0 {
        -((0x10000 - raw as u32) as f32)
    } else {
        0.0
    }
}

fn grain_from_exif(roughness: u16, size: u16) -> GrainPreset {
    // ExifTool: 0=Off; non-zero roughness/size encode weak/strong × small/large.
    match (roughness, size) {
        (0, _) => GrainPreset::Off,
        (_, 0x20) => GrainPreset::WeakLarge,
        (_, 0x40) => GrainPreset::StrongLarge,
        (0x20, _) => GrainPreset::WeakSmall,
        (0x40, _) => GrainPreset::StrongSmall,
        _ if roughness != 0 => GrainPreset::WeakSmall,
        _ => GrainPreset::Off,
    }
}

fn effect_from_exif(v: u16) -> EffectStrength {
    match v {
        0 => EffectStrength::Off,
        0x20 | 1 => EffectStrength::Weak,
        0x40 | 2 => EffectStrength::Strong,
        _ => EffectStrength::Off,
    }
}

/// Map ExifTool Fujifilm FilmMode (0x1401) → PTP RAW CONV film simulation.
fn exif_film_mode_to_ptp(exif: u16) -> Option<u16> {
    Some(match exif {
        0x000 => film_sim::PROVIA,
        0x100 | 0x110 | 0x130 => film_sim::PROVIA, // older studio portrait → nearest
        0x120 => film_sim::ASTIA,
        0x200 | 0x400 => film_sim::VELVIA,
        0x300 => film_sim::ASTIA,
        0x500 => film_sim::PRO_NEG_STD,
        0x501 => film_sim::PRO_NEG_HI,
        0x600 => film_sim::CLASSIC_CHROME,
        0x700 => film_sim::ETERNA,
        0x800 => film_sim::CLASSIC_NEG,
        0x900 => film_sim::ETERNA_BLEACH,
        0xa00 => film_sim::NOSTALGIC_NEG,
        0xb00 => film_sim::REALA_ACE,
        // Monochrome / Acros often encoded via Saturation; leave default if unknown.
        _ => return None,
    })
}

fn parse_fuji_makernote_shorts(mn: &[u8]) -> HashMap<u16, u16> {
    let mut out = HashMap::new();
    if mn.len() < 14 || !mn.starts_with(b"FUJIFILM") {
        return out;
    }
    let ifd_off = u32::from_le_bytes(mn[8..12].try_into().unwrap_or([0; 4])) as usize;
    if ifd_off + 2 > mn.len() {
        return out;
    }
    let entry_count = u16::from_le_bytes(mn[ifd_off..ifd_off + 2].try_into().unwrap_or([0; 2])) as usize;
    let mut pos = ifd_off + 2;
    for _ in 0..entry_count {
        if pos + 12 > mn.len() {
            break;
        }
        let tag = u16::from_le_bytes([mn[pos], mn[pos + 1]]);
        let typ = u16::from_le_bytes([mn[pos + 2], mn[pos + 3]]);
        let count = u32::from_le_bytes([mn[pos + 4], mn[pos + 5], mn[pos + 6], mn[pos + 7]]);
        let val = u32::from_le_bytes([mn[pos + 8], mn[pos + 9], mn[pos + 10], mn[pos + 11]]);
        // TIFF SHORT inline
        if typ == 3 && count == 1 {
            out.insert(tag, (val & 0xffff) as u16);
        }
        pos += 12;
    }
    out
}

fn read_exif_map(bytes: &[u8]) -> Result<HashMap<String, String>, String> {
    let exif_reader = exif::Reader::new();
    let mut cursor = Cursor::new(bytes);
    let exif = exif_reader
        .read_from_container(&mut cursor)
        .map_err(|e| e.to_string())?;
    let mut map = HashMap::new();
    for field in exif.fields() {
        map.insert(
            field.tag.to_string(),
            field.display_value().with_unit(&exif).to_string(),
        );
    }
    Ok(map)
}

fn apply_exif_hints(recipe: &mut FujiRecipe, exif: &HashMap<String, String>) {
    if let Some(wb) = exif.get("WhiteBalance") {
        let lower = wb.to_lowercase();
        if lower.contains("auto") {
            recipe.white_balance = wb_mode::AUTO;
        } else if lower.contains("daylight") || lower.contains("sunny") {
            recipe.white_balance = wb_mode::DAYLIGHT;
        } else if lower.contains("shade") {
            recipe.white_balance = wb_mode::SHADE;
        } else if lower.contains("tungsten") || lower.contains("incandescent") {
            recipe.white_balance = wb_mode::INCANDESCENT;
        }
    }

    if let Some(mode) = exif
        .get("FilmMode")
        .or_else(|| exif.get("0x1401"))
        .cloned()
    {
        if let Some(sim) = parse_film_mode_label(&mode) {
            recipe.film_simulation = sim;
        }
    }

    let _ = (GrainPreset::Off, EffectStrength::Off, film_sim::PROVIA);
}

fn parse_film_mode_label(label: &str) -> Option<u16> {
    let l = label.to_lowercase();
    if l.contains("classic chrome") {
        Some(film_sim::CLASSIC_CHROME)
    } else if l.contains("velvia") {
        Some(film_sim::VELVIA)
    } else if l.contains("astia") {
        Some(film_sim::ASTIA)
    } else if l.contains("classic neg") {
        Some(film_sim::CLASSIC_NEG)
    } else if l.contains("nostalgic") {
        Some(film_sim::NOSTALGIC_NEG)
    } else if l.contains("reala") {
        Some(film_sim::REALA_ACE)
    } else if l.contains("eterna bleach") || l.contains("bleach bypass") {
        Some(film_sim::ETERNA_BLEACH)
    } else if l.contains("eterna") {
        Some(film_sim::ETERNA)
    } else if l.contains("acros") && (l.contains("yellow") || l.contains("+ye") || l.ends_with("ye")) {
        Some(film_sim::ACROS_YE)
    } else if l.contains("acros") && (l.contains("red") || l.contains("+r") || l.ends_with("+r")) {
        Some(film_sim::ACROS_R)
    } else if l.contains("acros") && (l.contains("green") || l.contains("+g") || l.ends_with("+g")) {
        Some(film_sim::ACROS_G)
    } else if l.contains("acros") {
        Some(film_sim::ACROS)
    } else if l.contains("provia") || l.contains("standard") {
        Some(film_sim::PROVIA)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn film_mode_label_mapping() {
        assert_eq!(
            parse_film_mode_label("Classic Chrome"),
            Some(film_sim::CLASSIC_CHROME)
        );
        assert_eq!(parse_film_mode_label("Acros+R"), Some(film_sim::ACROS_R));
    }

    #[test]
    fn empty_bytes_yield_defaults() {
        let recipe = parse_recipe_from_raf_bytes(&[]);
        assert_eq!(recipe.film_simulation, film_sim::PROVIA);
        assert_eq!(recipe.dynamic_range, 100);
    }

    #[test]
    fn exif_film_mode_maps_to_ptp() {
        assert_eq!(exif_film_mode_to_ptp(0x000), Some(film_sim::PROVIA));
        assert_eq!(exif_film_mode_to_ptp(0x200), Some(film_sim::VELVIA));
        assert_eq!(exif_film_mode_to_ptp(0x600), Some(film_sim::CLASSIC_CHROME));
        assert_eq!(exif_film_mode_to_ptp(0xb00), Some(film_sim::REALA_ACE));
    }
}
