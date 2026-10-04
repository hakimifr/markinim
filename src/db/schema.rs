//! Schema creation, byte-compatible with the tables norm creates (as embedded
//! in `tools/gdpr_export.py` and `tools/data_removal.py`), followed by the
//! same ten `ALTER TABLE` statements `initDatabase` runs, tolerating
//! "duplicate column" errors so both old and new databases open fine.

use rusqlite::Connection;

const CREATE_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS "chats"(chatId INTEGER NOT NULL UNIQUE, enabled INTEGER NOT NULL, percentage INTEGER NOT NULL, premium INTEGER NOT NULL, banned INTEGER NOT NULL, blockLinks INTEGER NOT NULL, blockUsernames INTEGER NOT NULL, keepSfw INTEGER NOT NULL, markovDisabled INTEGER NOT NULL, quotesDisabled INTEGER NOT NULL, id INTEGER NOT NULL PRIMARY KEY, pollsDisabled INTEGER NOT NULL DEFAULT 1);
CREATE TABLE IF NOT EXISTS "messages"(session INTEGER NOT NULL, sender INTEGER NOT NULL, text TEXT NOT NULL, id INTEGER NOT NULL PRIMARY KEY, FOREIGN KEY(session) REFERENCES "sessions"(id) ON DELETE CASCADE, FOREIGN KEY(sender) REFERENCES "users"(id));
CREATE TABLE IF NOT EXISTS "sessions"(name TEXT NOT NULL, uuid TEXT NOT NULL UNIQUE, chat INTEGER NOT NULL, isDefault INTEGER NOT NULL, owoify INTEGER NOT NULL, emojipasta INTEGER NOT NULL, caseSensitive INTEGER NOT NULL, alwaysReply INTEGER NOT NULL, id INTEGER NOT NULL PRIMARY KEY, randomReplies INTEGER NOT NULL DEFAULT 0, learningPaused INTEGER NOT NULL DEFAULT 0, FOREIGN KEY(chat) REFERENCES "chats"(id) ON DELETE CASCADE);
CREATE TABLE IF NOT EXISTS "users"(userId INTEGER NOT NULL UNIQUE, admin INTEGER NOT NULL, banned INTEGER NOT NULL, id INTEGER NOT NULL PRIMARY KEY, consented INTEGER NOT NULL DEFAULT 0);
"#;

const ALTERS: [&str; 10] = [
    "ALTER TABLE sessions ADD owoify INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE sessions ADD emojipasta INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE sessions ADD caseSensitive INTEGER NOT NULL DEFAULT 1",
    "ALTER TABLE sessions ADD alwaysReply INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE sessions ADD randomReplies INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE chats ADD markovDisabled INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE chats ADD quotesDisabled INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE chats ADD pollsDisabled INTEGER NOT NULL DEFAULT 1",
    "ALTER TABLE users ADD consented INTEGER NOT NULL DEFAULT 1",
    "ALTER TABLE sessions ADD learningPaused INTEGER NOT NULL DEFAULT 0",
];

pub fn init(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch(CREATE_TABLES)?;
    for alter in ALTERS {
        if let Err(e) = conn.execute_batch(alter)
            && !e.to_string().contains("duplicate column")
        {
            return Err(e);
        }
    }
    Ok(())
}
