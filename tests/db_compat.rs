//! Database compatibility tests: the Rust layer must open and extend
//! databases created by the norm/Nim schema (and by the Python tools'
//! documented schema), without changing the schema the tools embed.

use std::path::PathBuf;

use markinim::db::{CountTable, Db};
use rusqlite::Connection;

struct TempDb(PathBuf);

impl TempDb {
    fn fresh(name: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!("markinim-tests-{}-{name}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }

    fn from_fixture(name: &str) -> Self {
        let temp = Self::fresh(name);
        let conn = Connection::open(&temp.0).unwrap();
        conn.execute_batch(include_str!("fixtures/schema.sql"))
            .unwrap();
        drop(conn);
        temp
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn tools_schema_queries(conn: &Connection) {
    // The exact queries the Python tools rely on (see tools/gdpr_export.py).
    for sql in [
        "SELECT chatId, enabled, percentage, premium, banned, blockLinks, blockUsernames, keepSfw, markovDisabled, quotesDisabled, pollsDisabled FROM chats LIMIT 0",
        "SELECT name, uuid, chat, isDefault, owoify, emojipasta, caseSensitive, alwaysReply, randomReplies, learningPaused FROM sessions LIMIT 0",
        "SELECT session, sender, text FROM messages LIMIT 0",
        "SELECT userId, admin, banned, consented FROM users LIMIT 0",
    ] {
        conn.prepare(sql).expect(sql);
    }
}

#[tokio::test]
async fn fresh_database_is_created_and_idempotent() {
    let temp = TempDb::fresh("fresh");
    let db = Db::open(&temp.0).unwrap();
    // Second open re-runs CREATE TABLE + all ALTERs without failing.
    let db2 = Db::open(&temp.0).unwrap();
    assert_eq!(db.get_count(CountTable::Users).await.unwrap(), 0);
    assert_eq!(db2.get_count(CountTable::Messages).await.unwrap(), 0);
}

#[tokio::test]
async fn opens_existing_norm_schema_database() {
    let temp = TempDb::from_fixture("norm-schema");
    let db = Db::open(&temp.0).unwrap();

    let user = db.get_or_insert_user(12345).await.unwrap();
    assert_eq!(user.user_id, 12345);
    assert!(!user.consented); // new users start unconsented

    let chat = db.get_or_insert_chat(-100200, false).await.unwrap();
    assert!(chat.enabled);
    assert_eq!(chat.percentage, 30); // groups default to 30%

    let private = db.get_or_insert_chat(12345, false).await.unwrap();
    assert_eq!(private.percentage, 100); // private chats to 100%

    db.add_message(chat.id, user.id, "hello world".to_owned())
        .await
        .unwrap();
    assert_eq!(db.get_total_user_messages_count(12345).await.unwrap(), 1);
    assert_eq!(db.get_count(CountTable::Messages).await.unwrap(), 1);

    let conn = Connection::open(&temp.0).unwrap();
    tools_schema_queries(&conn);
}

#[tokio::test]
async fn orphan_rows_are_tolerated_like_the_nim_build() {
    // foreign_keys stays OFF (the Nim build never enabled it and old
    // databases contain orphans): inserting a dangling message must work.
    let temp = TempDb::from_fixture("orphans");
    let db = Db::open(&temp.0).unwrap();
    db.add_message(9999, 8888, "orphan".to_owned())
        .await
        .unwrap();
    assert_eq!(db.get_count(CountTable::Messages).await.unwrap(), 1);
}

#[tokio::test]
async fn default_sessions_are_created_once_and_promoted() {
    let temp = TempDb::fresh("defaults");
    let db = Db::open(&temp.0).unwrap();

    let first = db.get_default_session(-77).await.unwrap();
    assert_eq!(first.name, "default");
    assert!(first.is_default);
    let second = db.get_default_session(-77).await.unwrap();
    assert_eq!(
        first.uuid, second.uuid,
        "must not create a new default session"
    );

    let extra = db
        .add_session("second".to_owned(), -77, false)
        .await
        .unwrap();
    let flipped = db
        .set_default_session(-77, extra.uuid.clone())
        .await
        .unwrap();
    assert_eq!(flipped.iter().filter(|s| s.is_default).count(), 1);
    assert!(
        flipped
            .iter()
            .find(|s| s.uuid == extra.uuid)
            .unwrap()
            .is_default
    );
}

#[tokio::test]
async fn latest_messages_and_delete_counts() {
    let temp = TempDb::fresh("deletes");
    let db = Db::open(&temp.0).unwrap();
    let chat = db.get_or_insert_chat(-500, false).await.unwrap();
    let alice = db.get_or_insert_user(1).await.unwrap();
    let bob = db.get_or_insert_user(2).await.unwrap();

    for text in ["one", "two", "three", "four"] {
        db.add_message(chat.id, alice.id, text.to_owned())
            .await
            .unwrap();
    }
    db.add_message(chat.id, bob.id, "bob says hi".to_owned())
        .await
        .unwrap();

    let session = db.get_default_session(-500).await.unwrap();
    let latest = db
        .get_latest_messages(session.uuid.clone(), -500, 3)
        .await
        .unwrap();
    assert_eq!(
        latest.iter().map(|m| m.text.as_str()).collect::<Vec<_>>(),
        ["bob says hi", "four", "three"]
    );

    assert_eq!(
        db.get_messages_count(session.uuid.clone()).await.unwrap(),
        5
    );
    assert_eq!(
        db.get_user_messages_count(session.uuid.clone(), 1)
            .await
            .unwrap(),
        4
    );

    // /deletefrom: only alice's messages in this session
    let deleted = db
        .delete_from_user_in_chat(session.uuid.clone(), 1)
        .await
        .unwrap();
    assert_eq!(deleted, 4);
    assert_eq!(
        db.get_messages_count(session.uuid.clone()).await.unwrap(),
        1
    );

    // /delete: removes the whole session and its messages, returns the count
    db.add_message(chat.id, bob.id, "another".to_owned())
        .await
        .unwrap();
    let deleted = db
        .delete_messages(session.uuid.clone(), -500)
        .await
        .unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(
        db.get_messages_count(session.uuid.clone()).await.unwrap(),
        0
    );

    // /deleteme (bob's earlier messages were already removed by /delete)
    db.add_message(chat.id, bob.id, "more".to_owned())
        .await
        .unwrap();
    let deleted = db.delete_all_messages_from_user(2).await.unwrap();
    assert_eq!(deleted, 1);
    assert_eq!(db.get_total_user_messages_count(2).await.unwrap(), 0);
}

#[tokio::test]
async fn admin_and_ban_flags_roundtrip() {
    let temp = TempDb::fresh("flags");
    let db = Db::open(&temp.0).unwrap();

    let admin = db.set_admin(55, true).await.unwrap();
    assert!(admin.admin);
    let admins = db.get_bot_admins().await.unwrap();
    assert_eq!(admins.len(), 1);

    let banned_user = db.set_banned_user(66, true).await.unwrap();
    assert!(banned_user.banned);
    let banned_chat = db.set_banned_chat(-99, true).await.unwrap();
    assert!(banned_chat.banned);
    let banned = db.get_banned_users().await.unwrap();
    assert_eq!(banned.len(), 1);
}
