//! Port of the `emojipasta` Nim package (itself a port of
//! EmojipastaBot's EmojipastaGenerator, MIT licensed).
//!
//! Bug-for-bug parity with the deployed Nim code:
//! - the lookup key is the text up to and including the *first* word
//!   character, so single-letter keys are what actually match;
//! - `\w` is treated as ASCII, like Nim's std/re (not Unicode word chars).

use std::sync::OnceLock;

use super::nim_case::to_lower_nim;
use super::nim_str::is_whitespace;

const MAPPINGS_JSON: &str = include_str!("../../assets/emoji-mappings.json");

/// Nim's `rand(x)`, which is inclusive on both ends: `0..=x`.
pub trait EmojiRandom {
    fn rand(&mut self, x: usize) -> usize;
}

impl<R: rand::Rng> EmojiRandom for R {
    fn rand(&mut self, x: usize) -> usize {
        self.random_range(0..=x)
    }
}

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

/// Port of `splitIntoBlocks` (`findAll(\s*[^\s]*)`): optional whitespace and
/// then a word. Nim's `re` runs in byte mode, so only ASCII whitespace counts.
fn blocks(text: &str) -> Vec<&str> {
    let mut blocks = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let word = rest.trim_start_matches(is_whitespace);
        let word_len = word.find(is_whitespace).unwrap_or(word.len());
        let end = rest.len() - word.len() + word_len;
        blocks.push(&rest[..end]);
        rest = &rest[end..];
    }
    blocks
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
    let lowered = to_lower_nim(trim_nonalpha_chars(block));
    let key = alphanumeric_prefix(&lowered);
    match mappings().get(key) {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

fn generate_emojis(chunk: &str, max_emojis_per_block: usize, rng: &mut dyn EmojiRandom) -> String {
    let matching = matching_emojis(chunk);
    let mut emojis = String::new();
    if !matching.is_empty() {
        let count = rng.rand(max_emojis_per_block);
        for _ in 0..count {
            emojis += &matching[rng.rand(matching.len() - 1)];
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
    rng: &mut dyn EmojiRandom,
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
