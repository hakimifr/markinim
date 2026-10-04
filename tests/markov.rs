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

/// nimkov picks the next word uniformly among the DISTINCT followers
/// (counts are stored but unused). This pins that behavior: a word that
/// followed once and a word that followed 99 times must come up equally
/// often, give or take noise.
#[test]
fn sampling_is_uniform_over_distinct_followers() {
    let samples: Vec<String> = std::iter::repeat_n("a x".to_string(), 99)
        .chain(std::iter::once("a y".to_string()))
        .collect();
    let chain = MarkovChain::new(&samples, true);
    let mut rng = StdRng::seed_from_u64(42);

    let mut x_count = 0;
    let draws = 1000;
    for _ in 0..draws {
        let out = chain.generate(None, &mut rng).unwrap();
        if out.trim_end() == "a x" {
            x_count += 1;
        }
    }
    // Weighted sampling would give ~990 x's; uniform gives ~500.
    assert!(
        (350..=650).contains(&x_count),
        "expected roughly uniform sampling, got {x_count}/{draws} x's"
    );
}

#[test]
fn samples_remain_raw_even_when_lowercased() {
    let chain = chain(&["MiXeD"], true);
    assert_eq!(chain.samples, vec!["MiXeD".to_string()]);
}
