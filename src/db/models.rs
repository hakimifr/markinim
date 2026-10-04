//! Row models, 1:1 with `src/database.nim` norm models. Column names and
//! table names are kept identical so the existing `data/markov.db` (and the
//! Python tools that embed its schema) keep working unchanged.

#[derive(Debug, Clone)]
pub struct User {
    pub id: i64,
    pub user_id: i64,
    pub admin: bool,
    pub banned: bool,
    pub consented: bool,
}

impl User {
    pub fn new(user_id: i64) -> Self {
        // Nim defaults: admin/banned/consented all false on insert.
        Self {
            id: 0,
            user_id,
            admin: false,
            banned: false,
            consented: false,
        }
    }

    pub fn mention(&self) -> String {
        format!("[{}](tg://user?id={})", self.user_id, self.user_id)
    }
}

#[derive(Debug, Clone)]
pub struct Chat {
    pub id: i64,
    pub chat_id: i64,
    pub enabled: bool,
    pub percentage: i64,
    pub premium: bool,
    pub banned: bool,
    pub block_links: bool,
    pub block_usernames: bool,
    pub keep_sfw: bool,
    pub markov_disabled: bool,
    pub quotes_disabled: bool,
    pub polls_disabled: bool,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: i64,
    pub name: String,
    pub uuid: String,
    pub chat: Chat,
    pub is_default: bool,
    pub learning_paused: bool,
    pub owoify: i64,
    pub emojipasta: bool,
    pub case_sensitive: bool,
    pub always_reply: bool,
    pub random_replies: bool,
}

impl Session {
    pub fn content_rules(&self) -> crate::filter::ContentRules {
        crate::filter::ContentRules::new(
            self.chat.keep_sfw,
            self.chat.block_links,
            self.chat.block_usernames,
        )
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: i64,
    pub session: i64,
    pub sender: i64,
    pub text: String,
}

/// Tables for the `/count` stats command.
#[derive(Debug, Clone, Copy)]
pub enum CountTable {
    Users,
    Chats,
    Messages,
    Sessions,
}

impl CountTable {
    pub fn table_name(self) -> &'static str {
        match self {
            CountTable::Users => "users",
            CountTable::Chats => "chats",
            CountTable::Messages => "messages",
            CountTable::Sessions => "sessions",
        }
    }
}
