//! Message filters, a direct port of `isMessageOk` in `src/markinim.nim`.

use std::sync::OnceLock;

use fancy_regex::Regex as FancyRegex;
use regex::Regex;

const SFW_WORDS: &str = include_str!("premium/bad-words.csv");

// The URL pattern is copied verbatim from the Nim source (std/re compatible).
const URL_PATTERN: &str = r#"(?i)\b((?:https?://|www\d{0,3}[.]|[a-z0-9.\-]+[.][a-z]{2,4}/)(?:[^\s()<>]+|\(([^\s()<>]+|(\([^\s()<>]+\)))*\))+(?:\(([^\s()<>]+|(\([^\s()<>]+\)))*\)|[^\s`!()\[\]{};:'".,<>?«»“”‘’]))"#;
const USERNAME_PATTERN: &str = r"(?i)@([a-zA-Z](_(?!_)|[a-zA-Z0-9]){3,32}[a-zA-Z0-9])";

fn sfw_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let words = SFW_WORDS
            .trim_matches([' ', '\n', '\r'])
            .split('\n')
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join("|");
        Regex::new(&format!("(?i){words}")).expect("bad-words.csv built an invalid regex")
    })
}

fn url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(URL_PATTERN).expect("invalid URL regex"))
}

fn username_regex() -> &'static FancyRegex {
    static RE: OnceLock<FancyRegex> = OnceLock::new();
    RE.get_or_init(|| FancyRegex::new(USERNAME_PATTERN).expect("invalid username regex"))
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
    if text.trim().is_empty() {
        return false;
    }
    if rules.keep_sfw && sfw_regex().is_match(text) {
        return false;
    }
    if rules.block_links && url_regex().is_match(text) {
        return false;
    }
    if rules.block_usernames && username_regex().is_match(text).unwrap_or(false) {
        return false;
    }
    true
}
