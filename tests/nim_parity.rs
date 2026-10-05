//! The bot's text path must behave exactly like the Nim bot it replaced.
//! Expected values come from running the original Nim code, see
//! `tests/fixtures/nim/README.md`.

use std::collections::HashMap;

use markinim::filter::{ContentRules, is_message_ok};
use markinim::handlers::commands::poll_candidates;
use markinim::markov::MarkovChain;
use markinim::text::emojipasta::{EmojiRandom, emojify_with};
use markinim::text::nim_case::to_lower_nim;
use markinim::text::nim_str;
use markinim::text::owo::{OwoRandom, owoify_with};
use serde_json::Value;

fn rows(fixture: &str) -> Vec<Value> {
    fixture
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("fixture line is json"))
        .collect()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn lowercase_matches_nim_for_every_code_point() {
    let changed: HashMap<u32, u32> = include_str!("fixtures/nim/lowercase.txt")
        .lines()
        .map(|l| {
            let (from, to) = l.split_once(' ').unwrap();
            (
                u32::from_str_radix(from, 16).unwrap(),
                u32::from_str_radix(to, 16).unwrap(),
            )
        })
        .collect();
    assert_eq!(changed.len(), 1348);

    let mut wrong = Vec::new();
    for cp in 0..=0x10FFFFu32 {
        let Some(c) = char::from_u32(cp) else {
            continue;
        };
        let expected = changed.get(&cp).copied().unwrap_or(cp);
        let actual = to_lower_nim(&c.to_string());
        if actual.chars().collect::<Vec<_>>() != [char::from_u32(expected).unwrap()] {
            wrong.push(format!("{cp:X} -> {actual:?}, Nim gives {expected:X}"));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} mismatches: {:?}",
        wrong.len(),
        &wrong[..wrong.len().min(8)]
    );
}

#[test]
fn lowercase_differs_from_rust_std_where_the_nim_bot_did() {
    // Final sigma, dotted capital I, and a script newer than Nim 2.0.2's tables.
    assert_eq!(to_lower_nim("ΟΔΥΣΣΕΥΣ"), "οδυσσευσ");
    assert_eq!(to_lower_nim("İ"), "i");
    assert_eq!(to_lower_nim("𐕰"), "𐕰");
}

#[test]
fn markov_model_matches_nimkov() {
    let cases = rows(include_str!("fixtures/nim/model.jsonl"));
    assert!(cases.len() > 90);
    for case in cases {
        let samples = strings(&case["samples"]);
        let chain = MarkovChain::new(&samples, case["as_lower"].as_bool().unwrap());

        let mut expected: Vec<(String, String, u32)> = case["model"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e[0].as_str().unwrap().to_owned(),
                    e[1].as_str().unwrap().to_owned(),
                    e[2].as_u64().unwrap() as u32,
                )
            })
            .collect();
        expected.sort();
        assert_eq!(chain.transitions(), expected, "samples {samples:?}");
        assert_eq!(chain.samples, strings(&case["samples_out"]));
    }
}

#[test]
fn whitespace_helpers_match_nim() {
    for case in rows(include_str!("fixtures/nim/strings.jsonl")) {
        let input = case["in"].as_str().unwrap();
        assert_eq!(
            nim_str::split(input),
            strings(&case["split"]),
            "split {input:?}"
        );
        assert_eq!(
            nim_str::is_blank(input),
            case["strip_empty"].as_bool().unwrap(),
            "blank {input:?}"
        );
        assert_eq!(
            to_lower_nim(input),
            case["lower"].as_str().unwrap(),
            "lower {input:?}"
        );
    }
}

#[test]
fn command_arguments_are_not_collapsed_like_in_nim() {
    // Two spaces make an empty first argument, which then has no followers in
    // the model, so /markov falls back to a plain generation.
    assert_eq!(nim_str::split("/markov  hello"), ["/markov", "", "hello"]);
    // No-break space is not whitespace to Nim.
    assert_eq!(nim_str::split("/markov\u{a0}hello"), ["/markov\u{a0}hello"]);
}

struct Fixed {
    coin: bool,
    face: usize,
}

impl OwoRandom for Fixed {
    fn coin(&mut self) -> bool {
        self.coin
    }

    fn face(&mut self) -> usize {
        self.face
    }
}

#[test]
fn owoify_matches_owoifynim() {
    let cases = rows(include_str!("fixtures/nim/owo.jsonl"));
    assert!(cases.len() > 1500);
    for case in cases {
        let mut rng = Fixed {
            coin: case["coin"].as_bool().unwrap(),
            face: case["face"].as_u64().unwrap() as usize,
        };
        let level = case["level"].as_str().unwrap();
        let input = case["in"].as_str().unwrap();
        assert_eq!(
            owoify_with(input, level, &mut rng),
            case["out"].as_str().unwrap(),
            "level {level} input {input:?}"
        );
    }
}

#[test]
fn owoify_keeps_the_interleave_quirk() {
    let mut rng = Fixed {
        coin: true,
        face: 0,
    };
    // A space in front, and the last two words glued together.
    assert_eq!(owoify_with("hello world", "owo", &mut rng), " hewwoworld");
    assert_eq!(
        owoify_with("the cat sat on the mat", "owo", &mut rng),
        " teh cat sat on tehmat"
    );
}

struct Mode(&'static str);

impl EmojiRandom for Mode {
    fn rand(&mut self, x: usize) -> usize {
        match self.0 {
            "zero" => 0,
            "half" => x / 2,
            _ => x,
        }
    }
}

#[test]
fn emojipasta_matches_the_emojipasta_package() {
    let cases = rows(include_str!("fixtures/nim/emoji.jsonl"));
    assert!(cases.len() > 500);
    for case in cases {
        let mode = match case["mode"].as_str().unwrap() {
            "zero" => "zero",
            "half" => "half",
            _ => "max",
        };
        let input = case["in"].as_str().unwrap();
        assert_eq!(
            emojify_with(input, " ", 2, &mut Mode(mode)),
            case["out"].as_str().unwrap(),
            "mode {mode} input {input:?}"
        );
    }
}

#[test]
fn content_filters_match_pcre() {
    let cases = rows(include_str!("fixtures/nim/filter.jsonl"));
    assert!(cases.len() > 500);
    for case in cases {
        let input = case["in"].as_str().unwrap();
        let blank = nim_str::is_blank(input);
        let hit = |sfw, links, users| {
            !blank && !is_message_ok(&ContentRules::new(sfw, links, users), input)
        };
        assert_eq!(blank, case["blank"].as_bool().unwrap(), "blank {input:?}");
        assert_eq!(
            hit(true, false, false),
            case["sfw"].as_bool().unwrap(),
            "sfw {input:?}"
        );
        assert_eq!(
            hit(false, true, false),
            case["url"].as_bool().unwrap(),
            "url {input:?}"
        );
        assert_eq!(
            hit(false, false, true),
            case["user"].as_bool().unwrap(),
            "username {input:?}"
        );
    }
}

#[test]
fn poll_candidates_match_the_nim_cleanup() {
    for case in rows(include_str!("fixtures/nim/poll.jsonl")) {
        assert_eq!(
            poll_candidates(strings(&case["in"])),
            strings(&case["out"]),
            "input {:?}",
            case["in"]
        );
    }
}
