//! Integration checks against real X100VI RAF samples (raw.pixls.us).
//! Skips automatically when samples are not present on disk.

use super::cache::{hash_raf_bytes, make_cache_key};
use super::preset::FujiRecipe;
use super::recipe_parse::parse_recipe_from_raf;
use std::path::Path;

const SAMPLE_COMPRESSED: &str = "/tmp/fuji-samples/DSCF0076.RAF";
const SAMPLE_LOSSLESS: &str = "/tmp/fuji-samples/DSCF0075.RAF";

fn sample_path(path: &str) -> Option<std::path::PathBuf> {
    let p = Path::new(path);
    if p.is_file() {
        Some(p.to_path_buf())
    } else {
        eprintln!("skip: missing sample {path}");
        None
    }
}

#[test]
fn parse_recipe_from_x100vi_compressed_raf() {
    let Some(path) = sample_path(SAMPLE_COMPRESSED) else {
        return;
    };
    let recipe = parse_recipe_from_raf(&path).expect("parse RAF");
    assert!(
        (0x01..=0x14).contains(&recipe.film_simulation),
        "unexpected film_simulation {:#x}",
        recipe.film_simulation
    );
    assert!(
        matches!(recipe.dynamic_range, 100 | 200 | 400),
        "unexpected DR {}",
        recipe.dynamic_range
    );
    let props = recipe.to_prop_payloads().expect("encode");
    assert!(!props.is_empty(), "encoded props must not be empty");
    let round = FujiRecipe::from_prop_payloads(&props);
    assert_eq!(round.film_simulation, recipe.film_simulation);
    assert_eq!(round.dynamic_range, recipe.dynamic_range);

    let raf = std::fs::read(&path).unwrap();
    let key = make_cache_key(&hash_raf_bytes(&raf), &recipe);
    assert!(key.contains('_'));
    assert!(key.len() >= 20);
    eprintln!(
        "DSCF0076: film={:#x} (expect Provia 0x01) dr={} props={} cache_key={}",
        recipe.film_simulation,
        recipe.dynamic_range,
        props.len(),
        key
    );
    assert_eq!(recipe.film_simulation, 0x01, "sample is Provia");
    assert_eq!(recipe.dynamic_range, 100, "sample DevelopmentDynamicRange=100");
}

#[test]
fn parse_recipe_from_x100vi_lossless_raf() {
    let Some(path) = sample_path(SAMPLE_LOSSLESS) else {
        return;
    };
    let recipe = parse_recipe_from_raf(&path).expect("parse RAF");
    assert!((0x01..=0x14).contains(&recipe.film_simulation));
    let raf = std::fs::read(&path).unwrap();
    let key_a = make_cache_key(&hash_raf_bytes(&raf), &recipe);
    let mut other = recipe.clone();
    other.color = (other.color + 1.0).clamp(-4.0, 4.0);
    let key_b = make_cache_key(&hash_raf_bytes(&raf), &other);
    assert_ne!(key_a, key_b, "recipe change must change cache key");
    eprintln!(
        "DSCF0075: film={:#x} dr={} key={}",
        recipe.film_simulation, recipe.dynamic_range, key_a
    );
}
