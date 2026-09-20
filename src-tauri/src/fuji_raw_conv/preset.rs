//! Fujifilm recipe encode/decode for PTP properties D18E–D1A5.
//!
//! Encoding rules reimplemented from public PTP behaviour documented by
//! MIT-licensed filmkit research (study-only; no source copied). Values are
//! the wire encodings confirmed on X100VI — not a third-party approximation
//! of the look itself.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::fuji_raw_conv::error::{FujiRawConvError, FujiRawConvErrorKind};

/// Preset property IDs (custom recipe slot payload).
pub mod prop {
    pub const IMAGE_SIZE: u16 = 0xD18E;
    pub const IMAGE_QUALITY: u16 = 0xD18F;
    pub const DYNAMIC_RANGE: u16 = 0xD190;
    pub const UNKNOWN_D191: u16 = 0xD191;
    pub const FILM_SIMULATION: u16 = 0xD192;
    pub const MONO_WC: u16 = 0xD193;
    pub const MONO_MG: u16 = 0xD194;
    pub const GRAIN_EFFECT: u16 = 0xD195;
    pub const COLOR_CHROME: u16 = 0xD196;
    pub const COLOR_CHROME_FX_BLUE: u16 = 0xD197;
    pub const SMOOTH_SKIN: u16 = 0xD198;
    pub const WHITE_BALANCE: u16 = 0xD199;
    pub const WB_SHIFT_R: u16 = 0xD19A;
    pub const WB_SHIFT_B: u16 = 0xD19B;
    pub const COLOR_TEMP: u16 = 0xD19C;
    pub const HIGHLIGHT_TONE: u16 = 0xD19D;
    pub const SHADOW_TONE: u16 = 0xD19E;
    pub const COLOR: u16 = 0xD19F;
    pub const SHARPNESS: u16 = 0xD1A0;
    pub const HIGH_ISO_NR: u16 = 0xD1A1;
    pub const CLARITY: u16 = 0xD1A2;
    pub const LONG_EXP_NR: u16 = 0xD1A3;
    pub const COLOR_SPACE: u16 = 0xD1A4;
    pub const UNKNOWN_D1A5: u16 = 0xD1A5;
}

#[allow(dead_code)]
pub mod film_sim {
    pub const PROVIA: u16 = 0x01;
    pub const VELVIA: u16 = 0x02;
    pub const ASTIA: u16 = 0x03;
    pub const PRO_NEG_HI: u16 = 0x04;
    pub const PRO_NEG_STD: u16 = 0x05;
    pub const MONOCHROME: u16 = 0x06;
    pub const MONOCHROME_YE: u16 = 0x07;
    pub const MONOCHROME_R: u16 = 0x08;
    pub const MONOCHROME_G: u16 = 0x09;
    pub const SEPIA: u16 = 0x0A;
    pub const CLASSIC_CHROME: u16 = 0x0B;
    pub const ACROS: u16 = 0x0C;
    pub const ACROS_YE: u16 = 0x0D;
    pub const ACROS_R: u16 = 0x0E;
    pub const ACROS_G: u16 = 0x0F;
    pub const ETERNA: u16 = 0x10;
    pub const CLASSIC_NEG: u16 = 0x11;
    pub const ETERNA_BLEACH: u16 = 0x12;
    pub const NOSTALGIC_NEG: u16 = 0x13;
    pub const REALA_ACE: u16 = 0x14;
}

#[allow(dead_code)]
pub mod wb_mode {
    pub const AS_SHOT: u16 = 0x0000;
    pub const AUTO: u16 = 0x0002;
    pub const DAYLIGHT: u16 = 0x0004;
    pub const INCANDESCENT: u16 = 0x0006;
    pub const UNDERWATER: u16 = 0x0008;
    pub const FLUORESCENT_1: u16 = 0x8001;
    pub const FLUORESCENT_2: u16 = 0x8002;
    pub const FLUORESCENT_3: u16 = 0x8003;
    pub const SHADE: u16 = 0x8006;
    pub const COLOR_TEMP: u16 = 0x8007;
    pub const AMBIENCE_PRIORITY: u16 = 0x8021;
}

/// Preset grain flat enum (1–5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum GrainPreset {
    #[default]
    Off = 1,
    WeakSmall = 2,
    StrongSmall = 3,
    WeakLarge = 4,
    StrongLarge = 5,
}

impl GrainPreset {
    pub fn from_wire(v: i32) -> Self {
        match v {
            2 => Self::WeakSmall,
            3 => Self::StrongSmall,
            4 => Self::WeakLarge,
            5 => Self::StrongLarge,
            _ => Self::Off,
        }
    }

    pub fn to_wire(self) -> i32 {
        self as i32
    }
}

/// Off/Weak/Strong as stored on preset props (1-indexed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum EffectStrength {
    #[default]
    Off = 1,
    Weak = 2,
    Strong = 3,
}

impl EffectStrength {
    pub fn from_wire(v: i32) -> Self {
        match v {
            2 => Self::Weak,
            3 => Self::Strong,
            _ => Self::Off,
        }
    }

    pub fn to_wire(self) -> i32 {
        self as i32
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FujiRecipe {
    pub film_simulation: u16,
    /// Raw percentage: 100 / 200 / 400.
    pub dynamic_range: u16,
    pub highlight_tone: f32,
    pub shadow_tone: f32,
    /// Colour — omitted on write for mono sims.
    pub color: f32,
    pub sharpness: f32,
    pub grain: GrainPreset,
    pub clarity: f32,
    pub color_chrome: EffectStrength,
    pub color_chrome_fx_blue: EffectStrength,
    pub white_balance: u16,
    pub wb_shift_r: i16,
    pub wb_shift_b: i16,
    pub color_temp_k: u16,
    pub high_iso_nr: i8,
    pub mono_wc: f32,
    pub mono_mg: f32,
    pub smooth_skin: EffectStrength,
    pub long_exp_nr: bool,
    pub color_space_srgb: bool,
    pub image_size: u16,
    pub image_quality: u16,
}

impl Default for FujiRecipe {
    fn default() -> Self {
        Self {
            film_simulation: film_sim::PROVIA,
            dynamic_range: 100,
            highlight_tone: 0.0,
            shadow_tone: 0.0,
            color: 0.0,
            sharpness: 0.0,
            grain: GrainPreset::Off,
            clarity: 0.0,
            color_chrome: EffectStrength::Off,
            color_chrome_fx_blue: EffectStrength::Off,
            white_balance: wb_mode::AS_SHOT,
            wb_shift_r: 0,
            wb_shift_b: 0,
            color_temp_k: 6500,
            high_iso_nr: 0,
            mono_wc: 0.0,
            mono_mg: 0.0,
            smooth_skin: EffectStrength::Off,
            long_exp_nr: true,
            color_space_srgb: true,
            image_size: 0x07,
            image_quality: 0x02,
        }
    }
}

impl FujiRecipe {
    pub fn is_monochrome(&self) -> bool {
        matches!(
            self.film_simulation,
            film_sim::MONOCHROME
                | film_sim::MONOCHROME_YE
                | film_sim::MONOCHROME_R
                | film_sim::MONOCHROME_G
                | film_sim::SEPIA
                | film_sim::ACROS
                | film_sim::ACROS_YE
                | film_sim::ACROS_R
                | film_sim::ACROS_G
        )
    }

    /// Stable digest for cache keys (recipe content only).
    pub fn content_hash(&self) -> String {
        let bytes = serde_json::to_vec(self).unwrap_or_default();
        let digest = blake3::hash(&bytes);
        digest.to_hex().to_string()
    }

    pub fn validate_for_write(&self) -> Result<(), FujiRawConvError> {
        if !matches!(self.dynamic_range, 100 | 200 | 400) {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::Protocol,
                "Dynamic range must be 100, 200, or 400.",
            ));
        }
        if self.high_iso_nr < -4 || self.high_iso_nr > 4 {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::Protocol,
                "High ISO NR must be between -4 and +4.",
            ));
        }
        if self.white_balance == wb_mode::COLOR_TEMP
            && !(2500..=10000).contains(&self.color_temp_k)
        {
            return Err(FujiRawConvError::new(
                FujiRawConvErrorKind::Protocol,
                "Colour temperature must be between 2500 K and 10000 K.",
            ));
        }
        Ok(())
    }

    /// Encode to wire properties. Skips colour for mono sims; skips colour temp
    /// unless WB mode is Color Temp.
    pub fn to_wire_props(&self) -> Result<BTreeMap<u16, i32>, FujiRawConvError> {
        self.validate_for_write()?;
        let mut map = BTreeMap::new();
        map.insert(prop::IMAGE_SIZE, i32::from(self.image_size));
        map.insert(prop::IMAGE_QUALITY, i32::from(self.image_quality));
        map.insert(prop::DYNAMIC_RANGE, i32::from(self.dynamic_range));
        map.insert(prop::UNKNOWN_D191, 0);
        map.insert(prop::FILM_SIMULATION, i32::from(self.film_simulation));

        if self.is_monochrome() {
            map.insert(prop::MONO_WC, encode_tone(self.mono_wc));
            map.insert(prop::MONO_MG, encode_tone(self.mono_mg));
            // Colour must not be written for mono sims.
        } else {
            map.insert(prop::COLOR, encode_tone(self.color));
        }

        map.insert(prop::GRAIN_EFFECT, self.grain.to_wire());
        map.insert(prop::COLOR_CHROME, self.color_chrome.to_wire());
        map.insert(
            prop::COLOR_CHROME_FX_BLUE,
            self.color_chrome_fx_blue.to_wire(),
        );
        map.insert(prop::SMOOTH_SKIN, self.smooth_skin.to_wire());
        map.insert(prop::WHITE_BALANCE, i32::from(self.white_balance));
        map.insert(prop::WB_SHIFT_R, i32::from(self.wb_shift_r));
        map.insert(prop::WB_SHIFT_B, i32::from(self.wb_shift_b));

        if self.white_balance == wb_mode::COLOR_TEMP {
            map.insert(prop::COLOR_TEMP, i32::from(self.color_temp_k));
        }

        map.insert(prop::HIGHLIGHT_TONE, encode_tone(self.highlight_tone));
        map.insert(prop::SHADOW_TONE, encode_tone(self.shadow_tone));
        map.insert(prop::SHARPNESS, encode_tone(self.sharpness));
        map.insert(prop::HIGH_ISO_NR, encode_high_iso_nr(self.high_iso_nr));
        map.insert(prop::CLARITY, encode_tone(self.clarity));
        map.insert(prop::LONG_EXP_NR, if self.long_exp_nr { 1 } else { 0 });
        map.insert(prop::COLOR_SPACE, if self.color_space_srgb { 1 } else { 2 });
        map.insert(prop::UNKNOWN_D1A5, 7);
        Ok(map)
    }

    pub fn from_wire_props(props: &BTreeMap<u16, i32>) -> Self {
        let mut recipe = Self::default();
        if let Some(&v) = props.get(&prop::IMAGE_SIZE) {
            recipe.image_size = v as u16;
        }
        if let Some(&v) = props.get(&prop::IMAGE_QUALITY) {
            recipe.image_quality = v as u16;
        }
        if let Some(&v) = props.get(&prop::DYNAMIC_RANGE) {
            recipe.dynamic_range = v as u16;
        }
        if let Some(&v) = props.get(&prop::FILM_SIMULATION) {
            recipe.film_simulation = v as u16;
        }
        if let Some(&v) = props.get(&prop::MONO_WC) {
            recipe.mono_wc = decode_tone(v);
        }
        if let Some(&v) = props.get(&prop::MONO_MG) {
            recipe.mono_mg = decode_tone(v);
        }
        if let Some(&v) = props.get(&prop::GRAIN_EFFECT) {
            recipe.grain = GrainPreset::from_wire(v);
        }
        if let Some(&v) = props.get(&prop::COLOR_CHROME) {
            recipe.color_chrome = EffectStrength::from_wire(v);
        }
        if let Some(&v) = props.get(&prop::COLOR_CHROME_FX_BLUE) {
            recipe.color_chrome_fx_blue = EffectStrength::from_wire(v);
        }
        if let Some(&v) = props.get(&prop::SMOOTH_SKIN) {
            recipe.smooth_skin = EffectStrength::from_wire(v);
        }
        if let Some(&v) = props.get(&prop::WHITE_BALANCE) {
            recipe.white_balance = (v as u16) & 0xFFFF;
        }
        if let Some(&v) = props.get(&prop::WB_SHIFT_R) {
            recipe.wb_shift_r = v as i16;
        }
        if let Some(&v) = props.get(&prop::WB_SHIFT_B) {
            recipe.wb_shift_b = v as i16;
        }
        if let Some(&v) = props.get(&prop::COLOR_TEMP) {
            if v > 0 {
                recipe.color_temp_k = v as u16;
            }
        }
        if let Some(&v) = props.get(&prop::HIGHLIGHT_TONE) {
            recipe.highlight_tone = decode_tone(v);
        }
        if let Some(&v) = props.get(&prop::SHADOW_TONE) {
            recipe.shadow_tone = decode_tone(v);
        }
        if let Some(&v) = props.get(&prop::COLOR) {
            recipe.color = decode_tone(v);
        }
        if let Some(&v) = props.get(&prop::SHARPNESS) {
            recipe.sharpness = decode_tone(v);
        }
        if let Some(&v) = props.get(&prop::HIGH_ISO_NR) {
            recipe.high_iso_nr = decode_high_iso_nr(v);
        }
        if let Some(&v) = props.get(&prop::CLARITY) {
            recipe.clarity = decode_tone(v);
        }
        if let Some(&v) = props.get(&prop::LONG_EXP_NR) {
            recipe.long_exp_nr = v != 0;
        }
        if let Some(&v) = props.get(&prop::COLOR_SPACE) {
            recipe.color_space_srgb = v != 2;
        }
        recipe
    }

    /// Pack each property as little-endian i16/u16 payload bytes for SetDevicePropValue.
    pub fn to_prop_payloads(&self) -> Result<BTreeMap<u16, Vec<u8>>, FujiRawConvError> {
        let props = self.to_wire_props()?;
        Ok(props
            .into_iter()
            .map(|(id, value)| (id, encode_prop_payload(id, value)))
            .collect())
    }

    pub fn from_prop_payloads(payloads: &BTreeMap<u16, Vec<u8>>) -> Self {
        let mut props = BTreeMap::new();
        for (&id, bytes) in payloads {
            if let Some(v) = decode_prop_payload(id, bytes) {
                props.insert(id, v);
            }
        }
        Self::from_wire_props(&props)
    }
}

fn encode_tone(v: f32) -> i32 {
    (v * 10.0).round() as i32
}

fn decode_tone(raw: i32) -> f32 {
    if raw == -32768 || (raw as u16) == 0x8000 {
        return 0.0;
    }
    raw as f32 / 10.0
}

fn encode_high_iso_nr(level: i8) -> i32 {
    match level {
        -4 => 0x8000,
        -3 => 0x7000,
        -2 => 0x4000,
        -1 => 0x3000,
        0 => 0x2000,
        1 => 0x1000,
        2 => 0x0000,
        3 => 0x6000,
        4 => 0x5000,
        _ => 0x2000,
    }
}

fn decode_high_iso_nr(raw: i32) -> i8 {
    match raw as u16 {
        0x8000 => -4,
        0x7000 => -3,
        0x4000 => -2,
        0x3000 => -1,
        0x2000 => 0,
        0x1000 => 1,
        0x0000 => 2,
        0x6000 => 3,
        0x5000 => 4,
        _ => 0,
    }
}

fn encode_prop_payload(id: u16, value: i32) -> Vec<u8> {
    match id {
        // 32-bit-ish NR codes still travel as uint16 on the wire for presets.
        prop::HIGH_ISO_NR | prop::WHITE_BALANCE | prop::COLOR_TEMP | prop::DYNAMIC_RANGE => {
            (value as u16).to_le_bytes().to_vec()
        }
        _ => (value as i16).to_le_bytes().to_vec(),
    }
}

fn decode_prop_payload(id: u16, bytes: &[u8]) -> Option<i32> {
    if bytes.len() >= 4 {
        return Some(i32::from_le_bytes(bytes[0..4].try_into().ok()?));
    }
    if bytes.len() >= 2 {
        let u = u16::from_le_bytes(bytes[0..2].try_into().ok()?);
        return Some(match id {
            prop::HIGH_ISO_NR | prop::WHITE_BALANCE | prop::COLOR_TEMP | prop::DYNAMIC_RANGE => {
                i32::from(u)
            }
            _ => i32::from(u as i16),
        });
    }
    if bytes.len() == 1 {
        return Some(i32::from(bytes[0] as i8));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_colour_recipe() {
        let mut recipe = FujiRecipe::default();
        recipe.film_simulation = film_sim::CLASSIC_CHROME;
        recipe.dynamic_range = 200;
        recipe.highlight_tone = -1.0;
        recipe.shadow_tone = -1.0;
        recipe.color = 2.0;
        recipe.grain = GrainPreset::WeakLarge;
        recipe.color_chrome = EffectStrength::Strong;
        recipe.clarity = 1.0;
        recipe.white_balance = wb_mode::COLOR_TEMP;
        recipe.color_temp_k = 5200;
        recipe.high_iso_nr = -2;

        let payloads = recipe.to_prop_payloads().unwrap();
        assert!(payloads.contains_key(&prop::COLOR));
        assert!(payloads.contains_key(&prop::COLOR_TEMP));
        let back = FujiRecipe::from_prop_payloads(&payloads);
        assert_eq!(back.film_simulation, recipe.film_simulation);
        assert_eq!(back.dynamic_range, 200);
        assert!((back.highlight_tone - (-1.0)).abs() < 0.01);
        assert!((back.color - 2.0).abs() < 0.01);
        assert_eq!(back.grain, GrainPreset::WeakLarge);
        assert_eq!(back.color_chrome, EffectStrength::Strong);
        assert_eq!(back.white_balance, wb_mode::COLOR_TEMP);
        assert_eq!(back.color_temp_k, 5200);
        assert_eq!(back.high_iso_nr, -2);
    }

    #[test]
    fn mono_recipe_omits_colour_property() {
        let mut recipe = FujiRecipe::default();
        recipe.film_simulation = film_sim::ACROS;
        recipe.mono_wc = 2.0;
        recipe.mono_mg = -1.0;
        let props = recipe.to_wire_props().unwrap();
        assert!(!props.contains_key(&prop::COLOR));
        assert!(props.contains_key(&prop::MONO_WC));
        assert!(props.contains_key(&prop::MONO_MG));
    }

    #[test]
    fn colour_temp_only_when_wb_is_colour_temp() {
        let mut recipe = FujiRecipe::default();
        recipe.white_balance = wb_mode::DAYLIGHT;
        recipe.color_temp_k = 5600;
        let props = recipe.to_wire_props().unwrap();
        assert!(!props.contains_key(&prop::COLOR_TEMP));

        recipe.white_balance = wb_mode::COLOR_TEMP;
        let props = recipe.to_wire_props().unwrap();
        assert_eq!(props.get(&prop::COLOR_TEMP), Some(&5600));
    }

    #[test]
    fn high_iso_nr_proprietary_encoding_round_trip() {
        for level in -4i8..=4 {
            let encoded = encode_high_iso_nr(level);
            assert_eq!(decode_high_iso_nr(encoded), level);
        }
    }
}
