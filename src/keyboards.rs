//! Inline keyboards, ports of `getSettingsKeyboard` and `showSessions`.
//! The `callbackData` strings are part of the bot's contract: they must stay
//! byte-identical so inline keyboards sent by the old Nim build keep working.

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use crate::db::Session;
use crate::text::{as_emoji, as_emoji_level};

fn callback_btn(text: String, data: String) -> InlineKeyboardButton {
    InlineKeyboardButton::callback(text, data)
}

pub fn settings_keyboard(session: &Session) -> InlineKeyboardMarkup {
    let chat_id = session.chat.chat_id.to_string();
    let uuid = &session.uuid;
    InlineKeyboardMarkup {
        inline_keyboard: vec![
            vec![
                callback_btn(
                    format!("Usernames {}", as_emoji(!session.chat.block_usernames)),
                    format!("usernames_{chat_id}"),
                ),
                callback_btn(
                    format!("Links {}", as_emoji(!session.chat.block_links)),
                    format!("links_{chat_id}"),
                ),
            ],
            vec![callback_btn(
                format!("Keep SFW {}", as_emoji(session.chat.keep_sfw)),
                format!("sfw_{chat_id}"),
            )],
            vec![
                callback_btn(
                    format!("Disable /markov {}", as_emoji(session.chat.markov_disabled)),
                    format!("markov_{chat_id}"),
                ),
                callback_btn(
                    format!("Disable quotes {}", as_emoji(session.chat.quotes_disabled)),
                    format!("quotes_{chat_id}"),
                ),
            ],
            vec![callback_btn(
                format!(
                    "[BETA] Would you rather {}",
                    as_emoji(!session.chat.polls_disabled)
                ),
                format!("polls_{chat_id}"),
            )],
            vec![callback_btn(
                "Session Bound:".to_owned(),
                "nothing".to_owned(),
            )],
            vec![
                callback_btn(
                    format!("Emojipasta {}", as_emoji(session.emojipasta)),
                    format!("emojipasta_{chat_id}_{uuid}"),
                ),
                callback_btn(
                    format!("Owoify {}", as_emoji_level(session.owoify)),
                    format!("owoify_{chat_id}_{uuid}"),
                ),
            ],
            vec![callback_btn(
                format!("Case sensitive {}", as_emoji(session.case_sensitive)),
                format!("casesensivity_{chat_id}_{uuid}"),
            )],
            vec![callback_btn(
                format!("Always reply to replies {}", as_emoji(session.always_reply)),
                format!("alwaysreply_{chat_id}_{uuid}"),
            )],
            vec![callback_btn(
                format!(
                    "Randomly quote messages {}",
                    as_emoji(session.random_replies)
                ),
                format!("randomreplies_{chat_id}_{uuid}"),
            )],
            vec![callback_btn(
                format!("Pause learning {}", as_emoji(session.learning_paused)),
                format!("pauselearning_{chat_id}_{uuid}"),
            )],
        ],
    }
}

/// Port of `showSessions`: one row per session (with message count) plus the
/// "Add session" button.
pub fn sessions_keyboard(
    chat_id: i64,
    sessions: &[Session],
    default_uuid: &str,
    counts: &[i64],
) -> InlineKeyboardMarkup {
    let chat = chat_id.to_string();
    let mut rows: Vec<Vec<InlineKeyboardButton>> = sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let hat = if s.is_default || s.uuid == default_uuid {
                "🎩 "
            } else {
                ""
            };
            vec![callback_btn(
                format!("{hat}{} - {}", s.name, counts.get(i).copied().unwrap_or(0)),
                format!("set_{chat}_{}", s.uuid),
            )]
        })
        .collect();
    rows.push(vec![callback_btn(
        "Add session".to_owned(),
        format!("addsession_{chat}"),
    )]);
    InlineKeyboardMarkup {
        inline_keyboard: rows,
    }
}
