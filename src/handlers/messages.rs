//! The message path: port of `updateHandler` in `src/markinim.nim` plus the
//! session-name prompt flow that used to live in `src/utils/listen.nim`.

use std::sync::Arc;

use rand::Rng;
use teloxide::types::{ChatKind, Message, PublicChatKind};

use super::commands::{self, MsgCtx};
use super::{FlowError, ParseModeKind, SendParams, Tg};
use crate::filter;
use crate::state::{
    ANTIFLOOD_RATE, ANTIFLOOD_SECONDS, AppState, MAX_SESSION_NAME_LENGTH, unix_now,
};
use crate::text::{emojipasta, owo};

pub fn is_group_chat(chat: &teloxide::types::Chat) -> bool {
    matches!(&chat.kind, ChatKind::Public(public)
        if matches!(public.kind, PublicChatKind::Group | PublicChatKind::Supergroup(_)))
}

pub fn thread_id(msg: &Message) -> Option<i64> {
    if msg.is_topic_message {
        msg.thread_id.map(|t| t.0.0 as i64)
    } else {
        None
    }
}

/// Applies the session's owoify/emojipasta transformations.
pub fn decorate(session: &crate::db::Session, text: String) -> String {
    let mut text = text;
    if session.owoify != 0 {
        text = owo::owoify(&text, owo::get_owoify_level(session.owoify));
    }
    if session.emojipasta {
        text = emojipasta::emojify(&text);
    }
    text
}

pub async fn handle_message(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    msg: &Message,
) -> Result<(), FlowError> {
    let Some(sender) = msg.from.as_ref() else {
        return Ok(());
    };
    let sender_id = sender.id.0 as i64;
    let chat_id = msg.chat.id.0;
    let Some(text) = msg.text().or(msg.caption()) else {
        return Ok(());
    };

    // `msgUser.id notin admins and chatId in banned or msgUser.id in banned`
    // — the Nim operator precedence is kept as-is.
    let is_admin = st.admins.read().unwrap().contains(&sender_id);
    let banned = st.banned.read().unwrap().clone();
    if (!is_admin && banned.contains(&chat_id)) || banned.contains(&sender_id) {
        return Ok(());
    }

    // Port of `let user = conn.getOrInsert(database.User(userId: msgUser.id))`:
    // every text message creates the user row before the command branch.
    let user = st.db.get_or_insert_user(sender_id).await?;

    if text.starts_with('/') {
        // Admins bypass the flood gate, and (Nim short-circuit) only non-admin
        // commands are recorded by it.
        if !is_admin && st.is_flood(chat_id, ANTIFLOOD_RATE, ANTIFLOOD_SECONDS) {
            return Ok(());
        }
        let mut parts = text.split_whitespace();
        let Some(first) = parts.next() else {
            return Ok(());
        };
        let mut command = first.trim_start_matches('/');
        if let Some((name, target)) = command.split_once('@') {
            if !target.eq_ignore_ascii_case(&st.bot_username()) {
                return Ok(());
            }
            command = name;
        }
        let args: Vec<String> = parts.map(str::to_owned).collect();
        let ctx = MsgCtx {
            chat_id,
            msg_id: msg.id.0 as i64,
            thread_id: thread_id(msg),
            sender_id,
            sender_anonymous_admin: msg.sender_chat.as_ref().map(|c| c.id.0) == Some(chat_id),
            is_group: is_group_chat(&msg.chat),
            reply_to_msg_id: msg.reply_to_message().map(|r| r.id.0 as i64),
            reply_to_sender_id: msg
                .reply_to_message()
                .and_then(|r| r.from.as_ref())
                .map(|u| u.id.0 as i64),
            reply_to_sender_chat_id: msg
                .reply_to_message()
                .and_then(|r| r.sender_chat.as_ref())
                .map(|c| c.id.0),
        };
        return commands::dispatch(st, tg, &ctx, command, &args, &user).await;
    }

    // Learning path.
    let chat = st.db.get_or_insert_chat(chat_id, false).await?;
    if !chat.enabled {
        return Ok(());
    }
    let session = st.get_cached_session(chat_id).await?;
    if !filter::is_message_ok(&session.content_rules(), text) {
        return Ok(());
    }

    let learn = user.consented && !session.learning_paused;
    let existed = st
        .has_key_or_put_markov(&session, if learn { Some(text) } else { None })
        .await;
    if !existed {
        st.refill_markov(&session).await;
    } else if learn {
        let as_lower = !session.case_sensitive;
        if let Some((_, chain)) = st.markovs.lock().unwrap().get_mut(&chat_id) {
            chain.add_sample(text, as_lower);
        }
    }
    if learn {
        st.db
            .add_message(session.id, user.id, text.to_owned())
            .await?;
    }

    if !is_group_chat(&msg.chat) && !user.consented {
        tg.send_message(
            chat_id,
            &format!(
                "❕ You have not given consent to the bot's learning. Please [click here](https://t.me/{}?start=consent) to manage your data settings.",
                st.bot_username()
            ),
            SendParams {
                thread: thread_id(msg),
                reply_to: Some(msg.id.0 as i64),
                parse_mode: Some(ParseModeKind::Markdown),
                disable_preview: true,
                ..Default::default()
            },
        )
        .await?;
        return Ok(());
    }

    let mut percentage = chat.percentage;
    let reply = msg.reply_to_message();
    let replied_to_markinim =
        reply.and_then(|r| r.from.as_ref()).map(|u| u.id.0 as i64) == Some(st.me_id());
    if replied_to_markinim {
        percentage *= 2;
    }

    let mut rng = rand::rng();
    let trigger = rng.random_range(1..=100) <= percentage
        || (percentage > 0 && replied_to_markinim && session.always_reply);
    if trigger && !st.is_flood(chat_id, 10, 30) {
        // Max 10 messages per chat per 30 seconds
        let context = if session.case_sensitive {
            text.to_owned()
        } else {
            text.to_lowercase()
        };
        let generated = {
            let markovs = st.markovs.lock().unwrap();
            match markovs.get(&chat_id) {
                Some((_, chain)) => chain.generate_reply(&context, &mut rng),
                None => None,
            }
        };
        let Some(generated) = generated else {
            // The Nim build lets the MarkovGenerateError bubble to the outer
            // catch, which logs it and sends nothing.
            tracing::error!("[ERROR] | MarkovGenerateError: not enough samples");
            return Ok(());
        };
        let text = decorate(&session, generated);

        if !session.chat.quotes_disabled && rng.random_range(0..=30) == 20 {
            // Randomly send a quote
            let png = st.quote.render(&text, &mut rng);
            tg.send_photo(chat_id, png).await?;
        } else {
            let reply_here = replied_to_markinim
                || (rng.random_range(1..=100) <= (percentage / 2) && session.random_replies);
            tg.send_message(
                chat_id,
                &text,
                SendParams {
                    thread: thread_id(msg),
                    reply_to: if reply_here {
                        Some(msg.id.0 as i64)
                    } else {
                        None
                    },
                    ..Default::default()
                },
            )
            .await?;
        }
    }
    Ok(())
}

/// Consumes a message that answers a pending "addsession" prompt. Returns
/// `true` when the message was consumed (and must not be processed further),
/// exactly like `listenUpdater` swallowing the update in Nim.
pub async fn consume_pending_prompt(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    user_id: i64,
    chat_id: i64,
    msg: &Message,
) -> bool {
    let prompt = st
        .pending_prompts
        .lock()
        .unwrap()
        .get(&(user_id, chat_id))
        .copied();
    let Some(prompt) = prompt else { return false };

    if unix_now() >= prompt.expires_at {
        // Nim's timeout path: the settings message is deleted and the late
        // message is processed as a normal message.
        st.pending_prompts
            .lock()
            .unwrap()
            .remove(&(user_id, chat_id));
        let _ = tg.delete_message(chat_id, prompt.message_id).await;
        return false;
    }

    // The Nim loop only accepts text-only messages (no caption); anything
    // else is consumed but keeps the prompt alive.
    let text = msg
        .text()
        .map(str::to_owned)
        .filter(|_| msg.caption().is_none());
    let Some(text) = text else { return true };

    st.pending_prompts
        .lock()
        .unwrap()
        .remove(&(user_id, chat_id));
    finish_add_session(st, tg, chat_id, prompt.message_id, &text).await;
    true
}

/// Continuation of the `addsession` callback after the user sends a name.
async fn finish_add_session(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    chat_id: i64,
    settings_msg_id: i64,
    text: &str,
) {
    if text.to_lowercase().starts_with("/cancel") {
        let _ = tg
            .edit_message_text(
                chat_id,
                settings_msg_id,
                "*Operation cancelled...*",
                super::EditParams {
                    parse_mode: Some(ParseModeKind::Markdown),
                    ..Default::default()
                },
            )
            .await;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let _ = tg.delete_message(chat_id, settings_msg_id).await;
        return;
    }
    if text.len() > MAX_SESSION_NAME_LENGTH {
        let _ = tg
            .edit_message_text(
                chat_id,
                settings_msg_id,
                &format!(
                    "*Operation cancelled...* The session name is longer than `{MAX_SESSION_NAME_LENGTH}` characters"
                ),
                super::EditParams {
                    parse_mode: Some(ParseModeKind::Markdown),
                    ..Default::default()
                },
            )
            .await;
        return;
    }
    let Ok(Some(chat)) = st.db.get_chat(chat_id).await else {
        return;
    };
    let Ok(sessions_count) = st.db.get_sessions_count(chat_id).await else {
        return;
    };
    if sessions_count >= crate::state::MAX_FREE_SESSIONS
        || (sessions_count >= crate::state::MAX_SESSIONS && !chat.premium)
    {
        let current_max = if chat.premium {
            crate::state::MAX_SESSIONS
        } else {
            crate::state::MAX_FREE_SESSIONS
        };
        let _ = tg
            .edit_message_text(
                chat_id,
                settings_msg_id,
                &format!("You cannot add more than {current_max} sessions per chat."),
                super::EditParams::default(),
            )
            .await;
        return;
    }
    if let Ok(session) = st.db.add_session(text.to_owned(), chat_id, false).await {
        let sessions = st.db.get_sessions(chat_id).await.unwrap_or_default();
        let _ = commands::show_sessions(st, tg, chat_id, settings_msg_id, sessions).await;
        let _ = session;
    }
}

/// The consent notice shared by /percentage, /markov, /quote and
/// /wouldyourather.
pub async fn consent_notice(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
) -> Result<(), FlowError> {
    tg.send_message(
        ctx.chat_id,
        &format!(
            "❕ You have not given consent to the bot's learning. Please [click here](https://t.me/{}?start=consent) to manage your data settings.",
            st.bot_username()
        ),
        SendParams {
            thread: ctx.thread_id,
            reply_to: Some(ctx.msg_id),
            parse_mode: Some(ParseModeKind::Markdown),
            disable_preview: true,
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}
