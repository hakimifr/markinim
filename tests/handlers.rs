//! Handler behavior tests: the same flows as the Nim updateHandler, driven
//! through `handle_update` against a recording fake Telegram client.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::json;
use teloxide::types::{InlineKeyboardMarkup, Update};

use markinim::config::Config;
use markinim::db::Db;
use markinim::handlers::{SendParams, Tg, TgError, handle_update};
use markinim::state::AppState;

type Sent = (i64, String, Option<i64>, Option<InlineKeyboardMarkup>);
type Edit = (i64, i64, String, Option<InlineKeyboardMarkup>);

#[derive(Default)]
struct FakeTg {
    sent: Mutex<Vec<Sent>>,
    edited: Mutex<Vec<Edit>>,
    answers: Mutex<Vec<(String, Option<String>, bool)>>,
    photos: Mutex<Vec<(i64, Vec<u8>)>>,
    polls: Mutex<Vec<(i64, String, Vec<String>)>>,
    deleted: Mutex<Vec<(i64, i64)>>,
    left: Mutex<Vec<i64>>,
    next_message_id: Mutex<i64>,
    admin_status: Mutex<std::collections::HashMap<(i64, i64), bool>>,
}

impl FakeTg {
    fn make_admin(&self, chat_id: i64, user_id: i64) {
        self.admin_status
            .lock()
            .unwrap()
            .insert((chat_id, user_id), true);
    }

    fn sent_texts(&self) -> Vec<String> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|(_, text, _, _)| text.clone())
            .collect()
    }
}

#[async_trait]
impl Tg for FakeTg {
    async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        params: SendParams<'_>,
    ) -> Result<i64, TgError> {
        let id = {
            let mut n = self.next_message_id.lock().unwrap();
            *n += 1;
            *n
        };
        self.sent.lock().unwrap().push((
            chat_id,
            text.to_owned(),
            params.reply_to,
            params.keyboard.cloned(),
        ));
        Ok(id)
    }

    async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
        params: markinim::handlers::EditParams<'_>,
    ) -> Result<(), TgError> {
        self.edited.lock().unwrap().push((
            chat_id,
            message_id,
            text.to_owned(),
            params.keyboard.cloned(),
        ));
        Ok(())
    }

    async fn answer_callback(
        &self,
        query_id: &str,
        text: Option<&str>,
        show_alert: bool,
    ) -> Result<(), TgError> {
        self.answers.lock().unwrap().push((
            query_id.to_owned(),
            text.map(str::to_owned),
            show_alert,
        ));
        Ok(())
    }

    async fn send_photo(&self, chat_id: i64, png: Vec<u8>) -> Result<(), TgError> {
        self.photos.lock().unwrap().push((chat_id, png));
        Ok(())
    }

    async fn send_poll(
        &self,
        chat_id: i64,
        question: &str,
        options: Vec<String>,
        _thread: Option<i64>,
        _is_anonymous: bool,
    ) -> Result<(), TgError> {
        self.polls
            .lock()
            .unwrap()
            .push((chat_id, question.to_owned(), options));
        Ok(())
    }

    async fn delete_message(&self, chat_id: i64, message_id: i64) -> Result<(), TgError> {
        self.deleted.lock().unwrap().push((chat_id, message_id));
        Ok(())
    }

    async fn leave_chat(&self, chat_id: i64) -> Result<(), TgError> {
        self.left.lock().unwrap().push(chat_id);
        Ok(())
    }

    async fn chat_member_is_admin(&self, chat_id: i64, user_id: i64) -> Result<bool, TgError> {
        Ok(self
            .admin_status
            .lock()
            .unwrap()
            .get(&(chat_id, user_id))
            .copied()
            .unwrap_or(false))
    }
}

struct TempDb(PathBuf);

impl TempDb {
    fn fresh(name: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "markinim-handlers-{}-{name}.db",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        Self(path)
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn setup(name: &str) -> (Arc<AppState>, FakeTg, TempDb) {
    let temp = TempDb::fresh(name);
    let db = Db::open(&temp.0).unwrap();
    let config = Config {
        token: "x".to_owned(),
        admin: None,
        logging: false,
        keep_last: 1500,
    };
    let assets = markinim::quote::QuoteAssets::load().unwrap();
    let st = Arc::new(AppState::new(db, config, assets));
    st.set_bot_info("CorrectBot".to_owned(), 4242);
    (st, FakeTg::default(), temp)
}

// NB: deserialize through a JSON *string* — `from_value` trips over serde's
// nested `#[serde(flatten)]` buffering (UpdateKind inside Update), which the
// real wire deserializer (from_str) doesn't hit.
fn parse_update(value: serde_json::Value) -> Update {
    serde_json::from_str(&value.to_string()).unwrap()
}

fn update_message(id: u32, user_id: i64, chat_json: serde_json::Value, text: &str) -> Update {
    // NB: the update-kind key must come first — UpdateKind's deserializer
    // inspects the first key of the map.
    let update = json!({
        "message": {
            "message_id": 10 + id,
            "date": 1_700_000_000i64,
            "chat": chat_json,
            "from": {"id": user_id, "is_bot": false, "first_name": "U"},
            "text": text,
        },
        "update_id": id,
    });
    parse_update(update)
}

fn update_message_with_reply(
    id: u32,
    user_id: i64,
    chat_json: serde_json::Value,
    text: &str,
    reply: serde_json::Value,
) -> Update {
    let mut message = json!({
        "message_id": 10 + id,
        "date": 1_700_000_000i64,
        "chat": chat_json,
        "from": {"id": user_id, "is_bot": false, "first_name": "U"},
        "text": text,
    });
    message["reply_to_message"] = reply;
    parse_update(json!({"message": message, "update_id": id}))
}

fn update_callback(
    id: u32,
    query_id: &str,
    user_id: i64,
    data: &str,
    attached: serde_json::Value,
) -> Update {
    parse_update(json!({
        "callback_query": {
            "id": query_id,
            "from": {"id": user_id, "is_bot": false, "first_name": "U"},
            "chat_instance": "0",
            "data": data,
            "message": attached,
        },
        "update_id": id,
    }))
}

fn group(chat_id: i64) -> serde_json::Value {
    json!({"id": chat_id, "title": "G", "type": "supergroup"})
}

fn private(chat_id: i64) -> serde_json::Value {
    json!({"id": chat_id, "type": "private", "first_name": "U"})
}

fn attached_message(chat_id: i64) -> serde_json::Value {
    json!({
        "message_id": 500,
        "date": 1_700_000_000i64,
        "chat": group(chat_id),
        "from": {"id": 4242, "is_bot": true, "first_name": "Bot"},
        "text": "settings placeholder",
    })
}

async fn make_consented(st: &AppState, user_id: i64) {
    let mut user = st.db.get_or_insert_user(user_id).await.unwrap();
    user.consented = true;
    st.db.update_user(&user).await.unwrap();
}

#[tokio::test]
async fn group_messages_are_learned_and_replied_to() {
    let (st, tg, _db) = setup("learn").await;
    make_consented(&st, 5).await;

    let mut chat = st.db.get_or_insert_chat(-100, false).await.unwrap();
    chat.percentage = 100;
    chat.quotes_disabled = true; // keep the random quote path out of the test
    st.db.update_chat(&chat).await.unwrap();

    handle_update(&st, &tg, update_message(1, 5, group(-100), "hello")).await;
    assert_eq!(st.db.get_total_user_messages_count(5).await.unwrap(), 1);
    let replies = tg.sent_texts();
    assert!(!replies.is_empty(), "percentage 100 must trigger a reply");

    handle_update(&st, &tg, update_message(2, 5, group(-100), "world")).await;
    assert_eq!(st.db.get_total_user_messages_count(5).await.unwrap(), 2);
}

#[tokio::test]
async fn unconsented_private_users_get_the_consent_notice() {
    let (st, tg, _db) = setup("consent").await;
    handle_update(&st, &tg, update_message(1, 5, private(5), "hello there")).await;
    let texts = tg.sent_texts();
    assert_eq!(texts.len(), 1);
    assert!(texts[0].contains("You have not given consent"), "{texts:?}");
    assert!(texts[0].contains("https://t.me/CorrectBot?start=consent"));
    // nothing was learned
    assert_eq!(st.db.get_total_user_messages_count(5).await.unwrap(), 0);
}

#[tokio::test]
async fn banned_users_and_chats_are_skipped() {
    let (st, tg, _db) = setup("banned").await;
    make_consented(&st, 9).await;
    st.banned.write().unwrap().insert(9);

    handle_update(&st, &tg, update_message(1, 9, group(-100), "hello")).await;
    assert_eq!(st.db.get_total_user_messages_count(9).await.unwrap(), 0);
    assert!(tg.sent_texts().is_empty());

    // a banned CHAT only blocks non-admin senders (Nim precedence)
    let mut chat = st.db.get_or_insert_chat(-200, false).await.unwrap();
    chat.percentage = 0;
    st.db.update_chat(&chat).await.unwrap();
    make_consented(&st, 10).await;
    st.banned.write().unwrap().insert(-200);

    let admin = st.db.get_or_insert_user(1).await.unwrap();
    st.admins.write().unwrap().insert(admin.user_id);
    make_consented(&st, 1).await; // admins need consent to learn too
    handle_update(&st, &tg, update_message(2, 1, group(-200), "admin hello")).await;
    assert_eq!(
        st.db.get_total_user_messages_count(1).await.unwrap(),
        1,
        "admins bypass chat bans"
    );

    handle_update(&st, &tg, update_message(3, 10, group(-200), "user hello")).await;
    assert_eq!(
        st.db.get_total_user_messages_count(10).await.unwrap(),
        0,
        "non-admins are blocked"
    );
}

#[tokio::test]
async fn flood_gate_drops_the_seventh_command() {
    let (st, tg, _db) = setup("flood").await;
    for i in 1..=7 {
        handle_update(&st, &tg, update_message(i, 7, group(-300), "/count")).await;
    }
    // 6 commands got the UNALLOWED reply, the 7th was dropped silently
    assert_eq!(tg.sent_texts().len(), 6);
    assert!(tg.sent_texts()[0].contains("You are not allowed"));
}

#[tokio::test]
async fn commands_for_other_bots_are_ignored() {
    let (st, tg, _db) = setup("otherbot").await;
    make_consented(&st, 5).await;
    handle_update(
        &st,
        &tg,
        update_message(1, 5, group(-100), "/markov@SomeoneElse"),
    )
    .await;
    assert!(tg.sent_texts().is_empty());

    handle_update(
        &st,
        &tg,
        update_message(2, 5, group(-100), "/markov@correctbot"),
    )
    .await;
    assert_eq!(
        tg.sent_texts(),
        vec!["Not enough data to generate a sentence"]
    );
}

#[tokio::test]
async fn markov_without_data_replies_not_enough() {
    let (st, tg, _db) = setup("empty-markov").await;
    make_consented(&st, 5).await;
    handle_update(&st, &tg, update_message(1, 5, group(-100), "/markov hello")).await;
    assert_eq!(
        tg.sent_texts(),
        vec!["Not enough data to generate a sentence"]
    );
}

#[tokio::test]
async fn delete_confirm_removes_the_session_data() {
    let (st, tg, _db) = setup("delete").await;
    tg.make_admin(-400, 5);
    make_consented(&st, 5).await;
    let chat = st.db.get_or_insert_chat(-400, false).await.unwrap();
    let user = st.db.get_or_insert_user(5).await.unwrap();
    for text in ["a", "b", "c"] {
        st.db
            .add_message(chat.id, user.id, text.to_owned())
            .await
            .unwrap();
    }

    // without confirm: only the prompt
    handle_update(&st, &tg, update_message(1, 5, group(-400), "/delete")).await;
    assert!(tg.sent_texts()[0].contains("send `/delete confirm`"));

    handle_update(
        &st,
        &tg,
        update_message(2, 5, group(-400), "/delete confirm"),
    )
    .await;
    assert_eq!(
        st.db
            .get_count(markinim::db::CountTable::Messages)
            .await
            .unwrap(),
        0
    );
    let edits = tg.edited.lock().unwrap();
    let completion = edits
        .iter()
        .map(|(_, _, text, _)| text)
        .next_back()
        .unwrap();
    assert!(
        completion.contains("Successfully deleted `3` messages"),
        "{completion:?}"
    );
}

#[tokio::test]
async fn addsession_prompt_consumes_the_next_message() {
    let (st, tg, _db) = setup("addsession").await;
    tg.make_admin(-500, 5);
    st.db.get_or_insert_chat(-500, false).await.unwrap();

    handle_update(
        &st,
        &tg,
        update_callback(1, "cb1", 5, "addsession_-500", attached_message(-500)),
    )
    .await;
    assert!(
        st.pending_prompts.lock().unwrap().contains_key(&(5, -500)),
        "the prompt must be registered"
    );
    let first_edit = tg.edited.lock().unwrap()[0].2.clone();
    assert!(
        first_edit.contains("Send me the name for the new session"),
        "{first_edit:?}"
    );
    drop(first_edit);

    // the next message from the same user in the same chat becomes the name
    handle_update(&st, &tg, update_message(2, 5, group(-500), "my session")).await;
    let sessions = st.db.get_sessions(-500).await.unwrap();
    assert!(
        sessions.iter().any(|s| s.name == "my session"),
        "{sessions:?}"
    );
    assert!(st.pending_prompts.lock().unwrap().is_empty());
}

#[tokio::test]
async fn long_session_names_are_rejected() {
    let (st, tg, _db) = setup("longname").await;
    tg.make_admin(-500, 5);
    st.db.get_or_insert_chat(-500, false).await.unwrap();

    handle_update(
        &st,
        &tg,
        update_callback(1, "cb1", 5, "addsession_-500", attached_message(-500)),
    )
    .await;
    handle_update(
        &st,
        &tg,
        update_message(2, 5, group(-500), "this name is way too long"),
    )
    .await;
    let sessions = st.db.get_sessions(-500).await.unwrap();
    assert_eq!(sessions.len(), 1, "only the default session must exist");
    assert_eq!(sessions[0].name, "default");
    let edits = tg.edited.lock().unwrap();
    assert!(
        edits
            .iter()
            .any(|(_, _, text, _)| text.contains("longer than `16` characters")),
        "{edits:?}"
    );
}

#[tokio::test]
async fn settings_toggles_flip_their_columns() {
    let (st, tg, _db) = setup("toggles").await;
    tg.make_admin(-600, 5);
    let session = st.db.get_or_insert_chat(-600, false).await.unwrap();
    let _ = session;
    let default = st.db.get_default_session(-600).await.unwrap();
    assert!(!default.case_sensitive); // norm inserts explicit false on creation
    assert!(!default.chat.keep_sfw);
    assert_eq!(default.owoify, 0);

    handle_update(
        &st,
        &tg,
        update_callback(1, "cb1", 5, "sfw_-600", attached_message(-600)),
    )
    .await;
    let session = st.db.get_default_session(-600).await.unwrap();
    assert!(
        session.chat.keep_sfw,
        "sfw toggle must flip the chat column"
    );
    // sfw answers with its special toast, exactly once
    assert_eq!(tg.answers.lock().unwrap().len(), 1);

    handle_update(
        &st,
        &tg,
        update_callback(
            2,
            "cb2",
            5,
            &format!("owoify_-600_{}", default.uuid),
            attached_message(-600),
        ),
    )
    .await;
    let session = st.db.get_default_session(-600).await.unwrap();
    assert_eq!(session.owoify, 1, "owoify must cycle 0 -> 1");

    handle_update(
        &st,
        &tg,
        update_callback(
            3,
            "cb3",
            5,
            &format!("casesensivity_-600_{}", default.uuid),
            attached_message(-600),
        ),
    )
    .await;
    let session = st.db.get_default_session(-600).await.unwrap();
    assert!(
        session.case_sensitive,
        "casesensivity must flip the session column"
    );
}

/// The callback_data strings are a frozen contract with the deployed bot:
/// inline keyboards the old Nim build sent must keep working.
#[tokio::test]
async fn callback_data_strings_stay_identical() {
    let (st, _tg, _db) = setup("callback-contract").await;
    st.db.get_or_insert_chat(-700, false).await.unwrap();
    let session = st.db.get_default_session(-700).await.unwrap();
    let kb = markinim::keyboards::settings_keyboard(&session);
    let serialized = serde_json::to_string(&kb).unwrap();

    for expected in [
        "usernames_-700",
        "links_-700",
        "sfw_-700",
        "markov_-700",
        "quotes_-700",
        "polls_-700",
        "nothing",
        &format!("emojipasta_-700_{}", session.uuid),
        &format!("owoify_-700_{}", session.uuid),
        &format!("casesensivity_-700_{}", session.uuid),
        &format!("alwaysreply_-700_{}", session.uuid),
        &format!("randomreplies_-700_{}", session.uuid),
        &format!("pauselearning_-700_{}", session.uuid),
    ] {
        assert!(
            serialized.contains(expected),
            "missing callback_data {expected} in {serialized}"
        );
    }
}

#[tokio::test]
async fn percentage_command_updates_the_chat() {
    let (st, tg, _db) = setup("percentage").await;
    tg.make_admin(-800, 5);
    make_consented(&st, 5).await;

    handle_update(
        &st,
        &tg,
        update_message(1, 5, group(-800), "/percentage 40"),
    )
    .await;
    let chat = st.db.get_chat(-800).await.unwrap().unwrap();
    assert_eq!(chat.percentage, 40);
    assert!(tg.sent_texts()[0].contains("successfully updated to `40%`"));

    handle_update(
        &st,
        &tg,
        update_message(2, 5, group(-800), "/percentage abc"),
    )
    .await;
    assert_eq!(tg.sent_texts()[1], "The value you inserted is not a number");

    handle_update(
        &st,
        &tg,
        update_message(3, 5, group(-800), "/percentage 140"),
    )
    .await;
    assert_eq!(
        tg.sent_texts()[2],
        "Percentage must be a number between 0 and 100"
    );
}

#[tokio::test]
async fn replying_to_the_bot_can_trigger_a_reply() {
    let (st, tg, _db) = setup("reply-bot").await;
    make_consented(&st, 5).await;
    let mut chat = st.db.get_or_insert_chat(-100, false).await.unwrap();
    chat.percentage = 50;
    chat.quotes_disabled = true;
    st.db.update_chat(&chat).await.unwrap();

    handle_update(&st, &tg, update_message(1, 5, group(-100), "seed phrase")).await;

    // reply to the bot with percentage>0 and always_reply semantics:
    // percentage*2 = 100 chance, always fires
    let mut session = st.db.get_default_session(-100).await.unwrap();
    session.always_reply = true;
    st.db.update_session(&session).await.unwrap();
    st.cache_session(&session);

    handle_update(
        &st,
        &tg,
        update_message_with_reply(
            2,
            5,
            group(-100),
            "what do you say?",
            json!({
                "message_id": 900,
                "date": 1_700_000_000i64,
                "chat": group(-100),
                "from": {"id": 4242, "is_bot": true, "first_name": "Bot"},
                "text": "bot message",
            }),
        ),
    )
    .await;
    assert!(
        !tg.sent_texts().is_empty(),
        "replying to the bot with always_reply must answer"
    );
}
