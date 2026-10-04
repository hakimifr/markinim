//! The narrow Telegram surface the handlers use, as an object-safe trait so
//! command logic can be tested against a recording fake (the Nim code is
//! coupled to telebot and untestable).

use async_trait::async_trait;
use teloxide::RequestError;
use teloxide::prelude::*;
use teloxide::types::{
    CallbackQueryId, ChatMemberKind, InlineKeyboardMarkup, InputFile, LinkPreviewOptions,
    MessageId, ParseMode, ReplyParameters, ThreadId,
};

fn no_preview() -> LinkPreviewOptions {
    LinkPreviewOptions {
        is_disabled: true,
        url: None,
        prefer_small_media: false,
        prefer_large_media: false,
        show_above_text: false,
    }
}

fn thread_param(t: i64) -> ThreadId {
    ThreadId(MessageId(t as i32))
}

#[derive(Debug, Clone)]
pub struct TgError(pub String);

impl std::fmt::Display for TgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TgError {}

impl From<RequestError> for TgError {
    fn from(e: RequestError) -> Self {
        // Keep the raw API description in the string: the update handler
        // matches on "no rights to send" to decide whether to leave a chat.
        TgError(e.to_string())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ParseModeKind {
    Markdown,
    Html,
}

#[derive(Default)]
pub struct SendParams<'k> {
    pub thread: Option<i64>,
    pub reply_to: Option<i64>,
    pub parse_mode: Option<ParseModeKind>,
    pub disable_preview: bool,
    pub keyboard: Option<&'k InlineKeyboardMarkup>,
}

#[derive(Default)]
pub struct EditParams<'k> {
    pub parse_mode: Option<ParseModeKind>,
    pub keyboard: Option<&'k InlineKeyboardMarkup>,
}

#[async_trait]
pub trait Tg: Send + Sync {
    async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        params: SendParams<'_>,
    ) -> Result<i64, TgError>;
    async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
        params: EditParams<'_>,
    ) -> Result<(), TgError>;
    async fn answer_callback(
        &self,
        query_id: &str,
        text: Option<&str>,
        show_alert: bool,
    ) -> Result<(), TgError>;
    async fn send_photo(&self, chat_id: i64, png: Vec<u8>) -> Result<(), TgError>;
    async fn send_poll(
        &self,
        chat_id: i64,
        question: &str,
        options: Vec<String>,
        thread: Option<i64>,
        is_anonymous: bool,
    ) -> Result<(), TgError>;
    async fn delete_message(&self, chat_id: i64, message_id: i64) -> Result<(), TgError>;
    async fn leave_chat(&self, chat_id: i64) -> Result<(), TgError>;
    async fn chat_member_is_admin(&self, chat_id: i64, user_id: i64) -> Result<bool, TgError>;
}

/// Production implementation backed by teloxide.
pub struct TeloxideBot(pub Bot);

#[async_trait]
impl Tg for TeloxideBot {
    async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        params: SendParams<'_>,
    ) -> Result<i64, TgError> {
        let mut req = self.0.send_message(ChatId(chat_id), text);
        if let Some(t) = params.thread {
            req = req.message_thread_id(thread_param(t));
        }
        if let Some(reply) = params.reply_to {
            req = req.reply_parameters(ReplyParameters {
                message_id: MessageId(reply as i32),
                ..Default::default()
            });
        }
        if let Some(mode) = params.parse_mode {
            req = req.parse_mode(to_parse_mode(mode));
        }
        if params.disable_preview {
            req = req.link_preview_options(no_preview());
        }
        if let Some(kb) = params.keyboard {
            req = req.reply_markup(kb.clone());
        }
        Ok(req.send().await?.id.0 as i64)
    }

    async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
        params: EditParams<'_>,
    ) -> Result<(), TgError> {
        let mut req = self
            .0
            .edit_message_text(ChatId(chat_id), MessageId(message_id as i32), text);
        if let Some(mode) = params.parse_mode {
            req = req.parse_mode(to_parse_mode(mode));
        }
        if let Some(kb) = params.keyboard {
            req = req.reply_markup(kb.clone());
        }
        req.send().await?;
        Ok(())
    }

    async fn answer_callback(
        &self,
        query_id: &str,
        text: Option<&str>,
        show_alert: bool,
    ) -> Result<(), TgError> {
        let mut req = self
            .0
            .answer_callback_query(CallbackQueryId(query_id.to_owned()));
        if let Some(text) = text {
            req = req.text(text.to_owned());
        }
        if show_alert {
            req = req.show_alert(true);
        }
        req.send().await?;
        Ok(())
    }

    async fn send_photo(&self, chat_id: i64, png: Vec<u8>) -> Result<(), TgError> {
        self.0
            .send_photo(ChatId(chat_id), InputFile::memory(png))
            .send()
            .await?;
        Ok(())
    }

    async fn send_poll(
        &self,
        chat_id: i64,
        question: &str,
        options: Vec<String>,
        thread: Option<i64>,
        is_anonymous: bool,
    ) -> Result<(), TgError> {
        let options: Vec<teloxide::types::InputPollOption> = options
            .into_iter()
            .map(|text| teloxide::types::InputPollOption {
                text,
                formatting: None,
            })
            .collect();
        let mut req = self
            .0
            .send_poll(ChatId(chat_id), question, options)
            .is_anonymous(is_anonymous);
        if let Some(t) = thread {
            req = req.message_thread_id(thread_param(t));
        }
        req.send().await?;
        Ok(())
    }

    async fn delete_message(&self, chat_id: i64, message_id: i64) -> Result<(), TgError> {
        self.0
            .delete_message(ChatId(chat_id), MessageId(message_id as i32))
            .send()
            .await?;
        Ok(())
    }

    async fn leave_chat(&self, chat_id: i64) -> Result<(), TgError> {
        self.0.leave_chat(ChatId(chat_id)).send().await?;
        Ok(())
    }

    async fn chat_member_is_admin(&self, chat_id: i64, user_id: i64) -> Result<bool, TgError> {
        let member = self
            .0
            .get_chat_member(ChatId(chat_id), UserId(user_id as u64))
            .send()
            .await?;
        Ok(matches!(
            member.kind,
            ChatMemberKind::Owner(_) | ChatMemberKind::Administrator(_)
        ))
    }
}

#[allow(deprecated)]
fn to_parse_mode(mode: ParseModeKind) -> ParseMode {
    match mode {
        ParseModeKind::Markdown => ParseMode::Markdown,
        ParseModeKind::Html => ParseMode::Html,
    }
}
