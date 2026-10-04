use std::collections::HashSet;

use markinim::markov::{MarkovChain, NotEnoughSamples};
use rand::SeedableRng;
use rand::rngs::StdRng;

fn chain(samples: &[&str], as_lower: bool) -> MarkovChain {
    let owned: Vec<String> = samples.iter().map(|s| s.to_string()).collect();
    MarkovChain::new(&owned, as_lower)
}

#[test]
fn empty_samples_error() {
    let mut rng = StdRng::seed_from_u64(1);
    let chain = MarkovChain::default();
    assert_eq!(chain.generate(None, &mut rng), Err(NotEnoughSamples));
}

#[test]
fn output_uses_only_trained_adjacencies() {
    let bigrams: HashSet<(String, String)> = [
        ("the", "cat"),
        ("cat", "sat"),
        ("cat", "ran"),
        ("sat", "here"),
        ("ran", "here"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();

    let chain = chain(&["the cat sat here", "the cat ran here"], true);
    let mut rng = StdRng::seed_from_u64(2);
    for _ in 0..200 {
        let out = chain.generate(None, &mut rng).unwrap();
        let words: Vec<&str> = out.split(' ').collect();
        assert!(
            words.len() >= 2,
            "output must contain start and end framing"
        );
        for pair in words.windows(2) {
            let bigram = (pair[0].to_string(), pair[1].to_string());
            assert!(
                bigrams.contains(&bigram),
                "unexpected adjacency {bigram:?} in {out:?}"
            );
        }
    }
}

#[test]
fn lowercasing_and_case_preservation() {
    let mut rng = StdRng::seed_from_u64(3);
    let lowered = chain(&["HeLLo World"], true);
    assert_eq!(lowered.generate(None, &mut rng).unwrap(), "hello world");

    let cased = chain(&["HeLLo World"], false);
    assert_eq!(cased.generate(None, &mut rng).unwrap(), "HeLLo World");
}

#[test]
fn begin_is_kept_and_unknown_begin_falls_back() {
    let mut rng = StdRng::seed_from_u64(4);
    let chain = chain(&["alpha beta", "alpha delta"], true);

    for _ in 0..50 {
        let out = chain.generate(Some("alpha"), &mut rng).unwrap();
        assert!(out.starts_with("alpha "), "begin word not kept: {out:?}");
        let rest = &out["alpha ".len()..];
        assert!(
            rest == "beta" || rest == "delta",
            "unexpected continuation {out:?}"
        );
    }

    // Multi-word begin: both words come back in the output.
    let out = chain.generate(Some("alpha beta"), &mut rng).unwrap();
    assert_eq!(out, "alpha beta");

    // Unknown begin word: the caller falls back to a plain generation.
    assert_eq!(chain.generate(Some("zzz"), &mut rng), Err(NotEnoughSamples));
    assert!(chain.generate(None, &mut rng).is_ok());
}

#[test]
fn sentinels_never_leak_into_output() {
    let mut rng = StdRng::seed_from_u64(5);
    let chain = chain(&["__start hello __end world"], true);
    for _ in 0..50 {
        let out = chain.generate(None, &mut rng).unwrap();
        assert!(
            !out.contains("__start") && !out.contains("__end"),
            "{out:?}"
        );
    }
    // Sentinel-like words inside samples are skipped, so the chain only
    // contains hello/world.
    assert!(chain.generate(None, &mut rng).is_ok());
}

#[test]
fn sampling_is_weighted_by_follower_counts() {
    let samples: Vec<String> = std::iter::repeat_n("a x".to_string(), 99)
        .chain(std::iter::once("a y".to_string()))
        .collect();
    let chain = MarkovChain::new(&samples, true);
    let mut rng = StdRng::seed_from_u64(42);

    let draws = 1000;
    let x_count = (0..draws)
        .filter(|_| chain.generate(None, &mut rng).unwrap() == "a x")
        .count();
    // Weighted sampling gives ~990 x's, uniform sampling would give ~500.
    assert!(x_count >= 960, "got {x_count}/{draws} x's");
}

#[test]
fn trigram_context_keeps_sentences_apart() {
    // A bigram chain could produce "the cat sat up"; the (cat, sat) context
    // only ever saw "down".
    let chain = chain(&["the cat sat down", "a dog sat up"], true);
    let mut rng = StdRng::seed_from_u64(6);
    for _ in 0..200 {
        let out = chain.generate(None, &mut rng).unwrap();
        assert!(
            out == "the cat sat down" || out == "a dog sat up",
            "{out:?}"
        );
    }
}

#[test]
fn unseen_begin_pair_backs_off_to_last_word() {
    let chain = chain(&["the cat sat down", "a dog sat up"], true);
    let mut rng = StdRng::seed_from_u64(7);
    let outs: HashSet<String> = (0..100)
        .map(|_| chain.generate(Some("the sat"), &mut rng).unwrap())
        .collect();
    let expected: HashSet<String> = ["the sat down", "the sat up"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(outs, expected);
}

const CORPUS: &[&str] = &[
    "i love pizza with cheese",
    "my dog hates pizza with pineapple",
    "pizza is the best food ever",
    "we ordered pizza last night",
    "the weather is nice today",
    "i think the weather will change",
    "my dog loves the park",
    "we went to the park last night",
    "cheese is the best snack",
    "nobody likes pineapple on anything",
];

fn adjacencies(samples: &[&str]) -> HashSet<(String, String)> {
    let mut pairs = HashSet::new();
    for sample in samples {
        let words: Vec<&str> = std::iter::once("__start")
            .chain(sample.split(' '))
            .chain(std::iter::once("__end"))
            .collect();
        for pair in words.windows(2) {
            pairs.insert((pair[0].to_string(), pair[1].to_string()));
        }
    }
    pairs
}

fn assert_trained(out: &str, pairs: &HashSet<(String, String)>) {
    let words: Vec<&str> = std::iter::once("__start")
        .chain(out.split(' '))
        .chain(std::iter::once("__end"))
        .collect();
    for pair in words.windows(2) {
        let pair = (pair[0].to_string(), pair[1].to_string());
        assert!(
            pairs.contains(&pair),
            "unexpected adjacency {pair:?} in {out:?}"
        );
    }
}

#[test]
fn every_generator_uses_only_trained_adjacencies() {
    let pairs = adjacencies(CORPUS);
    let chain = chain(CORPUS, true);
    let mut rng = StdRng::seed_from_u64(8);
    for _ in 0..100 {
        assert_trained(&chain.generate(None, &mut rng).unwrap(), &pairs);
        assert_trained(&chain.generate_best(&mut rng).unwrap(), &pairs);
        for context in ["pizza tonight?", "is the park open", "the and of"] {
            assert_trained(&chain.generate_reply(context, &mut rng).unwrap(), &pairs);
        }
    }
}

#[test]
fn reply_contains_the_keyword() {
    let chain = chain(CORPUS, true);
    let mut rng = StdRng::seed_from_u64(9);
    for (context, keyword) in [
        ("anyone up for pizza?", "pizza"),
        ("the weather sucks", "weather"),
        ("is it a dog or a cat", "dog"),
    ] {
        for _ in 0..30 {
            let out = chain.generate_reply(context, &mut rng).unwrap();
            assert!(
                out.split(' ').any(|w| w == keyword),
                "{keyword:?} missing from {out:?}"
            );
        }
    }
}

#[test]
fn reply_avoids_verbatim_samples_when_it_can() {
    let chain = chain(CORPUS, true);
    for seed in 0..50 {
        let mut rng = StdRng::seed_from_u64(seed);
        let out = chain
            .generate_reply("pizza or pineapple?", &mut rng)
            .unwrap();
        assert!(!CORPUS.contains(&out.as_str()), "verbatim copy {out:?}");
        let out = chain.generate_best(&mut rng).unwrap();
        assert!(!CORPUS.contains(&out.as_str()), "verbatim copy {out:?}");
    }

    // With a single sample there is no alternative, so copying is allowed.
    let single = self::chain(&["hello there friend"], true);
    let mut rng = StdRng::seed_from_u64(10);
    assert_eq!(
        single.generate_reply("well hello", &mut rng).as_deref(),
        Some("hello there friend")
    );
    assert_eq!(
        single.generate_best(&mut rng).as_deref(),
        Some("hello there friend")
    );
}

#[test]
fn reply_without_keywords_falls_back_to_plain_generation() {
    let chain = chain(CORPUS, true);
    let mut rng = StdRng::seed_from_u64(11);
    assert!(chain.generate_reply("the and of", &mut rng).is_some());
    assert!(chain.generate_reply("", &mut rng).is_some());
    assert!(
        chain
            .generate_reply("unknown words only", &mut rng)
            .is_some()
    );

    let empty = MarkovChain::default();
    assert_eq!(empty.generate_reply("pizza", &mut rng), None);
    assert_eq!(empty.generate_best(&mut rng), None);
}

#[test]
fn reply_and_best_never_leak_sentinels() {
    let chain = chain(&["__start pizza __end night", "pizza is good __end"], true);
    let mut rng = StdRng::seed_from_u64(12);
    for _ in 0..50 {
        for out in [
            chain.generate_best(&mut rng).unwrap(),
            chain
                .generate_reply("pizza __end __start", &mut rng)
                .unwrap(),
        ] {
            assert!(
                !out.contains("__start") && !out.contains("__end"),
                "{out:?}"
            );
        }
    }
}

#[test]
fn output_is_deterministic_under_a_seed() {
    let chain = chain(CORPUS, true);
    let run = |seed| {
        let mut rng = StdRng::seed_from_u64(seed);
        (0..5)
            .map(|_| {
                format!(
                    "{} | {} | {}",
                    chain.generate(None, &mut rng).unwrap(),
                    chain.generate_best(&mut rng).unwrap(),
                    chain
                        .generate_reply("pizza at the park?", &mut rng)
                        .unwrap()
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(run(13), run(13));
    // A rebuilt chain behaves the same: no hash-map iteration order leaks in.
    let rebuilt = self::chain(CORPUS, true);
    let mut a = StdRng::seed_from_u64(14);
    let mut b = StdRng::seed_from_u64(14);
    for _ in 0..5 {
        assert_eq!(
            chain.generate_reply("pizza at the park?", &mut a),
            rebuilt.generate_reply("pizza at the park?", &mut b)
        );
    }
}

#[test]
fn samples_remain_raw_even_when_lowercased() {
    let chain = chain(&["MiXeD"], true);
    assert_eq!(chain.samples, vec!["MiXeD".to_string()]);
}
