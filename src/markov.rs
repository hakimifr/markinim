//! Port of the `nimkov` markov chain generator.
//!
//! Behavior parity notes:
//! - words are split on single spaces, empty tokens are kept;
//! - the `__start`/`__end` sentinels are skipped inside samples;
//! - the next frame is picked uniformly among the *distinct* followers
//!   (the stored counts are kept for a future weighted variant, exactly
//!   like nimkov stores but never uses them).

use rand::Rng;
use rand::seq::IteratorRandom;
use std::collections::HashMap;

use crate::text::nim_case::to_lower_nim;

pub const START: &str = "__start";
pub const END: &str = "__end";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotEnoughSamples;

#[derive(Debug, Default)]
pub struct MarkovChain {
    pub samples: Vec<String>,
    frames: Vec<String>,
    model: HashMap<String, HashMap<String, u32>>,
}

impl MarkovChain {
    pub fn new(samples: &[String], as_lower: bool) -> Self {
        let mut chain = MarkovChain::default();
        for sample in samples {
            chain.add_sample(sample, as_lower);
        }
        chain
    }

    pub fn add_sample(&mut self, sample: &str, as_lower: bool) {
        self.samples.push(sample.to_owned());
        let text = if as_lower {
            to_lower_nim(sample)
        } else {
            sample.to_owned()
        };
        let mut local: Vec<String> = vec![START.to_owned()];
        local.extend(
            text.split(' ')
                .filter(|w| *w != START && *w != END)
                .map(str::to_owned),
        );
        local.push(END.to_owned());
        self.frames.extend(local.iter().cloned());
        for pair in local.windows(2) {
            *self
                .model
                .entry(pair[0].clone())
                .or_default()
                .entry(pair[1].clone())
                .or_insert(0) += 1;
        }
    }

    /// Every `(from, to, count)` edge of the model, sorted, for comparing the
    /// model against the one nimkov builds.
    pub fn transitions(&self) -> Vec<(String, String, u32)> {
        let mut rows: Vec<(String, String, u32)> = self
            .model
            .iter()
            .flat_map(|(from, followers)| {
                followers
                    .iter()
                    .map(move |(to, count)| (from.clone(), to.clone(), *count))
            })
            .collect();
        rows.sort();
        rows
    }

    /// Mirrors `nimkov.generate`: raises [`NotEnoughSamples`] when the samples
    /// are empty or when a `begin` frame has no followers (the caller then
    /// falls back to a plain generation, like the Nim bot does).
    pub fn generate(
        &self,
        begin: Option<&str>,
        rng: &mut impl Rng,
    ) -> Result<String, NotEnoughSamples> {
        if self.samples.is_empty() {
            return Err(NotEnoughSamples);
        }
        let start = match begin {
            Some(b) => format!("{START} {b}"),
            None => START.to_owned(),
        };
        let mut attempt: Vec<String> = start.split(' ').map(str::to_owned).collect();
        let mut current = attempt.last().unwrap().clone();
        while current != END {
            let followers = self.model.get(&current).ok_or(NotEnoughSamples)?;
            let next = followers
                .keys()
                .choose(rng)
                .ok_or(NotEnoughSamples)?
                .clone();
            attempt.push(next.clone());
            current = next;
        }
        Ok(attempt[1..attempt.len() - 1].join(" "))
    }
}
