//! owoify wrapper. The `owoify_rs` crate is the Rust port of the same
//! owoify-js lineage as the Nim `owoifynim` package, with the same three
//! levels. Port of `src/utils/get_owoify_level.nim` included.

use owoify_rs::{Owoifiable, OwoifyLevel};

const LEVELS: [&str; 3] = ["owo", "uwu", "uvu"];

pub fn get_owoify_level(level: i64) -> &'static str {
    if level >= 0 && (level as usize) < LEVELS.len() {
        LEVELS[level as usize]
    } else {
        LEVELS[0]
    }
}

pub fn owoify(text: &str, level: &str) -> String {
    let level = match level {
        "uwu" => OwoifyLevel::Uwu,
        "uvu" => OwoifyLevel::Uvu,
        _ => OwoifyLevel::Owo,
    };
    text.owoify(level)
}
