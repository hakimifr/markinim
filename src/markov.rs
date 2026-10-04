//! Word-level markov chain behind every generated message.
//!
//! Samples are split on single spaces (empty tokens are kept, like the
//! original nimkov port) and framed by the `__start`/`__end` sentinels, which
//! are skipped when they appear inside a sample. Words are interned once and
//! the models only store `u32` ids.
//!
//! Two models are trained, one over the token stream and one over the
//! reversed stream. Each predicts the next word from a two-word (trigram)
//! context and backs off to the last word alone when that pair was never
//! seen, so every adjacency in the output was seen in training. Followers
//! are drawn proportionally to how often they were observed.
//!
//! [`MarkovChain::generate_reply`] follows MegaHAL (Hutchens 1998): pick rare
//! keywords from the incoming message, grow sentences around them in both
//! directions, and keep the one whose keywords surprise the model the most.

use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

pub const START: &str = "__start";
pub const END: &str = "__end";

type Tok = u32;
const START_TOK: Tok = 0;
const END_TOK: Tok = 1;

/// Candidates generated per scored call. Generation runs while the caller
/// holds the markov mutex, so this stays small.
const CANDIDATES: usize = 16;
const MAX_KEYWORDS: usize = 4;
const MIN_WORDS: usize = 3;
const MAX_WORDS: usize = 25;
/// Walks terminate with probability 1, but a loop of likely transitions can
/// still run for a long time while the mutex is held.
const MAX_WALK: usize = 300;

const STOPWORDS: &[&str] = &[
    "a", "about", "after", "again", "all", "also", "am", "an", "and", "any", "are", "as", "at",
    "be", "because", "been", "but", "by", "can", "could", "did", "do", "does", "don't", "dont",
    "for", "from", "get", "got", "had", "has", "have", "he", "her", "here", "him", "his", "how",
    "i", "i'm", "if", "im", "in", "into", "is", "it", "it's", "its", "just", "like", "lol", "me",
    "more", "my", "no", "not", "now", "of", "oh", "ok", "okay", "on", "one", "or", "our", "out",
    "really", "she", "so", "some", "than", "that", "the", "their", "them", "then", "there", "they",
    "this", "to", "too", "up", "very", "was", "we", "were", "what", "when", "where", "which",
    "who", "why", "will", "with", "would", "yeah", "yes", "you", "your",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotEnoughSamples;

#[derive(Debug, Default)]
struct Followers {
    total: u32,
    next: Vec<(Tok, u32)>,
}

impl Followers {
    fn add(&mut self, tok: Tok) {
        self.total += 1;
        match self.next.iter_mut().find(|(t, _)| *t == tok) {
            Some((_, count)) => *count += 1,
            None => self.next.push((tok, 1)),
        }
    }

    fn probability(&self, tok: Tok) -> f64 {
        let count = self.next.iter().find(|(t, _)| *t == tok).map_or(0, |e| e.1);
        f64::from(count) / f64::from(self.total)
    }

    fn sample(&self, rng: &mut impl Rng) -> Tok {
        let mut roll = rng.random_range(0..self.total);
        for &(tok, count) in &self.next {
            if roll < count {
                return tok;
            }
            roll -= count;
        }
        unreachable!("total is the sum of the counts")
    }
}

/// Trigram model with backoff to bigrams.
#[derive(Debug, Default)]
struct Model {
    order1: HashMap<Tok, Followers>,
    order2: HashMap<(Tok, Tok), Followers>,
}

impl Model {
    fn learn(&mut self, toks: &[Tok]) {
        for w in toks.windows(2) {
            self.order1.entry(w[0]).or_default().add(w[1]);
        }
        for w in toks.windows(3) {
            self.order2.entry((w[0], w[1])).or_default().add(w[2]);
        }
    }

    fn followers(&self, seq: &[Tok]) -> Option<&Followers> {
        let (&last, rest) = seq.split_last()?;
        rest.last()
            .and_then(|&prev| self.order2.get(&(prev, last)))
            .or_else(|| self.order1.get(&last))
    }

    /// Extends `seq` until it ends with `stop`. Returns `false` when the
    /// walk got stuck or too long.
    fn walk(&self, seq: &mut Vec<Tok>, stop: Tok, rng: &mut impl Rng) -> bool {
        while seq.last() != Some(&stop) {
            if seq.len() >= MAX_WALK {
                return false;
            }
            let Some(followers) = self.followers(seq) else {
                return false;
            };
            seq.push(followers.sample(rng));
        }
        true
    }

    /// Probability of `next` after `seq`, averaged over the context orders
    /// available, as MegaHAL does.
    fn probability(&self, seq: &[Tok], next: Tok) -> f64 {
        let Some((&last, rest)) = seq.split_last() else {
            return 0.0;
        };
        let contexts = [
            self.order1.get(&last),
            rest.last().and_then(|&prev| self.order2.get(&(prev, last))),
        ];
        let (sum, orders) = contexts
            .into_iter()
            .flatten()
            .fold((0.0, 0.0), |(sum, n), f| {
                (sum + f.probability(next), n + 1.0)
            });
        if orders > 0.0 { sum / orders } else { 0.0 }
    }
}

#[derive(Debug)]
pub struct MarkovChain {
    pub samples: Vec<String>,
    words: Vec<Arc<str>>,
    ids: HashMap<Arc<str>, Tok>,
    forward: Model,
    backward: Model,
    /// Hashes of the tokenised samples, to spot candidates that merely
    /// repeat one of them.
    seen: HashSet<u64>,
    word_count: usize,
}

impl Default for MarkovChain {
    fn default() -> Self {
        let mut chain = MarkovChain {
            samples: Vec::new(),
            words: Vec::new(),
            ids: HashMap::new(),
            forward: Model::default(),
            backward: Model::default(),
            seen: HashSet::new(),
            word_count: 0,
        };
        // Interned first so they get the START_TOK and END_TOK ids.
        chain.intern(START);
        chain.intern(END);
        chain
    }
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
            sample.to_lowercase()
        } else {
            sample.to_owned()
        };
        let mut toks = vec![START_TOK];
        for word in text.split(' ').filter(|w| *w != START && *w != END) {
            toks.push(self.intern(word));
        }
        toks.push(END_TOK);
        self.word_count += toks.len() - 2;
        self.seen.insert(sequence_hash(&toks[1..toks.len() - 1]));
        self.forward.learn(&toks);
        toks.reverse();
        self.backward.learn(&toks);
    }

    /// Generates one sentence. With `begin`, the begin words are the prefix
    /// of the output and [`NotEnoughSamples`] is returned when the last one
    /// is unknown (the caller then falls back to a plain generation).
    pub fn generate(
        &self,
        begin: Option<&str>,
        rng: &mut impl Rng,
    ) -> Result<String, NotEnoughSamples> {
        if self.samples.is_empty() {
            return Err(NotEnoughSamples);
        }
        let prefix: Vec<&str> = begin.map_or_else(Vec::new, |b| b.split(' ').collect());
        let mut seq = vec![START_TOK];
        if let Some((&last, rest)) = prefix.split_last() {
            let last = self.id(last).ok_or(NotEnoughSamples)?;
            if last == END_TOK {
                return Ok(rest.join(" "));
            }
            seq = match rest.last() {
                None => vec![START_TOK, last],
                Some(prev) => self.id(prev).map_or(vec![last], |prev| vec![prev, last]),
            };
        }
        let context = seq.len();
        // A walk cut short by MAX_WALK is still a valid (truncated) sentence.
        self.forward.walk(&mut seq, END_TOK, rng);
        let generated = seq[context..]
            .strip_suffix(&[END_TOK])
            .unwrap_or(&seq[context..]);
        let mut words = prefix;
        words.extend(generated.iter().map(|&t| self.word(t)));
        Ok(words.join(" "))
    }

    /// Generates several sentences and returns the most fluent one (highest
    /// mean log-probability per transition), preferring sentences that are
    /// not a verbatim sample and have a reasonable length.
    pub fn generate_best(&self, rng: &mut impl Rng) -> Option<String> {
        if self.samples.is_empty() {
            return None;
        }
        let candidates = (0..CANDIDATES)
            .filter_map(|_| {
                let mut seq = vec![START_TOK];
                self.forward
                    .walk(&mut seq, END_TOK, rng)
                    .then(|| seq[1..seq.len() - 1].to_vec())
            })
            .collect();
        let best = self.pick(candidates, |words| self.fluency(words))?;
        Some(self.render(&best))
    }

    /// MegaHAL-style reply to `context`: sentences are built around the
    /// rarest known words of the message and scored by how surprising those
    /// words are in them. Falls back to [`Self::generate_best`] when the
    /// message has no usable keyword. `context` must be lowercased like the
    /// samples when the chain is not case-sensitive.
    pub fn generate_reply(&self, context: &str, rng: &mut impl Rng) -> Option<String> {
        if self.samples.is_empty() {
            return None;
        }
        let keywords = self.keywords(context);
        // A word seen only once usually comes from the message being answered
        // (it is learned before the reply), and building around it would just
        // echo that message back.
        let mut seeds: Vec<Tok> = keywords
            .iter()
            .copied()
            .filter(|&t| self.occurrences(t) >= 2)
            .take(MAX_KEYWORDS)
            .collect();
        if seeds.is_empty() {
            seeds = keywords.iter().copied().take(MAX_KEYWORDS).collect();
        }
        if seeds.is_empty() {
            return self.generate_best(rng);
        }
        let candidates: Vec<Vec<Tok>> = (0..CANDIDATES)
            .filter_map(|i| self.sentence_around(seeds[i % seeds.len()], rng))
            .filter(|words| !self.is_echo(words, context))
            .collect();
        if candidates.is_empty() {
            return self.generate_best(rng);
        }
        let best = self.pick(candidates, |words| self.surprise(words, &keywords))?;
        Some(self.render(&best))
    }

    fn intern(&mut self, word: &str) -> Tok {
        if let Some(&id) = self.ids.get(word) {
            return id;
        }
        let id = Tok::try_from(self.words.len()).expect("vocabulary fits in u32");
        let word: Arc<str> = word.into();
        self.words.push(word.clone());
        self.ids.insert(word, id);
        id
    }

    fn id(&self, word: &str) -> Option<Tok> {
        self.ids.get(word).copied()
    }

    fn word(&self, tok: Tok) -> &str {
        &self.words[tok as usize]
    }

    fn render(&self, words: &[Tok]) -> String {
        let words: Vec<&str> = words.iter().map(|&t| self.word(t)).collect();
        words.join(" ")
    }

    fn occurrences(&self, tok: Tok) -> u32 {
        self.forward.order1.get(&tok).map_or(0, |f| f.total)
    }

    /// Known, non-stopword, not overly frequent words of `context`, rarest
    /// first (ties keep message order).
    fn keywords(&self, context: &str) -> Vec<Tok> {
        let mut found: Vec<Tok> = Vec::new();
        for raw in context.split_whitespace() {
            let trimmed = raw.trim_matches(|c: char| !c.is_alphanumeric());
            if trimmed.is_empty() || STOPWORDS.contains(&trimmed.to_lowercase().as_str()) {
                continue;
            }
            let Some(tok) = self.id(raw).or_else(|| self.id(trimmed)) else {
                continue;
            };
            let count = self.occurrences(tok) as usize;
            // Over 1% of all words: catches function words of languages the
            // stopword list misses. The floor keeps small corpora, where
            // topic words easily pass 1%, unaffected.
            let too_common = count >= 20 && count * 100 > self.word_count;
            if tok > END_TOK && !too_common && !found.contains(&tok) {
                found.push(tok);
            }
        }
        found.sort_by_key(|&t| self.occurrences(t));
        found
    }

    /// Walks forward from `keyword` to the end, then backward from it to
    /// the start, using the first forward word as backward context.
    fn sentence_around(&self, keyword: Tok, rng: &mut impl Rng) -> Option<Vec<Tok>> {
        let mut forward = vec![keyword];
        if !self.forward.walk(&mut forward, END_TOK, rng) {
            return None;
        }
        // [next, keyword], or [__end, keyword] when the keyword ends the sentence.
        let mut backward: Vec<Tok> = forward.iter().take(2).rev().copied().collect();
        if !self.backward.walk(&mut backward, START_TOK, rng) {
            return None;
        }
        let mut words: Vec<Tok> = backward[2..backward.len() - 1]
            .iter()
            .rev()
            .copied()
            .collect();
        words.extend_from_slice(&forward[..forward.len() - 1]);
        Some(words)
    }

    fn is_copy(&self, words: &[Tok]) -> bool {
        self.seen.contains(&sequence_hash(words))
    }

    fn is_echo(&self, words: &[Tok], context: &str) -> bool {
        context
            .split(' ')
            .filter(|w| *w != START && *w != END)
            .eq(words.iter().map(|&t| self.word(t)))
    }

    /// Best candidate by `score`, where being a verbatim sample weighs more
    /// than being outside the length band, and both only matter when a
    /// better candidate exists.
    fn pick(&self, candidates: Vec<Vec<Tok>>, score: impl Fn(&[Tok]) -> f64) -> Option<Vec<Tok>> {
        candidates
            .into_iter()
            .map(|words| {
                let penalty = 2 * u8::from(self.is_copy(&words))
                    + u8::from(!(MIN_WORDS..=MAX_WORDS).contains(&words.len()));
                (penalty, score(&words), words)
            })
            .min_by(|a, b| a.0.cmp(&b.0).then(b.1.total_cmp(&a.1)))
            .map(|(_, _, words)| words)
    }

    /// Mean log2-probability of every forward transition, `__end` included.
    fn fluency(&self, words: &[Tok]) -> f64 {
        let seq = framed(words);
        let total: f64 = (1..seq.len())
            .map(|i| self.forward.probability(&seq[..i], seq[i]).log2())
            .sum();
        total / (seq.len() - 1) as f64
    }

    /// MegaHAL's evaluate_reply: the information (-log2 p) the keywords carry
    /// in both directions, damped for replies with many keyword terms.
    fn surprise(&self, words: &[Tok], keywords: &[Tok]) -> f64 {
        let mut seq = framed(words);
        let mut entropy = 0.0;
        let mut terms = 0usize;
        for model in [&self.forward, &self.backward] {
            for i in 1..seq.len() - 1 {
                if keywords.contains(&seq[i]) {
                    entropy -= model.probability(&seq[..i], seq[i]).log2();
                    terms += 1;
                }
            }
            seq.reverse();
        }
        if terms >= 8 {
            entropy /= ((terms - 1) as f64).sqrt();
        }
        if terms >= 16 {
            entropy /= terms as f64;
        }
        entropy
    }
}

fn framed(words: &[Tok]) -> Vec<Tok> {
    let mut seq = Vec::with_capacity(words.len() + 2);
    seq.push(START_TOK);
    seq.extend_from_slice(words);
    seq.push(END_TOK);
    seq
}

fn sequence_hash(words: &[Tok]) -> u64 {
    let mut hasher = DefaultHasher::new();
    words.hash(&mut hasher);
    hasher.finish()
}
