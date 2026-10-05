//! The `strutils` string semantics the Nim bot relied on, which differ from
//! Rust's std in ways that change what the bot does with a message.

/// Nim's `strutils.Whitespace`. Unlike `char::is_whitespace` it excludes
/// non-ASCII spaces such as NBSP (`U+00A0`) and the ideographic space.
pub fn is_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\u{b}' | '\r' | '\n' | '\u{c}')
}

/// Nim's `s.split()`. Every whitespace char is a separator on its own, so a
/// run of spaces yields empty entries (`"a  b"` is `["a", "", "b"]`) and the
/// result always has at least one entry. `/markov  word` therefore parses as
/// the arguments `["", "word"]` in the Nim bot.
pub fn split(s: &str) -> Vec<&str> {
    s.split(is_whitespace).collect()
}

/// Nim's `s.strip() == ""`.
pub fn is_blank(s: &str) -> bool {
    s.chars().all(is_whitespace)
}
