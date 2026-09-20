//! Fujifilm D185 native RAW-conversion profile patching.
//!
//! Field indices and encodings reimplemented from public PTP behaviour
//! documented by MIT-licensed filmkit research (study-only; no source copied).
//! Confirmed layout is the camera's ~625-byte native format (X100VI).

use crate::fuji_raw_conv::preset::{EffectStrength, FujiRecipe, GrainPreset, encode_high_iso_nr};

/// Native D185 parameter indices (int32 LE slots after the header).
mod idx {
    pub const EXPOSURE_BIAS: usize = 4;
    pub const DYNAMIC_RANGE: usize = 6;
    pub const FILM_SIMULATION: usize = 8;
    pub const GRAIN_EFFECT: usize = 9;
    pub const COLOR_CHROME: usize = 10;
    pub const SMOOTH_SKIN: usize = 11;
    pub const WHITE_BALANCE: usize = 12;
    pub const WB_SHIFT_R: usize = 13;
    pub const WB_SHIFT_B: usize = 14;
    pub const WB_COLOR_TEMP: usize = 15;
    pub const HIGHLIGHT_TONE: usize = 16;
    pub const SHADOW_TONE: usize = 17;
    pub const COLOR: usize = 18;
    pub const SHARPNESS: usize = 19;
    pub const NOISE_REDUCTION: usize = 20;
    pub const CC_FX_BLUE: usize = 25;
    pub const CLARITY: usize = 27;
}

/// Patch the camera's base D185 profile with recipe fields the user controls.
/// Unspecified / sentinel slots in `base` are preserved so the camera can keep
/// as-shot values for untouched parameters.
pub fn patch_profile(base: &[u8], recipe: &FujiRecipe) -> Result<Vec<u8>, String> {
    if base.len() < 8 {
        return Err("D185 profile is too short.".into());
    }
    let mut out = base.to_vec();
    let num_params = u16::from_le_bytes([out[0], out[1]]) as usize;
    if num_params == 0 || out.len() < num_params * 4 {
        return Err(format!(
            "D185 profile header inconsistent (num_params={num_params}, len={}).",
            out.len()
        ));
    }
    let off = out.len() - num_params * 4;

    let mut set = |i: usize, val: i32| {
        let start = off + i * 4;
        if start + 4 <= out.len() {
            out[start..start + 4].copy_from_slice(&val.to_le_bytes());
        }
    };

    set(idx::FILM_SIMULATION, i32::from(recipe.film_simulation));
    set(idx::DYNAMIC_RANGE, i32::from(recipe.dynamic_range));
    set(idx::GRAIN_EFFECT, recipe.grain.to_wire());
    set(idx::COLOR_CHROME, recipe.color_chrome.to_wire());
    set(idx::SMOOTH_SKIN, recipe.smooth_skin.to_wire());
    set(idx::CC_FX_BLUE, recipe.color_chrome_fx_blue.to_wire());
    set(idx::WHITE_BALANCE, i32::from(recipe.white_balance));
    set(idx::WB_SHIFT_R, i32::from(recipe.wb_shift_r));
    set(idx::WB_SHIFT_B, i32::from(recipe.wb_shift_b));
    if recipe.white_balance == crate::fuji_raw_conv::preset::wb_mode::COLOR_TEMP {
        set(idx::WB_COLOR_TEMP, i32::from(recipe.color_temp_k));
    }
    set(idx::HIGHLIGHT_TONE, tone_x10(recipe.highlight_tone));
    set(idx::SHADOW_TONE, tone_x10(recipe.shadow_tone));
    if !recipe.is_monochrome() {
        set(idx::COLOR, tone_x10(recipe.color));
    }
    set(idx::SHARPNESS, tone_x10(recipe.sharpness));
    set(idx::NOISE_REDUCTION, encode_high_iso_nr(recipe.high_iso_nr));
    set(idx::CLARITY, tone_x10(recipe.clarity));
    // Leave ExposureBias (idx 4) as the camera's as-shot sentinel unless we add UI later.
    let _ = idx::EXPOSURE_BIAS;

    Ok(out)
}

fn tone_x10(v: f32) -> i32 {
    (v * 10.0).round() as i32
}

/// Quality for StartRawConversion (0xD183).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ConvertQuality {
    /// Half-resolution / preview JPEG (faster).
    Preview = 0,
    /// Full-resolution output (default).
    #[default]
    Full = 1,
}

impl ConvertQuality {
    pub fn to_wire(self) -> u16 {
        self as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fuji_raw_conv::preset::film_sim;

    fn synthetic_base(num_params: u16) -> Vec<u8> {
        let mut buf = vec![0u8; 8 + num_params as usize * 4];
        buf[0..2].copy_from_slice(&num_params.to_le_bytes());
        // Fill param slots with recognizable sentinel 0x7FFF_FFFF
        let off = buf.len() - num_params as usize * 4;
        for i in 0..num_params as usize {
            buf[off + i * 4..off + i * 4 + 4].copy_from_slice(&0x7FFF_FFFFu32.to_le_bytes());
        }
        buf
    }

    #[test]
    fn patch_writes_film_sim_and_dr() {
        let base = synthetic_base(32);
        let mut recipe = FujiRecipe::default();
        recipe.film_simulation = film_sim::CLASSIC_CHROME;
        recipe.dynamic_range = 200;
        recipe.highlight_tone = 1.5;
        recipe.grain = GrainPreset::WeakSmall;
        recipe.color_chrome = EffectStrength::Strong;

        let patched = patch_profile(&base, &recipe).unwrap();
        let num_params = u16::from_le_bytes([patched[0], patched[1]]) as usize;
        let off = patched.len() - num_params * 4;
        let read = |i: usize| {
            i32::from_le_bytes(patched[off + i * 4..off + i * 4 + 4].try_into().unwrap())
        };
        assert_eq!(read(idx::FILM_SIMULATION), i32::from(film_sim::CLASSIC_CHROME));
        assert_eq!(read(idx::DYNAMIC_RANGE), 200);
        assert_eq!(read(idx::HIGHLIGHT_TONE), 15);
        assert_eq!(read(idx::GRAIN_EFFECT), 2);
        assert_eq!(read(idx::COLOR_CHROME), 3);
        // Untouched ExposureBias stays sentinel
        assert_eq!(read(idx::EXPOSURE_BIAS), 0x7FFF_FFFF_u32 as i32);
    }
}
