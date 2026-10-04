//! Query layer: an async 1:1 port of the procs in `src/database.nim`.
//!
//! All calls run on the blocking pool through [`Db::call`]; the SQLite
//! connection is behind a std mutex, matching the single `conn` the Nim bot
//! shares. `PRAGMA foreign_keys` stays OFF, like the Nim build: the code
//! deletes rows explicitly and old databases may contain orphans.

use rand::RngCore;
use rusqlite::{Connection, OptionalExtension, params};

use super::models::{Chat, CountTable, Message, Session, User};
use super::schema;

pub fn new_oid() -> String {
    let mut bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[derive(Clone)]
pub struct Db {
    conn: std::sync::Arc<std::sync::Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &std::path::Path) -> Result<Self, rusqlite::Error> {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            // Errors surface on the Connection::open below instead.
            let _ = std::fs::create_dir_all(dir);
        }
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        // The Nim build never enabled foreign keys; old databases contain
        // orphans, so keep them off (the bundled SQLite may default to on).
        let _ = conn.execute_batch("PRAGMA foreign_keys = OFF");
        schema::init(&mut conn)?;
        Ok(Self {
            conn: std::sync::Arc::new(std::sync::Mutex::new(conn)),
        })
    }

    pub async fn call<T, F>(&self, f: F) -> Result<T, rusqlite::Error>
    where
        T: Send + 'static,
        F: FnOnce(&mut Connection) -> Result<T, rusqlite::Error> + Send + 'static,
    {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().expect("db mutex poisoned");
            f(&mut guard)
        })
        .await
        .expect("db task panicked")
    }
}

fn row_to_user(row: &rusqlite::Row<'_>) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get("id")?,
        user_id: row.get("userId")?,
        admin: row.get::<_, i64>("admin")? != 0,
        banned: row.get::<_, i64>("banned")? != 0,
        consented: row.get::<_, i64>("consented")? != 0,
    })
}

const CHAT_COLS: &str = "c.id AS c_id, c.chatId AS c_chatId, c.enabled AS c_enabled, c.percentage AS c_percentage, c.premium AS c_premium, c.banned AS c_banned, c.blockLinks AS c_blockLinks, c.blockUsernames AS c_blockUsernames, c.keepSfw AS c_keepSfw, c.markovDisabled AS c_markovDisabled, c.quotesDisabled AS c_quotesDisabled, c.pollsDisabled AS c_pollsDisabled";

fn row_to_chat(row: &rusqlite::Row<'_>) -> rusqlite::Result<Chat> {
    Ok(Chat {
        id: row.get("c_id")?,
        chat_id: row.get("c_chatId")?,
        enabled: row.get::<_, i64>("c_enabled")? != 0,
        percentage: row.get("c_percentage")?,
        premium: row.get::<_, i64>("c_premium")? != 0,
        banned: row.get::<_, i64>("c_banned")? != 0,
        block_links: row.get::<_, i64>("c_blockLinks")? != 0,
        block_usernames: row.get::<_, i64>("c_blockUsernames")? != 0,
        keep_sfw: row.get::<_, i64>("c_keepSfw")? != 0,
        markov_disabled: row.get::<_, i64>("c_markovDisabled")? != 0,
        quotes_disabled: row.get::<_, i64>("c_quotesDisabled")? != 0,
        polls_disabled: row.get::<_, i64>("c_pollsDisabled")? != 0,
    })
}

const SESSION_COLS: &str = "s.id AS s_id, s.name AS s_name, s.uuid AS s_uuid, s.isDefault AS s_isDefault, s.learningPaused AS s_learningPaused, s.owoify AS s_owoify, s.emojipasta AS s_emojipasta, s.caseSensitive AS s_caseSensitive, s.alwaysReply AS s_alwaysReply, s.randomReplies AS s_randomReplies";

fn row_to_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: row.get("s_id")?,
        name: row.get("s_name")?,
        uuid: row.get("s_uuid")?,
        chat: row_to_chat(row)?,
        is_default: row.get::<_, i64>("s_isDefault")? != 0,
        learning_paused: row.get::<_, i64>("s_learningPaused")? != 0,
        owoify: row.get("s_owoify")?,
        emojipasta: row.get::<_, i64>("s_emojipasta")? != 0,
        case_sensitive: row.get::<_, i64>("s_caseSensitive")? != 0,
        always_reply: row.get::<_, i64>("s_alwaysReply")? != 0,
        random_replies: row.get::<_, i64>("s_randomReplies")? != 0,
    })
}

const SESSION_JOIN: &str = "FROM sessions s JOIN chats c ON s.chat = c.id";

impl Db {
    pub async fn get_user(&self, user_id: i64) -> Result<Option<User>, rusqlite::Error> {
        self.call(move |conn| {
            conn.query_row(
                "SELECT id, userId, admin, banned, consented FROM users WHERE userId = ?1 LIMIT 1",
                params![user_id],
                row_to_user,
            )
            .optional()
        })
        .await
    }

    async fn add_user(&self, user: User) -> Result<User, rusqlite::Error> {
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO users (userId, admin, banned, consented) VALUES (?1, ?2, ?3, ?4)",
                params![
                    user.user_id,
                    user.admin as i64,
                    user.banned as i64,
                    user.consented as i64
                ],
            )?;
            Ok(User {
                id: conn.last_insert_rowid(),
                ..user
            })
        })
        .await
    }

    /// Port of `getOrInsert(user)`: a fresh user starts with `consented = false`
    /// (the explicit insert overrides the column default, like norm does).
    pub async fn get_or_insert_user(&self, user_id: i64) -> Result<User, rusqlite::Error> {
        if let Some(user) = self.get_user(user_id).await? {
            return Ok(user);
        }
        self.add_user(User::new(user_id)).await
    }

    pub async fn update_user(&self, user: &User) -> Result<(), rusqlite::Error> {
        let user = user.clone();
        self.call(move |conn| {
            conn.execute(
                "UPDATE users SET userId = ?2, admin = ?3, banned = ?4, consented = ?5 WHERE id = ?1",
                params![user.id, user.user_id, user.admin as i64, user.banned as i64, user.consented as i64],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_chat(&self, chat_id: i64) -> Result<Option<Chat>, rusqlite::Error> {
        self.call(move |conn| {
            conn.query_row(
                &format!("SELECT {CHAT_COLS} FROM chats c WHERE c.chatId = ?1 LIMIT 1"),
                params![chat_id],
                row_to_chat,
            )
            .optional()
        })
        .await
    }

    /// Port of `addChat`: percentage defaults to 30 for groups (id < 0) and
    /// 100 otherwise; `enabled` is always true; a default session is created
    /// unless `do_not_create_session`.
    pub async fn add_chat(
        &self,
        chat_id: i64,
        do_not_create_session: bool,
    ) -> Result<Chat, rusqlite::Error> {
        let chat = self.call(move |conn| {
            let percentage = if chat_id < 0 { 30 } else { 100 };
            conn.execute(
                "INSERT INTO chats (chatId, enabled, percentage, premium, banned, blockLinks, blockUsernames, keepSfw, markovDisabled, quotesDisabled, pollsDisabled) VALUES (?1, 1, ?2, 0, 0, 0, 0, 0, 0, 0, 0)",
                params![chat_id, percentage],
            )?;
            let id = conn.last_insert_rowid();
            Ok(Chat {
                id,
                chat_id,
                enabled: true,
                percentage,
                premium: false,
                banned: false,
                block_links: false,
                block_usernames: false,
                keep_sfw: false,
                markov_disabled: false,
                quotes_disabled: false,
                polls_disabled: false,
            })
        })
        .await?;
        if !do_not_create_session {
            self.add_session("default".to_owned(), chat.chat_id, true)
                .await?;
        }
        Ok(chat)
    }

    pub async fn get_or_insert_chat(
        &self,
        chat_id: i64,
        do_not_create_session: bool,
    ) -> Result<Chat, rusqlite::Error> {
        if let Some(chat) = self.get_chat(chat_id).await? {
            return Ok(chat);
        }
        self.add_chat(chat_id, do_not_create_session).await
    }

    pub async fn update_chat(&self, chat: &Chat) -> Result<(), rusqlite::Error> {
        let chat = chat.clone();
        self.call(move |conn| {
            conn.execute(
                "UPDATE chats SET chatId = ?2, enabled = ?3, percentage = ?4, premium = ?5, banned = ?6, blockLinks = ?7, blockUsernames = ?8, keepSfw = ?9, markovDisabled = ?10, quotesDisabled = ?11, pollsDisabled = ?12 WHERE id = ?1",
                params![
                    chat.id, chat.chat_id, chat.enabled as i64, chat.percentage, chat.premium as i64,
                    chat.banned as i64, chat.block_links as i64, chat.block_usernames as i64,
                    chat.keep_sfw as i64, chat.markov_disabled as i64, chat.quotes_disabled as i64,
                    chat.polls_disabled as i64,
                ],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_session(&self, uuid: String) -> Result<Option<Session>, rusqlite::Error> {
        self.call(move |conn| {
            conn.query_row(
                &format!(
                    "SELECT {SESSION_COLS}, {CHAT_COLS} {SESSION_JOIN} WHERE s.uuid = ?1 LIMIT 1"
                ),
                params![uuid],
                row_to_session,
            )
            .optional()
        })
        .await
    }

    pub async fn get_sessions(&self, chat_id: i64) -> Result<Vec<Session>, rusqlite::Error> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(&format!(
                "SELECT {SESSION_COLS}, {CHAT_COLS} {SESSION_JOIN} WHERE c.chatId = ?1 ORDER BY s.id"
            ))?;
            let rows = stmt.query_map(params![chat_id], row_to_session)?;
            rows.collect()
        })
        .await
    }

    /// Port of `addSession`: generates a fresh oid. `is_default` is true for
    /// the sessions `addChat`/`getDefaultSession` create, false for the
    /// "add session" flow (which promotes them explicitly afterwards).
    pub async fn add_session(
        &self,
        name: String,
        chat_id: i64,
        is_default: bool,
    ) -> Result<Session, rusqlite::Error> {
        let uuid = new_oid();
        let insert_uuid = uuid.clone();
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO sessions (name, uuid, chat, isDefault, owoify, emojipasta, caseSensitive, alwaysReply, randomReplies, learningPaused) VALUES (?1, ?2, (SELECT id FROM chats WHERE chatId = ?3 LIMIT 1), ?4, 0, 0, 0, 0, 0, 0)",
                params![name, insert_uuid, chat_id, is_default as i64],
            )?;
            Ok(())
        })
        .await?;
        // Port of `addSession`: the freshly created session is re-selected.
        self.get_session_by_uuid(&uuid).await
    }

    pub async fn get_session_by_uuid(&self, uuid: &str) -> Result<Session, rusqlite::Error> {
        self.get_session(uuid.to_owned())
            .await
            .map(|s| s.expect("session just inserted exists"))
    }

    pub async fn get_sessions_count(&self, chat_id: i64) -> Result<i64, rusqlite::Error> {
        self.call(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM sessions WHERE chat = (SELECT id FROM chats WHERE chatId = ?1 LIMIT 1)",
                params![chat_id],
                |row| row.get::<_, i64>(0),
            )
        })
        .await
    }

    /// Port of `getDefaultSession`: the default one if present, else the first
    /// session (promoted to default), else a new "default" session is created.
    pub async fn get_default_session(&self, chat_id: i64) -> Result<Session, rusqlite::Error> {
        let sessions = self.get_sessions(chat_id).await?;
        if let Some(s) = sessions.iter().find(|s| s.is_default) {
            return Ok(s.clone());
        }
        if let Some(first) = sessions.first() {
            let id = first.id;
            self.call(move |conn| {
                conn.execute(
                    "UPDATE sessions SET isDefault = 1 WHERE id = ?1",
                    params![id],
                )?;
                Ok(())
            })
            .await?;
            let mut promoted = first.clone();
            promoted.is_default = true;
            return Ok(promoted);
        }
        self.get_or_insert_chat(chat_id, true).await?;
        self.add_session("default".to_owned(), chat_id, true).await
    }

    /// Port of `setDefaultSession`: flips the default flag, returns all
    /// sessions of the chat with updated flags.
    pub async fn set_default_session(
        &self,
        chat_id: i64,
        uuid: String,
    ) -> Result<Vec<Session>, rusqlite::Error> {
        let mut sessions = self.get_sessions(chat_id).await?;
        for session in sessions.iter_mut() {
            let new_default = session.uuid == uuid;
            if session.is_default != new_default {
                self.call({
                    let id = session.id;
                    let flag = new_default as i64;
                    move |conn| {
                        conn.execute(
                            "UPDATE sessions SET isDefault = ?2 WHERE id = ?1",
                            params![id, flag],
                        )?;
                        Ok(())
                    }
                })
                .await?;
                session.is_default = new_default;
            }
        }
        Ok(sessions)
    }

    pub async fn update_session(&self, session: &Session) -> Result<(), rusqlite::Error> {
        let session = session.clone();
        self.call(move |conn| {
            conn.execute(
                "UPDATE sessions SET name = ?2, uuid = ?3, chat = ?4, isDefault = ?5, owoify = ?6, emojipasta = ?7, caseSensitive = ?8, alwaysReply = ?9, randomReplies = ?10, learningPaused = ?11 WHERE id = ?1",
                params![
                    session.id, session.name, session.uuid, session.chat.id, session.is_default as i64,
                    session.owoify, session.emojipasta as i64, session.case_sensitive as i64,
                    session.always_reply as i64, session.random_replies as i64,
                    session.learning_paused as i64,
                ],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn add_message(
        &self,
        session_id: i64,
        sender_id: i64,
        text: String,
    ) -> Result<(), rusqlite::Error> {
        self.call(move |conn| {
            conn.execute(
                "INSERT INTO messages (session, sender, text) VALUES (?1, ?2, ?3)",
                params![session_id, sender_id, text],
            )?;
            Ok(())
        })
        .await
    }

    pub async fn get_latest_messages(
        &self,
        session_uuid: String,
        chat_id: i64,
        count: i64,
    ) -> Result<Vec<Message>, rusqlite::Error> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT m.id, m.session, m.sender, m.text FROM messages m JOIN sessions s ON m.session = s.id WHERE s.uuid = ?1 AND s.chat = (SELECT id FROM chats WHERE chatId = ?2 LIMIT 1) ORDER BY m.id DESC LIMIT ?3",
            )?;
            let rows = stmt.query_map(params![session_uuid, chat_id, count], |row| {
                Ok(Message {
                    id: row.get(0)?,
                    session: row.get(1)?,
                    sender: row.get(2)?,
                    text: row.get(3)?,
                })
            })?;
            rows.collect()
        })
        .await
    }

    pub async fn get_messages_count(&self, session_uuid: String) -> Result<i64, rusqlite::Error> {
        self.call(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE session = (SELECT id FROM sessions WHERE uuid = ?1 LIMIT 1)",
                params![session_uuid],
                |row| row.get::<_, i64>(0),
            )
        })
        .await
    }

    pub async fn get_user_messages_count(
        &self,
        session_uuid: String,
        user_id: i64,
    ) -> Result<i64, rusqlite::Error> {
        self.call(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE session = (SELECT id FROM sessions WHERE uuid = ?1 LIMIT 1) AND sender = (SELECT id FROM users WHERE userId = ?2 LIMIT 1)",
                params![session_uuid, user_id],
                |row| row.get::<_, i64>(0),
            )
        })
        .await
    }

    /// Port of `deleteMessages`: returns the pre-delete count, removes the
    /// session row and (explicitly, since FKs are off) its messages.
    pub async fn delete_messages(
        &self,
        session_uuid: String,
        chat_id: i64,
    ) -> Result<i64, rusqlite::Error> {
        self.call(move |conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE session = (SELECT id FROM sessions WHERE uuid = ?1 LIMIT 1)",
                params![session_uuid],
                |row| row.get(0),
            )?;
            let session_id: Option<i64> = conn
                .query_row(
                    "SELECT id FROM sessions WHERE uuid = ?1 LIMIT 1",
                    params![session_uuid],
                    |row| row.get(0),
                )
                .optional()?;
            conn.execute(
                "DELETE FROM sessions WHERE uuid = ?1 AND chat = (SELECT id FROM chats WHERE chatId = ?2 LIMIT 1)",
                params![session_uuid, chat_id],
            )?;
            if let Some(session_id) = session_id {
                conn.execute("DELETE FROM messages WHERE session = ?1", params![session_id])?;
            }
            Ok(count)
        })
        .await
    }

    pub async fn delete_from_user_in_chat(
        &self,
        session_uuid: String,
        user_id: i64,
    ) -> Result<i64, rusqlite::Error> {
        self.call(move |conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE session = (SELECT id FROM sessions WHERE uuid = ?1 LIMIT 1) AND sender = (SELECT id FROM users WHERE userId = ?2 LIMIT 1)",
                params![session_uuid, user_id],
                |row| row.get(0),
            )?;
            conn.execute(
                "DELETE FROM messages WHERE session = (SELECT id FROM sessions WHERE uuid = ?1 LIMIT 1) AND sender = (SELECT id FROM users WHERE userId = ?2 LIMIT 1)",
                params![session_uuid, user_id],
            )?;
            Ok(count)
        })
        .await
    }

    pub async fn get_total_user_messages_count(
        &self,
        user_id: i64,
    ) -> Result<i64, rusqlite::Error> {
        self.call(move |conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM messages WHERE sender = (SELECT id FROM users WHERE userId = ?1 LIMIT 1)",
                params![user_id],
                |row| row.get::<_, i64>(0),
            )
        })
        .await
    }

    /// Port of `deleteAllMessagesFromUser`: returns the deleted count.
    pub async fn delete_all_messages_from_user(
        &self,
        user_id: i64,
    ) -> Result<i64, rusqlite::Error> {
        let count = self.get_total_user_messages_count(user_id).await?;
        self.call(move |conn| {
            conn.execute(
                "DELETE FROM messages WHERE sender = (SELECT id FROM users WHERE userId = ?1 LIMIT 1)",
                params![user_id],
            )?;
            Ok(())
        })
        .await?;
        Ok(count)
    }

    pub async fn get_bot_admins(&self) -> Result<Vec<User>, rusqlite::Error> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, userId, admin, banned, consented FROM users WHERE admin = 1",
            )?;
            let rows = stmt.query_map([], row_to_user)?;
            rows.collect()
        })
        .await
    }

    pub async fn get_banned_users(&self) -> Result<Vec<User>, rusqlite::Error> {
        self.call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT id, userId, admin, banned, consented FROM users WHERE banned = 1",
            )?;
            let rows = stmt.query_map([], row_to_user)?;
            rows.collect()
        })
        .await
    }

    /// Port of `setAdmin` (getOrInsert then update).
    pub async fn set_admin(&self, user_id: i64, admin: bool) -> Result<User, rusqlite::Error> {
        let mut user = self.get_or_insert_user(user_id).await?;
        user.admin = admin;
        self.update_user(&user).await?;
        Ok(user)
    }

    pub async fn set_banned_user(
        &self,
        user_id: i64,
        banned: bool,
    ) -> Result<User, rusqlite::Error> {
        let mut user = self.get_or_insert_user(user_id).await?;
        user.banned = banned;
        self.update_user(&user).await?;
        Ok(user)
    }

    pub async fn set_banned_chat(
        &self,
        chat_id: i64,
        banned: bool,
    ) -> Result<Chat, rusqlite::Error> {
        let mut chat = self.get_or_insert_chat(chat_id, false).await?;
        chat.banned = banned;
        self.update_chat(&chat).await?;
        Ok(chat)
    }

    pub async fn set_enabled(&self, chat_id: i64, enabled: bool) -> Result<Chat, rusqlite::Error> {
        let mut chat = self.get_or_insert_chat(chat_id, false).await?;
        chat.enabled = enabled;
        self.update_chat(&chat).await?;
        Ok(chat)
    }

    pub async fn get_count(&self, table: CountTable) -> Result<i64, rusqlite::Error> {
        self.call(move |conn| {
            conn.query_row(
                &format!("SELECT COUNT(*) FROM {}", table.table_name()),
                [],
                |row| row.get::<_, i64>(0),
            )
        })
        .await
    }
}
