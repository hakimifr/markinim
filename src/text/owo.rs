//! Port of `owoifynim`, the Nim package the original bot used, including its
//! quirks. The `owoify_rs` crate is a different algorithm (other mapping
//! lists, other regexes, a leading space in front of every kaomoji) and does
//! not match what the Nim bot produced.
//!
//! Quirks kept on purpose, all visible in the output of the Nim bot:
//! - `interleaveArrays` puts a space *before* the first word and glues the
//!   last two words together when there are as many words as spaces + 1
//!   (`"hello world"` becomes `" hewwoworld"`);
//! - a word's earlier replacements are remembered (`replacedWords`) and stop
//!   later rules from rewriting the same replacement value;
//! - rules that take a callback replace *every occurrence of the first match*
//!   as a plain string, not every match.
//!
//! The regexes run in byte mode in Nim (`nre` without `(*UTF8)`), so `\b` is
//! the ASCII word boundary here too.

use std::collections::HashMap;
use std::sync::OnceLock;

use fancy_regex::{Captures, Regex, RegexBuilder};
use rand::Rng;

use super::nim_str::is_whitespace;
use super::owo_patterns as p;

const LEVELS: [&str; 3] = ["owo", "uwu", "uvu"];

pub fn get_owoify_level(level: i64) -> &'static str {
    if level >= 0 && (level as usize) < LEVELS.len() {
        LEVELS[level as usize]
    } else {
        LEVELS[0]
    }
}

/// The two random draws `owoifynim` makes, so tests can pin them.
pub trait OwoRandom {
    /// `rand(1) > 0`, picks "owo" over "o" in the `o` rule.
    fn coin(&mut self) -> bool;
    /// `rand(len(FACES) - 1)`, the index of the kaomoji.
    fn face(&mut self) -> usize;
}

pub struct ThreadRandom;

impl OwoRandom for ThreadRandom {
    fn coin(&mut self) -> bool {
        rand::rng().random_bool(0.5)
    }

    fn face(&mut self) -> usize {
        rand::rng().random_range(0..p::FACES.len())
    }
}

pub fn owoify(text: &str, level: &str) -> String {
    owoify_with(text, level, &mut ThreadRandom)
}

pub fn owoify_with(text: &str, level: &str, rng: &mut dyn OwoRandom) -> String {
    let (mut words, spaces) = tokenize(text);

    for op in SPECIFIC_WORD_MAPPING_LIST {
        apply(&mut words, op, rng);
    }
    match level.to_lowercase().as_str() {
        "uwu" => {
            for op in UWU_MAPPING_LIST.into_iter().chain(OWO_MAPPING_LIST) {
                apply(&mut words, op, rng);
            }
        }
        "uvu" => {
            for op in UVU_MAPPING_LIST
                .into_iter()
                .chain(UWU_MAPPING_LIST)
                .chain(OWO_MAPPING_LIST)
            {
                apply(&mut words, op, rng);
            }
        }
        _ => {
            for op in OWO_MAPPING_LIST {
                apply(&mut words, op, rng);
            }
        }
    }

    let words: Vec<String> = words.into_iter().map(|w| w.word).collect();
    interleave(&words, &spaces).concat()
}

type Op = fn(&mut Word, &mut dyn OwoRandom);

fn apply(words: &mut [Word], op: Op, rng: &mut dyn OwoRandom) {
    for word in words.iter_mut() {
        op(word, rng);
    }
}

/// `findAll([^\s]+)` and `findAll(\s+)`.
fn tokenize(text: &str) -> (Vec<Word>, Vec<String>) {
    let mut words = Vec::new();
    let mut spaces = Vec::new();
    let mut rest = text;
    while let Some(first) = rest.chars().next() {
        let in_space = is_whitespace(first);
        let end = rest
            .find(|c: char| is_whitespace(c) != in_space)
            .unwrap_or(rest.len());
        let (token, tail) = rest.split_at(end);
        if in_space {
            spaces.push(token.to_owned());
        } else {
            words.push(Word::new(token));
        }
        rest = tail;
    }
    (words, spaces)
}

/// `interleaveArrays(words, spaces)`. With `words.len() >= spaces.len()` the
/// shorter list (spaces) goes first, which is what shifts the spaces.
fn interleave(words: &[String], spaces: &[String]) -> Vec<String> {
    let (min, max) = if words.len() < spaces.len() {
        (words, spaces)
    } else {
        (spaces, words)
    };
    let mut result = Vec::with_capacity(words.len() + spaces.len());
    for i in 0..min.len() {
        result.push(min[i].clone());
        result.push(max[i].clone());
    }
    if min.len() != max.len() {
        result.extend_from_slice(&max[min.len()..]);
    }
    result
}

fn regexes() -> &'static HashMap<&'static str, Regex> {
    static REGEXES: OnceLock<HashMap<&'static str, Regex>> = OnceLock::new();
    REGEXES.get_or_init(|| {
        p::ALL
            .iter()
            .map(|&pattern| {
                let compiled = RegexBuilder::new(pattern)
                    .unicode_mode(false)
                    .build()
                    .unwrap_or_else(|e| panic!("bad owoify pattern {pattern}: {e}"));
                (pattern, compiled)
            })
            .collect()
    })
}

fn re(pattern: &'static str) -> &'static Regex {
    &regexes()[pattern]
}

fn find_all(re: &Regex, s: &str) -> Vec<String> {
    re.find_iter(s)
        .filter_map(Result::ok)
        .map(|m| m.as_str().to_owned())
        .collect()
}

/// nre's `replace(str, pattern, sub)`: every match, with `$N` expanding to
/// capture group N and `$$` to a dollar sign.
fn replace_template(re: &Regex, s: &str, template: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for caps in re.captures_iter(s).filter_map(Result::ok) {
        let whole = caps.get(0).expect("group 0 always exists");
        out.push_str(&s[last..whole.start()]);
        expand(template, &caps, &mut out);
        last = whole.end();
    }
    out.push_str(&s[last..]);
    out
}

fn expand(template: &str, caps: &Captures<'_, str>, out: &mut String) {
    let bytes = template.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            if bytes[i + 1] == b'$' {
                out.push('$');
                i += 2;
                continue;
            }
            let digits = bytes[i + 1..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            if digits > 0 {
                let group: usize = template[i + 1..i + 1 + digits]
                    .parse()
                    .unwrap_or(usize::MAX);
                out.push_str(caps.get(group).map_or("", |m| m.as_str()));
                i += 1 + digits;
                continue;
            }
        }
        let ch = template[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
}

struct Word {
    word: String,
    replaced_words: Vec<String>,
}

impl Word {
    fn new(word: &str) -> Self {
        Word {
            word: word.to_owned(),
            replaced_words: Vec::new(),
        }
    }

    /// `searchValueContainsReplacedWords`.
    fn contains_replaced_words(&self, re: &Regex, replace_value: &str) -> bool {
        self.replaced_words.iter().any(|w| {
            find_all(re, w)
                .first()
                .is_some_and(|m| w.replace(m.as_str(), replace_value) == replace_value)
        })
    }

    fn commit(&mut self, replacing_word: String, collection_len: usize, replace_value: &str) {
        if replacing_word != self.word {
            if collection_len > 1 {
                self.replaced_words.extend(std::iter::repeat_n(
                    replace_value.to_owned(),
                    collection_len,
                ));
            }
            self.word = replacing_word;
        }
    }

    /// `replace`: every match, `$N` templates expanded.
    fn replace(&mut self, pattern: &'static str, replace_value: &str) {
        let re = re(pattern);
        if self.contains_replaced_words(re, replace_value) {
            return;
        }
        let collection = find_all(re, &self.word);
        let replacing_word = if collection.is_empty() {
            self.word.clone()
        } else {
            replace_template(re, &self.word, replace_value)
        };
        self.commit(replacing_word, collection.len(), replace_value);
    }

    /// `replaceWithProcSingle`: the callback runs first (and draws its random
    /// number) even when nothing matches, then every occurrence of the first
    /// match is replaced as a plain string.
    fn replace_with_proc_single(&mut self, pattern: &'static str, op: impl FnOnce() -> String) {
        let re = re(pattern);
        let replace_value = op();
        if self.contains_replaced_words(re, &replace_value) {
            return;
        }
        let collection = find_all(re, &self.word);
        let replacing_word = match collection.first() {
            Some(first) => self.word.replace(first.as_str(), &replace_value),
            None => self.word.clone(),
        };
        self.commit(replacing_word, collection.len(), &replace_value);
    }

    /// `replaceWithProcMultiple`: the callback sees capture groups 1 and 2 of
    /// the first match.
    fn replace_with_proc_multiple(
        &mut self,
        pattern: &'static str,
        op: impl FnOnce(&str, &str) -> String,
    ) {
        let re = re(pattern);
        let collection = find_all(re, &self.word);
        if collection.is_empty() {
            return;
        }
        let word = self.word.clone();
        let Ok(Some(caps)) = re.captures(&word) else {
            return;
        };
        let group = |i: usize| caps.get(i).map_or("", |m| m.as_str());
        let replace_value = op(group(1), group(2));
        if self.contains_replaced_words(re, &replace_value) {
            return;
        }
        let replacing_word = self.word.replace(collection[0].as_str(), &replace_value);
        self.commit(replacing_word, collection.len(), &replace_value);
    }
}

fn map_o_to_owo(w: &mut Word, rng: &mut dyn OwoRandom) {
    let value = if rng.coin() { "owo" } else { "o" };
    w.replace(p::O_TO_OWO, value);
}

fn map_ew_to_uwu(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::EW_TO_UWU, "uwu");
}

fn map_hey_to_hay(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::HEY_TO_HAY, "$1ay");
}

fn map_dead_to_ded(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::DEAD_TO_DED_UPPER, "Ded");
    w.replace(p::DEAD_TO_DED_LOWER, "ded");
}

fn map_n_vowel_t_to_nd(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::N_VOWEL_T_TO_ND, "nd");
}

fn map_read_to_wead(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::READ_TO_WEAD_UPPER, "Wead");
    w.replace(p::READ_TO_WEAD_LOWER, "wead");
}

fn map_brackets_to_star_trails(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::BRACKETS_TO_STARTRAILS_FORE, "｡･:*:･ﾟ★,｡･:*:･ﾟ☆");
    w.replace(p::BRACKETS_TO_STARTRAILS_REAR, "☆ﾟ･:*:･｡,★ﾟ･:*:･｡");
}

fn map_period_comma_exclamation_semicolon_to_kaomojis(w: &mut Word, rng: &mut dyn OwoRandom) {
    w.replace_with_proc_single(
        p::PERIOD_COMMA_EXCLAMATION_SEMICOLON_TO_KAOMOJIS_FIRST,
        || p::FACES[rng.face()].to_owned(),
    );
    w.replace_with_proc_single(
        p::PERIOD_COMMA_EXCLAMATION_SEMICOLON_TO_KAOMOJIS_SECOND,
        || p::FACES[rng.face()].to_owned(),
    );
}

fn map_that_to_dat(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::THAT_TO_DAT_LOWER, "dat");
    w.replace(p::THAT_TO_DAT_UPPER, "Dat");
}

fn map_th_to_f(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::TH_TO_F_LOWER, "f");
    w.replace(p::TH_TO_F_UPPER, "F");
}

fn map_le_to_wal(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::LE_TO_WAL, "wal");
}

fn map_ve_to_we(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::VE_TO_WE_LOWER, "we");
    w.replace(p::VE_TO_WE_UPPER, "We");
}

fn map_ry_to_wwy(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::RY_TO_WWY, "wwy");
}

fn map_r_or_l_to_w(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::RORL_TO_W_LOWER, "w");
    w.replace(p::RORL_TO_W_UPPER, "W");
}

fn map_ll_to_ww(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::LL_TO_WW, "ww");
}

fn map_vowel_or_r_except_o_l_to_wl(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::VOWEL_OR_R_EXCEPT_O_L_TO_WL_LOWER, "wl");
    w.replace(p::VOWEL_OR_R_EXCEPT_O_L_TO_WL_UPPER, "W$1");
}

fn map_old_to_owld(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::OLD_TO_OWLD_LOWER, "$1wld");
    w.replace(p::OLD_TO_OWLD_UPPER, "OWLD");
}

fn map_ol_to_owl(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::OL_TO_OWL_LOWER, "$1wl");
    w.replace(p::OL_TO_OWL_UPPER, "OWL");
}

fn map_l_or_r_o_to_wo(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::LORR_O_TO_WO_LOWER, "wo");
    w.replace(p::LORR_O_TO_WO_UPPER, "W$1");
}

fn map_specific_consonants_o_to_letter_and_wo(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::SPECIFIC_CONSONANTS_O_TO_LETTER_AND_WO_LOWER, "$1wo");
    w.replace_with_proc_multiple(p::SPECIFIC_CONSONANTS_O_TO_LETTER_AND_WO_UPPER, |s1, s2| {
        let wo = if s2.to_uppercase() == s2 { "W" } else { "w" };
        format!("{s1}{wo}{s2}")
    });
}

fn map_v_or_w_le_to_wal(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::VORW_LE_TO_WAL, "wal");
}

fn map_fi_to_fwi(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::FI_TO_FWI_LOWER, "$1wi");
    w.replace(p::FI_TO_FWI_UPPER, "FWI");
}

fn map_ver_to_wer(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::VER_TO_WER, "wer");
}

fn map_poi_to_pwoi(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::POI_TO_PWOI, "$1woi");
}

fn map_specific_consonants_le_to_letter_and_wal(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::SPECIFIC_CONSONANTS_LE_TO_LETTER_AND_WAL, "$1wal");
}

fn map_consonant_r_to_consonant_w(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::CONSONANT_R_TO_CONSONANT_W, "$1w");
}

fn map_ly_to_wy(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::LY_TO_WY_LOWER, "wy");
    w.replace(p::LY_TO_WY_UPPER, "Wy");
}

fn map_ple_to_pwe(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::PLE_TO_PWE, "$1we");
}

fn map_nr_to_nw(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::NR_TO_NW_LOWER, "nw");
    w.replace(p::NR_TO_NW_UPPER, "NW");
}

fn map_fuc_to_fwuc(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::FUC_TO_FWUC, "$1wuc");
}

fn map_mom_to_mwom(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::MOM_TO_MWOM, "$1wom");
}

fn map_me_to_mwe(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::ME_TO_MWE, "$1we");
}

fn map_n_vowel_to_ny(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::N_VOWEL_TO_NY_FIRST, "ny$1");
    w.replace(p::N_VOWEL_TO_NY_SECOND, "Ny$1");
    w.replace(p::N_VOWEL_TO_NY_THIRD, "NY$1");
}

fn map_ove_to_uv(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::OVE_TO_UV_LOWER, "uv");
    w.replace(p::OVE_TO_UV_UPPER, "UV");
}

fn map_haha_to_hehe_xd(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::HAHA_TO_HEHE_XD, "hehe xD");
}

fn map_the_to_teh(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::THE_TO_TEH, "$1eh");
}

fn map_you_to_u(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::YOU_TO_U_UPPER, "U");
    w.replace(p::YOU_TO_U_LOWER, "u");
}

fn map_time_to_tim(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::TIME_TO_TIM, "$1im");
}

fn map_over_to_owor(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::OVER_TO_OWOR, "$1wor");
}

fn map_worse_to_wose(w: &mut Word, _: &mut dyn OwoRandom) {
    w.replace(p::WORSE_TO_WOSE, "$1ose");
}

const SPECIFIC_WORD_MAPPING_LIST: [Op; 12] = [
    map_fuc_to_fwuc,
    map_mom_to_mwom,
    map_time_to_tim,
    map_me_to_mwe,
    map_n_vowel_to_ny,
    map_over_to_owor,
    map_ove_to_uv,
    map_haha_to_hehe_xd,
    map_the_to_teh,
    map_you_to_u,
    map_read_to_wead,
    map_worse_to_wose,
];

const UVU_MAPPING_LIST: [Op; 5] = [
    map_o_to_owo,
    map_ew_to_uwu,
    map_hey_to_hay,
    map_dead_to_ded,
    map_n_vowel_t_to_nd,
];

const UWU_MAPPING_LIST: [Op; 8] = [
    map_brackets_to_star_trails,
    map_period_comma_exclamation_semicolon_to_kaomojis,
    map_that_to_dat,
    map_th_to_f,
    map_le_to_wal,
    map_ve_to_we,
    map_ry_to_wwy,
    map_r_or_l_to_w,
];

const OWO_MAPPING_LIST: [Op; 15] = [
    map_ll_to_ww,
    map_vowel_or_r_except_o_l_to_wl,
    map_old_to_owld,
    map_ol_to_owl,
    map_l_or_r_o_to_wo,
    map_specific_consonants_o_to_letter_and_wo,
    map_v_or_w_le_to_wal,
    map_fi_to_fwi,
    map_ver_to_wer,
    map_poi_to_pwoi,
    map_specific_consonants_le_to_letter_and_wal,
    map_consonant_r_to_consonant_w,
    map_ly_to_wy,
    map_ple_to_pwe,
    map_nr_to_nw,
];
