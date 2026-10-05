//! Message filters, a direct port of `isMessageOk` in `src/markinim.nim`.

use std::sync::OnceLock;

use fancy_regex::{BytesMode, Regex as FancyRegex, RegexBuilder as FancyRegexBuilder};
use regex::bytes::{Regex, RegexBuilder};

use crate::text::nim_str;

const SFW_WORDS: &str = include_str!("premium/bad-words.csv");

// The Nim bot compiled these with `std/re`, i.e. PCRE in byte mode: ASCII-only
// case folding, `\b`, `\s` and `\d`, and no UTF-8 awareness. They are matched
// as bytes with Unicode off to keep that behavior (`\u{17f}` must not match
// `s`, a no-break space is not whitespace, and so on).
//
// The URL pattern is the Nim one. Its last class lists `«»“”‘’`, which PCRE
// read as the individual bytes of their UTF-8 encoding, so they are spelled as
// bytes here (`\xC2\xAB \xC2\xBB \xE2\x80\x9C \xE2\x80\x9D \xE2\x80\x98 \xE2\x80\x99`).
const URL_PATTERN: &str = r#"(?i)\b((?:https?://|www\d{0,3}[.]|[a-z0-9.\-]+[.][a-z]{2,4}/)(?:[^\s()<>]+|\(([^\s()<>]+|(\([^\s()<>]+\)))*\))+(?:\(([^\s()<>]+|(\([^\s()<>]+\)))*\)|[^\s`!()\[\]{};:'".,<>?\xC2\xAB\xBB\xE2\x80\x9C\x9D\x98\x99]))"#;
const USERNAME_PATTERN: &str = r"(?i)@([a-zA-Z](_(?!_)|[a-zA-Z0-9]){3,32}[a-zA-Z0-9])";

fn byte_regex(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .unicode(false)
        .build()
        .expect("invalid filter regex")
}

fn sfw_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let words = SFW_WORDS
            .trim_matches([' ', '\n', '\r'])
            .split('\n')
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join("|");
        byte_regex(&format!("(?i){words}"))
    })
}

fn url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| byte_regex(URL_PATTERN))
}

fn username_regex() -> &'static FancyRegex {
    static RE: OnceLock<FancyRegex> = OnceLock::new();
    RE.get_or_init(|| {
        FancyRegexBuilder::new(USERNAME_PATTERN)
            .bytes_mode(BytesMode::Ascii)
            .build()
            .expect("invalid username regex")
    })
}

#[derive(Debug, Clone, Copy)]
pub struct ContentRules {
    pub keep_sfw: bool,
    pub block_links: bool,
    pub block_usernames: bool,
}

impl ContentRules {
    pub fn new(keep_sfw: bool, block_links: bool, block_usernames: bool) -> Self {
        Self {
            keep_sfw,
            block_links,
            block_usernames,
        }
    }
}

pub fn is_message_ok(rules: &ContentRules, text: &str) -> bool {
    if nim_str::is_blank(text) {
        return false;
    }
    if rules.keep_sfw && sfw_regex().is_match(text.as_bytes()) {
        return false;
    }
    if rules.block_links && url_regex().is_match(text.as_bytes()) {
        return false;
    }
    if rules.block_usernames && username_regex().is_match(text.as_bytes()).unwrap_or(false) {
        return false;
    }
    true
}
