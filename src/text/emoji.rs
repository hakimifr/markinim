//! Ports of `src/utils/as_emoji.nim` and `src/utils/random_emoji.nim`.

pub fn as_emoji(b: bool) -> &'static str {
    if b { "✳️" } else { "🚫" }
}

const LEVELS: [&str; 4] = ["(ᗒ︵ᗕ) 🚫", "(>◡<) 👍🏻", "(^//ω/^) ✨", "(๑ↀᆺↀ๑) 🌺"];

pub fn as_emoji_level(level: i64) -> &'static str {
    if level < 0 || level as usize >= LEVELS.len() {
        return LEVELS[0];
    }
    LEVELS[level as usize]
}

// The duplicate ranges are intentional: the Nim list contains them too and
// they double the weight of those ranges when picking.
const EMOJI_RANGES: [(u32, u32); 18] = [
    (0x1F600, 0x1F64F), // Emoticons
    (0x1F300, 0x1F5FF), // Misc Symbols and Pictographs
    (0x1F680, 0x1F6FF), // Transport and Map
    (0x1F1E6, 0x1F1FF), // Regional country flags
    (0x2600, 0x26FF),   // Misc symbols
    (0x2700, 0x27BF),   // Dingbats
    (0x1F900, 0x1F9FF), // Supplemental Symbols and Pictographs
    (0x1F018, 0x1F270), // Various asian characters
    (0x238C, 0x2454),   // Misc items
    (0x1F300, 0x1F5FF), // Misc Symbols and Pictographs
    (0x1F600, 0x1F64F), // Emoticons
    (0x1F680, 0x1F6FF), // Transport and Map
    (0x1F1E6, 0x1F1FF), // Regional country flags
    (0x2600, 0x26FF),   // Misc symbols
    (0x2700, 0x27BF),   // Dingbats
    (0x1F900, 0x1F9FF), // Supplemental Symbols and Pictographs
    (0x1F018, 0x1F270), // Various asian characters
    (0x238C, 0x2454),   // Misc items
];

pub fn random_emoji() -> String {
    use rand::Rng;
    let mut rng = rand::rng();
    let range = EMOJI_RANGES[rng.random_range(0..EMOJI_RANGES.len())];
    let codepoint = rng.random_range(range.0..=range.1);
    char::from_u32(codepoint)
        .map(String::from)
        .unwrap_or_default()
}
