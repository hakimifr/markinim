//! Shared bot state: caches, flood tracker, pending prompts and the sweeper.
//! Port of the threadvar caches and `cleanerWorker` in `src/markinim.nim`.
//!
//! Rule: no lock is ever held across an `.await` — every method copies the
//! data it needs out of the lock before touching the network or the DB.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, RwLock};
use std::time::Instant;

use crate::config::Config;
use crate::db::{Db, Session};
use crate::filter;
use crate::handlers::tg::Tg;
use crate::markov::MarkovChain;
use crate::quote::QuoteAssets;

pub const ANTIFLOOD_SECONDS: i64 = 10;
pub const ANTIFLOOD_RATE: i64 = 6;

pub const MARKOV_SAMPLES_CACHE_TIMEOUT: i64 = 60 * 30; // 30 minutes
pub const GROUP_ADMINS_CACHE_TIMEOUT: i64 = 60 * 5; // five minutes
pub const MARKOV_CHAT_SESSIONS_TIMEOUT: i64 = 60 * 30; // 30 minutes

pub const MAX_SESSIONS: i64 = 20;
pub const MAX_FREE_SESSIONS: i64 = 5;
pub const MAX_SESSION_NAME_LENGTH: usize = 16;
pub const PROMPT_TIMEOUT_SECS: i64 = 60 * 3;

pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[derive(Debug, Clone, Copy)]
pub struct PendingPrompt {
    pub message_id: i64,
    pub expires_at: i64,
}

pub struct AppState {
    pub db: Db,
    pub config: Config,
    pub quote: QuoteAssets,
    pub started: Instant,
    bot_username: RwLock<String>,
    me_id: RwLock<i64>,
    pub admins: RwLock<HashSet<i64>>,
    pub banned: RwLock<HashSet<i64>>,
    pub markovs: Mutex<HashMap<i64, (i64, MarkovChain)>>,
    admins_cache: Mutex<HashMap<(i64, i64), (i64, bool)>>,
    pub chat_sessions: Mutex<HashMap<i64, (i64, Session)>>,
    anti_flood: Mutex<HashMap<i64, Vec<i64>>>,
    pub pending_prompts: Mutex<HashMap<(i64, i64), PendingPrompt>>,
    deleting: Mutex<HashSet<i64>>,
}

impl AppState {
    pub fn new(db: Db, config: Config, quote: QuoteAssets) -> Self {
        Self {
            db,
            config,
            quote,
            started: Instant::now(),
            bot_username: RwLock::new(String::new()),
            me_id: RwLock::new(0),
            admins: RwLock::new(HashSet::new()),
            banned: RwLock::new(HashSet::new()),
            markovs: Mutex::new(HashMap::new()),
            admins_cache: Mutex::new(HashMap::new()),
            chat_sessions: Mutex::new(HashMap::new()),
            anti_flood: Mutex::new(HashMap::new()),
            pending_prompts: Mutex::new(HashMap::new()),
            deleting: Mutex::new(HashSet::new()),
        }
    }

    pub fn set_bot_info(&self, username: String, id: i64) {
        *self.bot_username.write().unwrap() = username;
        *self.me_id.write().unwrap() = id;
    }

    pub fn bot_username(&self) -> String {
        self.bot_username.read().unwrap().clone()
    }

    pub fn me_id(&self) -> i64 {
        *self.me_id.read().unwrap()
    }

    pub fn is_deleting(&self, chat_id: i64) -> bool {
        self.deleting.lock().unwrap().contains(&chat_id)
    }

    pub fn mark_deleting(&self, chat_id: i64) -> DeletingGuard<'_> {
        self.deleting.lock().unwrap().insert(chat_id);
        DeletingGuard {
            state: self,
            chat_id,
        }
    }

    /// Port of `isFlood`: records the message first, then reports the breach.
    pub fn is_flood(&self, chat_id: i64, rate: i64, seconds: i64) -> bool {
        let time = unix_now();
        let mut flood = self.anti_flood.lock().unwrap();
        let entry = flood.entry(chat_id).or_default();
        entry.push(time);
        entry.retain(|t| time - *t < seconds);
        entry.len() > rate as usize
    }

    /// Port of `isAdminInGroup` with its five-minute cache (failures cache
    /// `false`, exactly like the Nim `except` branch).
    pub async fn is_admin_in_group(&self, tg: &dyn Tg, chat_id: i64, user_id: i64) -> bool {
        let time = unix_now();
        {
            let cache = self.admins_cache.lock().unwrap();
            if let Some((_, is_admin)) = cache.get(&(chat_id, user_id)) {
                return *is_admin;
            }
        }
        let result = tg
            .chat_member_is_admin(chat_id, user_id)
            .await
            .unwrap_or(false);
        self.admins_cache
            .lock()
            .unwrap()
            .insert((chat_id, user_id), (time, result));
        result
    }

    /// Port of `getCachedSession`: cache hit, else load (creating the default
    /// session if the chat has none) and cache with the current timestamp.
    pub async fn get_cached_session(&self, chat_id: i64) -> Result<Session, rusqlite::Error> {
        {
            let cache = self.chat_sessions.lock().unwrap();
            if let Some((_, session)) = cache.get(&chat_id) {
                return Ok(session.clone());
            }
        }
        let session = self.db.get_default_session(chat_id).await?;
        self.cache_session(&session);
        Ok(session)
    }

    pub fn cache_session(&self, session: &Session) {
        self.chat_sessions
            .lock()
            .unwrap()
            .insert(session.chat.chat_id, (unix_now(), session.clone()));
    }

    pub fn evict_cached_session(&self, chat_id: i64) {
        self.chat_sessions.lock().unwrap().remove(&chat_id);
    }

    /// Port of `refillMarkov`: fills the chat's cached generator with the
    /// latest `keep_last` messages that pass the content filters.
    pub async fn refill_markov(&self, session: &Session) {
        let Ok(messages) = self
            .db
            .get_latest_messages(
                session.uuid.clone(),
                session.chat.chat_id,
                self.config.keep_last,
            )
            .await
        else {
            return;
        };
        let rules = session.content_rules();
        let as_lower = !session.case_sensitive;
        if let Some((_, chain)) = self.markovs.lock().unwrap().get_mut(&session.chat.chat_id) {
            for message in messages {
                if filter::is_message_ok(&rules, &message.text) {
                    chain.add_sample(&message.text, as_lower);
                }
            }
        }
    }

    /// Port of `if not markovs.hasKey(chatId): markovs[chatId] = (unixTime(), newMarkov(@[])); refillMarkov(...)`.
    pub async fn ensure_markov(&self, session: &Session) {
        {
            let mut markovs = self.markovs.lock().unwrap();
            if markovs.contains_key(&session.chat.chat_id) {
                return;
            }
            markovs.insert(session.chat.chat_id, (unix_now(), MarkovChain::default()));
        }
        self.refill_markov(session).await;
    }

    /// Port of `markovs.hasKeyOrPut(chatId, ...)` used by the message path:
    /// returns `true` when the chat already had a generator, otherwise inserts
    /// a fresh one (optionally seeded with the current message).
    pub async fn has_key_or_put_markov(&self, session: &Session, sample: Option<&str>) -> bool {
        let mut markovs = self.markovs.lock().unwrap();
        if markovs.contains_key(&session.chat.chat_id) {
            return true;
        }
        let mut samples: Vec<String> = Vec::new();
        if let Some(sample) = sample {
            samples.push(sample.to_owned());
        }
        markovs.insert(
            session.chat.chat_id,
            (
                unix_now(),
                MarkovChain::new(&samples, !session.case_sensitive),
            ),
        );
        false
    }

    pub fn rebuild_markov(&self, chat_id: i64, chain: MarkovChain) {
        self.markovs
            .lock()
            .unwrap()
            .insert(chat_id, (unix_now(), chain));
    }

    /// Port of the `cleanerWorker` body. The Nim loop slept 30 *milliseconds*
    /// (a bug); the Rust sweeper runs every 30 seconds.
    pub async fn sweep(&self, tg: &dyn Tg) {
        let time = unix_now();

        self.anti_flood.lock().unwrap().retain(|_, messages| {
            messages.retain(|t| time - *t < ANTIFLOOD_SECONDS);
            !messages.is_empty()
        });
        self.admins_cache
            .lock()
            .unwrap()
            .retain(|_, (ts, _)| time - *ts <= GROUP_ADMINS_CACHE_TIMEOUT);
        self.markovs
            .lock()
            .unwrap()
            .retain(|_, (ts, _)| time - *ts <= MARKOV_SAMPLES_CACHE_TIMEOUT);
        self.chat_sessions
            .lock()
            .unwrap()
            .retain(|_, (ts, _)| time - *ts <= MARKOV_CHAT_SESSIONS_TIMEOUT);

        let expired: Vec<(i64, i64)> = self
            .pending_prompts
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, prompt)| prompt.expires_at <= time)
            .map(|(k, _)| *k)
            .collect();
        for (user_id, chat_id) in expired {
            let prompt = self
                .pending_prompts
                .lock()
                .unwrap()
                .remove(&(user_id, chat_id));
            if let Some(prompt) = prompt
                && let Err(e) = tg.delete_message(chat_id, prompt.message_id).await
            {
                tracing::debug!("sweep: could not delete expired prompt message: {e}");
            }
        }
    }
}

/// RAII guard around the `deleting` set, mirroring the Nim
/// `deleting.incl(...)` / `finally: deleting.excl(...)` pair.
pub struct DeletingGuard<'a> {
    state: &'a AppState,
    chat_id: i64,
}

impl Drop for DeletingGuard<'_> {
    fn drop(&mut self) {
        self.state.deleting.lock().unwrap().remove(&self.chat_id);
    }
}
