pub mod callbacks;
pub mod commands;
pub mod messages;
pub mod tg;

use std::sync::Arc;

use teloxide::types::{MaybeInaccessibleMessage, Update, UpdateKind};

use crate::state::AppState;
pub use tg::{EditParams, ParseModeKind, SendParams, TeloxideBot, Tg, TgError};

pub const UNALLOWED: &str = "You are not allowed to perform this command";
pub const CREATOR_STRING: &str =
    " Please contact my creator if you think this is a mistake (more information on @Markinim)";
pub const SETTINGS_TEXT: &str = "Tap on a button to toggle an option. Use /percentage to change the ratio of answers from the bot. Use /sessions to manage the sessions.";
pub const CONSENT_TEXT: &str = "Markinim is an opt-in service. With the options below, you can manage your data settings. For more information, see /privacy.";
pub const HELP_TEXT: &str = include_str!("../help.md");
pub const PRIVACY_TEXT: &str = include_str!("../privacy.md");

#[derive(Debug)]
pub enum FlowError {
    Tg(TgError),
    Db(rusqlite::Error),
    Other(String),
}

impl From<TgError> for FlowError {
    fn from(e: TgError) -> Self {
        FlowError::Tg(e)
    }
}

impl From<rusqlite::Error> for FlowError {
    fn from(e: rusqlite::Error) -> Self {
        FlowError::Db(e)
    }
}

impl std::fmt::Display for FlowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlowError::Tg(e) => write!(f, "{e}"),
            FlowError::Db(e) => write!(f, "{e}"),
            FlowError::Other(e) => write!(f, "{e}"),
        }
    }
}

pub async fn handle_update(st: &Arc<AppState>, tg: &dyn Tg, update: Update) {
    match update.kind {
        UpdateKind::Message(message) => {
            let Some(sender) = message.from.as_ref() else {
                return;
            };
            let user_id = sender.id.0 as i64;
            let chat_id = message.chat.id.0;
            // Pending session-name prompts are consumed before anything else,
            // mirroring listenUpdater running first in the Nim updateHandler.
            if messages::consume_pending_prompt(st, tg, user_id, chat_id, &message).await {
                return;
            }
            if let Err(e) = messages::handle_message(st, tg, &message).await {
                on_flow_error(tg, chat_id, &e).await;
            }
        }
        UpdateKind::CallbackQuery(query) => {
            let chat_id = query.message.as_ref().map(|m| match m {
                MaybeInaccessibleMessage::Regular(msg) => msg.chat.id.0,
                MaybeInaccessibleMessage::Inaccessible(inaccessible) => inaccessible.chat.id.0,
            });
            if let Err(e) = callbacks::handle_callback(st, tg, &query).await {
                match chat_id {
                    Some(chat_id) => on_flow_error(tg, chat_id, &e).await,
                    None => tracing::error!("[ERROR] | {e}"),
                }
            }
        }
        _ => {}
    }
}

/// Port of the updateHandler catch: leaving chats the bot cannot talk in, and
/// logging every error to stderr.
async fn on_flow_error(tg: &dyn Tg, chat_id: i64, e: &FlowError) {
    let text = e.to_string();
    if text.contains("have no rights to send a message")
        || text.contains("not enough rights to send text messages to the chat")
    {
        let _ = tg.leave_chat(chat_id).await;
    }
    tracing::error!("[ERROR] | {text}");
}
