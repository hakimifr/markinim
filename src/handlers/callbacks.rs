//! Port of `handleCallbackQuery` from `src/markinim.nim`.
//!
//! The `callbackData` strings are frozen (old keyboards in chats must keep
//! working), including the "casesensivity" typo. One deliberate fix: the Nim
//! code answered each query a second time after the "set"/"consent"/
//! "addsession" branches (which Telegram rejects with QUERY_ID_INVALID);
//! here every query is answered exactly once.

use std::sync::Arc;

use teloxide::types::{CallbackQuery, InlineKeyboardButton, InlineKeyboardMarkup, Message};

use super::commands::show_sessions;
use super::{
    CONSENT_TEXT, CREATOR_STRING, EditParams, FlowError, ParseModeKind, SETTINGS_TEXT, Tg,
    UNALLOWED,
};
use crate::db::Session;
use crate::filter;
use crate::keyboards;
use crate::markov::MarkovChain;
use crate::state::{
    AppState, MAX_FREE_SESSIONS, MAX_SESSIONS, PROMPT_TIMEOUT_SECS, PendingPrompt, unix_now,
};

pub fn consent_keyboard(consented: bool) -> InlineKeyboardMarkup {
    let (text, data) = if consented {
        ("Revoke consent", "consent_revoke")
    } else {
        ("Give consent", "consent_give")
    };
    InlineKeyboardMarkup {
        inline_keyboard: vec![vec![InlineKeyboardButton::callback(text, data)]],
    }
}

fn split_callback(data: &str) -> (String, Vec<String>) {
    let mut parts = data.split('_');
    let command = parts.next().unwrap_or("").to_owned();
    let args: Vec<String> = parts.map(str::to_owned).collect();
    (command, args)
}

pub async fn handle_callback(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    query: &CallbackQuery,
) -> Result<(), FlowError> {
    let user_id = query.from.id.0 as i64;
    if st.banned.read().unwrap().contains(&user_id) {
        return Ok(());
    }
    let Some(data) = query.data.as_deref() else {
        return Ok(());
    };
    let Some(message) = query.message.as_ref().and_then(|m| m.regular_message()) else {
        // Old keyboards may attach to inaccessible messages; the Nim build
        // crashed on `message.get()` and answered nothing useful either.
        return Ok(());
    };
    let (command, args) = split_callback(data);

    match callback_inner(st, tg, message, query, user_id, &command, &args).await {
        Ok(already_answered) => {
            if !already_answered {
                tg.answer_callback(&query.id.0, Some("Done!"), false)
                    .await?;
            }
            Ok(())
        }
        Err(e) => {
            if e.to_string().contains("message is not modified") {
                tg.answer_callback(&query.id.0, Some("Done!"), false)
                    .await?;
            } else {
                let _ = tg
                    .answer_callback(
                        &query.id.0,
                        Some(&format!(
                            "😔 Oh no, an ERROR occurred, try again. {CREATOR_STRING}"
                        )),
                        true,
                    )
                    .await;
            }
            Err(e)
        }
    }
}

/// Returns `Ok(true)` when the query was already answered inside the branch.
async fn callback_inner(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    message: &Message,
    query: &CallbackQuery,
    user_id: i64,
    command: &str,
    args: &[String],
) -> Result<bool, FlowError> {
    let chat_id = message.chat.id.0;
    let message_id = message.id.0 as i64;
    let is_group = super::messages::is_group_chat(&message.chat);

    match command {
        "set" => {
            if args.len() < 2 {
                tg.answer_callback(
                    &query.id.0,
                    Some("Error: try again with a new message"),
                    true,
                )
                .await?;
                return Ok(true);
            }
            let target_chat = parse_chat_id(args)?;
            let uuid = args[1].clone();
            if !admin_check(st, tg, query, chat_id, user_id, is_group).await? {
                return Ok(true);
            }

            let default = st.get_cached_session(target_chat).await?;
            if default.uuid == uuid {
                tg.answer_callback(
                    &query.id.0,
                    Some("This is already the default session for this chat"),
                    true,
                )
                .await?;
                return Ok(true);
            }

            let sessions = st.db.set_default_session(target_chat, uuid).await?;
            let mut new_default = sessions.iter().find(|s| s.is_default).cloned();
            if new_default.is_none() {
                new_default = Some(st.db.get_default_session(target_chat).await?);
            }
            let new_default = new_default.expect("a default session exists");
            st.cache_session(&new_default);

            let mut samples: Vec<String> = Vec::new();
            let messages = st
                .db
                .get_latest_messages(new_default.uuid.clone(), target_chat, st.config.keep_last)
                .await?;
            let rules = new_default.content_rules();
            for m in messages {
                if filter::is_message_ok(&rules, &m.text) {
                    samples.push(m.text);
                }
            }
            st.rebuild_markov(
                target_chat,
                MarkovChain::new(&samples, !new_default.case_sensitive),
            );

            show_sessions(st, tg, chat_id, message_id, sessions).await?;
            tg.answer_callback(&query.id.0, Some("Done"), true).await?;
            Ok(true)
        }
        "addsession" => {
            if !admin_check(st, tg, query, chat_id, user_id, is_group).await? {
                return Ok(true);
            }
            let target_chat = parse_chat_id(args)?;
            tg.answer_callback(&query.id.0, None, false).await?;

            let Some(chat) = st.db.get_chat(target_chat).await? else {
                return Err(FlowError::Other("chat not found".to_owned()));
            };
            let sessions_count = st.db.get_sessions_count(target_chat).await?;
            if sessions_count >= MAX_FREE_SESSIONS
                || (sessions_count >= MAX_SESSIONS && !chat.premium)
            {
                let current_max = if chat.premium {
                    MAX_SESSIONS
                } else {
                    MAX_FREE_SESSIONS
                };
                tg.edit_message_text(
                    chat_id,
                    message_id,
                    &format!("You cannot add more than {current_max} sessions per chat."),
                    EditParams::default(),
                )
                .await?;
                return Ok(true);
            }

            tg.edit_message_text(
                chat_id,
                message_id,
                "*Send me the name for the new session.* Send /cancel to cancel the current operation.",
                EditParams { parse_mode: Some(ParseModeKind::Markdown), ..Default::default() },
            )
            .await?;

            st.pending_prompts.lock().unwrap().insert(
                (user_id, target_chat),
                PendingPrompt {
                    message_id,
                    expires_at: unix_now() + PROMPT_TIMEOUT_SECS,
                },
            );
            Ok(true)
        }
        "nothing" => {
            tg.answer_callback(&query.id.0, Some("This button serves no purpose! ☔️"), true)
                .await?;
            Ok(true)
        }
        "consent" => {
            let give = args.first().map(String::as_str) == Some("give");
            let mut user = st.db.get_or_insert_user(user_id).await?;
            user.consented = give;
            st.db.update_user(&user).await?;
            tg.answer_callback(
                &query.id.0,
                Some("Your consent settings have been successfully updated!"),
                true,
            )
            .await?;
            let kb = consent_keyboard(user.consented);
            tg.edit_message_text(
                chat_id,
                message_id,
                CONSENT_TEXT,
                EditParams {
                    parse_mode: Some(ParseModeKind::Markdown),
                    keyboard: Some(&kb),
                },
            )
            .await?;
            Ok(true)
        }
        "usernames" | "links" | "markov" | "quotes" | "polls" | "sfw" | "casesensivity"
        | "alwaysreply" | "randomreplies" | "pauselearning" | "owoify" | "emojipasta" => {
            if !admin_check(st, tg, query, chat_id, user_id, is_group).await? {
                return Ok(true);
            }
            let target_chat = parse_chat_id(args)?;
            let mut session = st.get_cached_session(target_chat).await?;

            let chat_level = match command {
                "usernames" => {
                    session.chat.block_usernames = !session.chat.block_usernames;
                    true
                }
                "links" => {
                    session.chat.block_links = !session.chat.block_links;
                    true
                }
                "markov" => {
                    session.chat.markov_disabled = !session.chat.markov_disabled;
                    true
                }
                "quotes" => {
                    session.chat.quotes_disabled = !session.chat.quotes_disabled;
                    true
                }
                "polls" => {
                    session.chat.polls_disabled = !session.chat.polls_disabled;
                    true
                }
                "sfw" => {
                    session.chat.keep_sfw = !session.chat.keep_sfw;
                    true
                }
                "casesensivity" => {
                    session.case_sensitive = !session.case_sensitive;
                    false
                }
                "alwaysreply" => {
                    session.always_reply = !session.always_reply;
                    false
                }
                "randomreplies" => {
                    session.random_replies = !session.random_replies;
                    false
                }
                "pauselearning" => {
                    session.learning_paused = !session.learning_paused;
                    false
                }
                "owoify" => {
                    session.owoify = (session.owoify + 1) % 4;
                    false
                }
                "emojipasta" => {
                    session.emojipasta = !session.emojipasta;
                    false
                }
                _ => unreachable!(),
            };

            // The Nim build mutated the cached ref in place; the write-through
            // cache update gives the same immediate visibility.
            if chat_level {
                st.db.update_chat(&session.chat).await?;
            } else {
                st.db.update_session(&session).await?;
            }
            st.cache_session(&session);
            edit_settings(tg, chat_id, message_id, &session).await?;

            if command == "sfw" {
                tg.answer_callback(
                    &query.id.0,
                    Some("Done! NOTE: This feature is highly experimental, and it works for english messages only!"),
                    true,
                )
                .await?;
                return Ok(true);
            }
            Ok(false)
        }
        _ => Ok(false),
    }
}

/// Port of the `adminCheck` template: checks the admin status in the chat the
/// keyboard message belongs to (group chats only) and answers with a toast.
async fn admin_check(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    query: &CallbackQuery,
    chat_id: i64,
    user_id: i64,
    is_group: bool,
) -> Result<bool, FlowError> {
    if is_group && !st.is_admin_in_group(tg, chat_id, user_id).await {
        tg.answer_callback(&query.id.0, Some(UNALLOWED), true)
            .await?;
        return Ok(false);
    }
    Ok(true)
}

fn parse_chat_id(args: &[String]) -> Result<i64, FlowError> {
    args.first()
        .and_then(|a| a.parse::<i64>().ok())
        .ok_or_else(|| FlowError::Other("invalid chat id in callback data".to_owned()))
}

/// Port of the `editSettings` template.
async fn edit_settings(
    tg: &dyn Tg,
    chat_id: i64,
    message_id: i64,
    session: &Session,
) -> Result<(), FlowError> {
    let kb = keyboards::settings_keyboard(session);
    tg.edit_message_text(
        chat_id,
        message_id,
        SETTINGS_TEXT,
        EditParams {
            parse_mode: Some(ParseModeKind::Markdown),
            keyboard: Some(&kb),
        },
    )
    .await
    .map_err(FlowError::from)
}
