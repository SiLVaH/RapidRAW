//! Best-effort Fujifilm recipe extraction from RAF maker notes / EXIF.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::fuji_raw_conv::preset::{
    EffectStrength, FujiRecipe, GrainPreset, film_sim, wb_mode,
};

/// Parse a starting recipe from a RAF on disk.
/// Returns defaults filled with whatever FilmMode / DR / WB tags we can find.
pub fn parse_recipe_from_raf(path: &Path) -> Result<FujiRecipe, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    Ok(parse_recipe_from_raf_bytes(&bytes))
}

pub fn parse_recipe_from_raf_bytes(bytes: &[u8]) -> FujiRecipe {
    let mut recipe = FujiRecipe::default();

    // Prefer structured EXIF via kamadak-exif when the container is readable.
    if let Ok(exif) = read_exif_map(bytes) {
        apply_exif_hints(&mut recipe, &exif);
    }

    // Maker-note FilmMode tag 0x1401 is commonly a uint16 at a searchable pattern
    // inside Fujifilm MakerNote blobs. We also accept values already surfaced by EXIF.
    if let Some(film) = find_u16_tag(bytes, 0x1401) {
        recipe.film_simulation = film;
    }
    if let Some(dr) = find_u16_tag(bytes, 0x1400) {
        // DevelopmentDynamicRange sometimes stored as 100/200/400 already.
        if matches!(dr, 100 | 200 | 400) {
            recipe.dynamic_range = dr;
        } else if matches!(dr, 1 | 2 | 3) {
            recipe.dynamic_range = match dr {
                2 => 200,
                3 => 400,
                _ => 100,
            };
        }
    }

    recipe
}

fn read_exif_map(bytes: &[u8]) -> Result<HashMap<String, String>, String> {
    let exif_reader = exif::Reader::new();
    let mut cursor = std::io::Cursor::new(bytes);
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

    // Sensible defaults for grain/clarity remain Off until camera recipe is read.
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
    } else if l.contains("eterna bleach") {
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

/// Very small IFD-ish scan for a Fujifilm maker-note tag id followed by SHORT.
fn find_u16_tag(bytes: &[u8], tag: u16) -> Option<u16> {
    let tag_bytes = tag.to_le_bytes();
    let mut i = 0;
    while i + 12 < bytes.len() {
        if bytes[i] == tag_bytes[0] && bytes[i + 1] == tag_bytes[1] {
            // type SHORT = 3, count = 1 → value inline in next 4 bytes on LE TIFF
            if bytes[i + 2] == 3 && bytes[i + 3] == 0 && bytes[i + 4] == 1 && bytes[i + 5] == 0 {
                return Some(u16::from_le_bytes([bytes[i + 8], bytes[i + 9]]));
            }
        }
        i += 1;
    }
    None
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
}
