use markinim::text::{as_emoji, as_emoji_level, emojipasta, human_bytes_b};
use markinim::text::{get_owoify_level, owoify, random_emoji};
use rand::SeedableRng;
use rand::rngs::StdRng;

#[test]
fn as_emoji_matches_nim() {
    assert_eq!(as_emoji(true), "✳️");
    assert_eq!(as_emoji(false), "🚫");
}

#[test]
fn as_emoji_level_bounds() {
    assert_eq!(as_emoji_level(0), "(ᗒ︵ᗕ) 🚫");
    assert_eq!(as_emoji_level(3), "(๑ↀᆺↀ๑) 🌺");
    assert_eq!(as_emoji_level(4), "(ᗒ︵ᗕ) 🚫");
    assert_eq!(as_emoji_level(-1), "(ᗒ︵ᗕ) 🚫");
}

#[test]
fn owoify_level_mapping() {
    assert_eq!(get_owoify_level(0), "owo");
    assert_eq!(get_owoify_level(1), "uwu");
    assert_eq!(get_owoify_level(2), "uvu");
    assert_eq!(get_owoify_level(3), "owo"); // out of range falls back
    assert_eq!(get_owoify_level(-1), "owo");
}

#[test]
fn owoify_changes_text_on_all_levels() {
    for level in ["owo", "uwu", "uvu"] {
        let out = owoify("Hello World! Rust is fun?", level);
        assert!(!out.is_empty());
        assert_ne!(
            out, "Hello World! Rust is fun?",
            "level {level} did nothing"
        );
    }
}

#[test]
fn human_bytes_matches_nim_layout() {
    assert_eq!(human_bytes_b(0), "0B");
    assert_eq!(human_bytes_b(500), "500B");
    assert_eq!(human_bytes_b(1024), "1KiB");
    assert_eq!(human_bytes_b(500_000_000), "476MiB");
    assert_eq!(human_bytes_b(1024i64.pow(3) * 5), "5GiB");
}

#[test]
fn random_emoji_always_valid() {
    for _ in 0..200 {
        let emoji = random_emoji();
        assert!(!emoji.is_empty(), "random_emoji produced an empty string");
    }
}

#[test]
fn emojify_is_deterministic_per_seed_and_preserves_text() {
    let a = emojipasta::emojify_with("hello world", " ", 2, &mut StdRng::seed_from_u64(7));
    let b = emojipasta::emojify_with("hello world", " ", 2, &mut StdRng::seed_from_u64(7));
    assert_eq!(a, b);
    // the original chunks must survive in order (possibly with emoji inserts)
    let stripped: String = a
        .chars()
        .filter(|c| !c.is_ascii() || c.is_ascii_alphanumeric() || *c == ' ')
        .collect();
    assert!(stripped.contains("hello world"), "{a:?}");
}

#[test]
fn emojify_blocks() {
    let out = emojipasta::emojify_with("abc", " ", 0, &mut StdRng::seed_from_u64(1));
    assert_eq!(out, "abc");
}
