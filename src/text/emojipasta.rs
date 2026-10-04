//! Port of the `emojipasta` Nim package (itself a port of
//! EmojipastaBot's EmojipastaGenerator, MIT licensed).
//!
//! Bug-for-bug parity with the deployed Nim code:
//! - the lookup key is the text up to and including the *first* word
//!   character, so single-letter keys are what actually match;
//! - `\w` is treated as ASCII, like Nim's std/re (not Unicode word chars).

use rand::Rng;
use rand::seq::IteratorRandom;
use std::sync::OnceLock;

use regex::Regex;

const MAPPINGS_JSON: &str = include_str!("../../assets/emoji-mappings.json");

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn mappings() -> &'static serde_json::Map<String, serde_json::Value> {
    static MAPPINGS: OnceLock<serde_json::Map<String, serde_json::Value>> = OnceLock::new();
    MAPPINGS.get_or_init(|| {
        serde_json::from_str::<serde_json::Value>(MAPPINGS_JSON)
            .expect("assets/emoji-mappings.json is invalid")
            .as_object()
            .expect("emoji mappings must be a JSON object")
            .clone()
    })
}

/// Port of `splitIntoBlocks` (empty matches dropped, they contribute nothing).
fn blocks(text: &str) -> Vec<&str> {
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    let block = BLOCK.get_or_init(|| Regex::new(r"\s*[^\s]*").expect("invalid block regex"));
    block
        .find_iter(text)
        .map(|m| m.as_str())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Port of `trimNonalphaChars` (`^\W*|\W*$`) with ASCII word chars.
fn trim_nonalpha_chars(text: &str) -> &str {
    let end = text.trim_end_matches(|c| !is_word_char(c)).len();
    let head = &text[..end];
    let start = head.find(|c: char| is_word_char(c)).unwrap_or(end);
    &head[start..]
}

/// Port of `getAlphanumericPrefix`: `text[0 .. find(\w+)]`, inclusive slice.
/// When there is no word character the Nim code would raise; we return an
/// empty key which never matches, an equivalent visible outcome.
fn alphanumeric_prefix(text: &str) -> &str {
    match text.find(|c: char| is_word_char(c)) {
        Some(i) => &text[..=i],
        None => "",
    }
}

fn matching_emojis(block: &str) -> Vec<String> {
    let lowered = trim_nonalpha_chars(block).to_lowercase();
    let key = alphanumeric_prefix(&lowered);
    match mappings().get(key) {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn generate_emojis(chunk: &str, max_emojis_per_block: usize, rng: &mut impl Rng) -> String {
    let matching = matching_emojis(chunk);
    let mut emojis = String::new();
    if !matching.is_empty() {
        // Nim rand(0..max) is inclusive on both ends.
        let count = rng.random_range(0..=max_emojis_per_block);
        for _ in 0..count {
            if let Some(e) = matching.iter().choose(rng) {
                emojis += e;
            }
        }
    }
    emojis
}

pub fn emojify(s: &str) -> String {
    emojify_with(s, " ", 2, &mut rand::rng())
}

pub fn emojify_with(
    s: &str,
    word_delimiter: &str,
    max_emojis_per_block: usize,
    rng: &mut impl Rng,
) -> String {
    let mut result = String::new();
    for chunk in blocks(s) {
        let emojis = generate_emojis(chunk, max_emojis_per_block, rng);
        if !emojis.is_empty() {
            result += word_delimiter;
            result += &emojis;
        }
        result += chunk;
    }
    result
}
