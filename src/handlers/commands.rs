//! Port of `handleCommand` from `src/markinim.nim`, one function per command.

use std::sync::Arc;

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};

use super::messages::{consent_notice, decorate};
use super::{
    CONSENT_TEXT, CREATOR_STRING, EditParams, FlowError, HELP_TEXT, PRIVACY_TEXT, ParseModeKind,
    SETTINGS_TEXT, SendParams, Tg, UNALLOWED,
};
use crate::db::{CountTable, Session, User};
use crate::keyboards;
use crate::markov::END;
use crate::state::AppState;
use crate::text::{human_bytes_b, random_emoji};

pub struct MsgCtx {
    pub chat_id: i64,
    pub msg_id: i64,
    pub thread_id: Option<i64>,
    pub sender_id: i64,
    pub sender_anonymous_admin: bool,
    pub is_group: bool,
    pub reply_to_msg_id: Option<i64>,
    pub reply_to_sender_id: Option<i64>,
    pub reply_to_sender_chat_id: Option<i64>,
}

const START_MESSAGE: &str = "Hello, I learn from your messages and try to formulate my own sentences. Add me in a chat or send /enable to try me out here ᗜᴗᗜ\nSee /help for more information, and /privacy for my privacy policy.";

fn consent_keyboard(consented: bool) -> InlineKeyboardMarkup {
    let (text, data) = if consented {
        ("Revoke consent", "consent_revoke")
    } else {
        ("Give consent", "consent_give")
    };
    InlineKeyboardMarkup {
        inline_keyboard: vec![vec![InlineKeyboardButton::callback(text, data)]],
    }
}

fn add_me_keyboard(username: &str) -> InlineKeyboardMarkup {
    let url = url::Url::parse(&format!("https://t.me/{username}?startgroup=enable"))
        .expect("bot username produced an invalid url");
    InlineKeyboardMarkup {
        inline_keyboard: vec![vec![InlineKeyboardButton::url("Add me :D", url)]],
    }
}

async fn is_sender_admin(st: &AppState, tg: &dyn Tg, ctx: &MsgCtx) -> bool {
    ctx.sender_anonymous_admin || st.is_admin_in_group(tg, ctx.chat_id, ctx.sender_id).await
}

fn thread_params(ctx: &MsgCtx) -> SendParams<'static> {
    SendParams {
        thread: ctx.thread_id,
        ..Default::default()
    }
}

pub async fn dispatch(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    command: &str,
    args: &[String],
    db_user: &User,
) -> Result<(), FlowError> {
    match command {
        "start" => start(st, tg, ctx, args, db_user).await,
        "deleteme" => deleteme(st, tg, ctx, args, db_user.user_id).await,
        "help" => {
            if ctx.is_group && !is_sender_admin(st, tg, ctx).await {
                return Ok(());
            }
            tg.send_message(
                ctx.chat_id,
                HELP_TEXT,
                SendParams {
                    parse_mode: Some(ParseModeKind::Markdown),
                    thread: ctx.thread_id,
                    ..Default::default()
                },
            )
            .await?;
            Ok(())
        }
        "privacy" => {
            if ctx.chat_id != ctx.sender_id {
                return Ok(());
            }
            tg.send_message(
                ctx.chat_id,
                PRIVACY_TEXT,
                SendParams {
                    parse_mode: Some(ParseModeKind::Markdown),
                    thread: ctx.thread_id,
                    ..Default::default()
                },
            )
            .await?;
            Ok(())
        }
        "manageconsent" => {
            if ctx.chat_id != ctx.sender_id {
                return Ok(());
            }
            let kb = consent_keyboard(db_user.consented);
            tg.send_message(
                ctx.chat_id,
                CONSENT_TEXT,
                SendParams {
                    parse_mode: Some(ParseModeKind::Markdown),
                    thread: ctx.thread_id,
                    keyboard: Some(&kb),
                    ..Default::default()
                },
            )
            .await?;
            Ok(())
        }
        "admin" | "unadmin" | "remadmin" => admin_command(st, tg, ctx, args, command).await,
        "botadmins" => botadmins(st, tg, ctx).await,
        "count" | "stats" => stats(st, tg, ctx).await,
        "banpeer" | "unbanpeer" => banpeer(st, tg, ctx, args, command).await,
        "enable" | "disable" => enable_disable(st, tg, ctx, command, db_user).await,
        "sessions" => {
            if ctx.is_group && !is_sender_admin(st, tg, ctx).await {
                tg.send_message(ctx.chat_id, UNALLOWED, thread_params(ctx))
                    .await?;
                return Ok(());
            }
            st.db.get_default_session(ctx.chat_id).await?;
            let sessions = st.db.get_sessions(ctx.chat_id).await?;
            let kb = sessions_keyboard_for(st, ctx.chat_id, &sessions).await?;
            tg.send_message(
                ctx.chat_id,
                "*Current sessions in this chat.* Send /delete to delete the current one.",
                SendParams {
                    parse_mode: Some(ParseModeKind::Markdown),
                    thread: ctx.thread_id,
                    keyboard: Some(&kb),
                    ..Default::default()
                },
            )
            .await?;
            Ok(())
        }
        "percentage" => percentage(st, tg, ctx, args, db_user).await,
        "markov" | "quote" => markov_command(st, tg, ctx, args, command, db_user).await,
        "wouldyourather" => would_you_rather(st, tg, ctx, args, db_user).await,
        "settings" => {
            if ctx.is_group && !is_sender_admin(st, tg, ctx).await {
                tg.send_message(ctx.chat_id, UNALLOWED, thread_params(ctx))
                    .await?;
                return Ok(());
            }
            let session = st.get_cached_session(ctx.chat_id).await?;
            let kb = keyboards::settings_keyboard(&session);
            tg.send_message(
                ctx.chat_id,
                SETTINGS_TEXT,
                SendParams {
                    parse_mode: Some(ParseModeKind::Markdown),
                    thread: ctx.thread_id,
                    keyboard: Some(&kb),
                    ..Default::default()
                },
            )
            .await?;
            Ok(())
        }
        "distort" | "hazmat" => Ok(()),
        "delete" => delete(st, tg, ctx, args).await,
        "deletefrom" | "delfrom" | "delete_from" | "del_from" => {
            delete_from(st, tg, ctx, args).await
        }
        _ => Ok(()),
    }
}

async fn start(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    args: &[String],
    db_user: &User,
) -> Result<(), FlowError> {
    if ctx.chat_id != ctx.sender_id {
        // Groups (and channels): both arg branches send the same message in
        // the Nim source, so the arguments are simply ignored.
        let kb = add_me_keyboard(&st.bot_username());
        tg.send_message(
            ctx.chat_id,
            START_MESSAGE,
            SendParams {
                thread: ctx.thread_id,
                keyboard: Some(&kb),
                ..Default::default()
            },
        )
        .await?;
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("consent") {
        let kb = consent_keyboard(db_user.consented);
        tg.send_message(
            ctx.chat_id,
            CONSENT_TEXT,
            SendParams {
                parse_mode: Some(ParseModeKind::Markdown),
                thread: ctx.thread_id,
                keyboard: Some(&kb),
                ..Default::default()
            },
        )
        .await?;
        return Ok(());
    }
    let kb = add_me_keyboard(&st.bot_username());
    tg.send_message(
        ctx.chat_id,
        START_MESSAGE,
        SendParams {
            thread: ctx.thread_id,
            keyboard: Some(&kb),
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

async fn deleteme(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    args: &[String],
    sender_id: i64,
) -> Result<(), FlowError> {
    if ctx.chat_id != ctx.sender_id {
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("confirm") {
        let count = st.db.delete_all_messages_from_user(sender_id).await?;
        tg.send_message(
            ctx.chat_id,
            &format!(
                "Operation completed. Successfully deleted `{count}` messages from my database!\nNote: some messages might still be cached in the bot's memory in the compiled markov model, they will expire soon (at most in 4 hours, after the bot restarts for its backup procedure)\nIf this is an urgent matter, please contact my creator. You can find more information on the bot's bio."
            ),
            SendParams { parse_mode: Some(ParseModeKind::Markdown), thread: ctx.thread_id, ..Default::default() },
        )
        .await?;
        return Ok(());
    }
    let count = st.db.get_total_user_messages_count(sender_id).await?;
    tg.send_message(
        ctx.chat_id,
        &format!(
            "This command will delete all your {count} messages from my database. Are you sure? Send `/deleteme confirm` to confirm."
        ),
        SendParams { parse_mode: Some(ParseModeKind::Markdown), thread: ctx.thread_id, ..Default::default() },
    )
    .await?;
    Ok(())
}

async fn admin_command(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    args: &[String],
    command: &str,
) -> Result<(), FlowError> {
    if args.is_empty() {
        return Ok(());
    }
    if !st.admins.read().unwrap().contains(&ctx.sender_id) {
        tg.send_message(ctx.chat_id, UNALLOWED, thread_params(ctx))
            .await?;
        return Ok(());
    }
    let user_id: i64 = match args[0].parse() {
        Ok(id) => id,
        Err(e) => {
            tg.send_message(
                ctx.chat_id,
                &format!("An error occurred: <code>{e}</code>"),
                SendParams {
                    parse_mode: Some(ParseModeKind::Html),
                    thread: ctx.thread_id,
                    ..Default::default()
                },
            )
            .await?;
            return Ok(());
        }
    };
    let promote = command == "admin";
    if let Err(e) = st.db.set_admin(user_id, promote).await {
        tg.send_message(
            ctx.chat_id,
            &format!("An error occurred: <code>{e}</code>"),
            SendParams {
                parse_mode: Some(ParseModeKind::Html),
                thread: ctx.thread_id,
                ..Default::default()
            },
        )
        .await?;
        return Ok(());
    }
    if promote {
        st.admins.write().unwrap().insert(user_id);
    } else {
        st.admins.write().unwrap().remove(&user_id);
    }
    let text = if promote {
        format!("Successfully promoted [{user_id}](tg://user?id={user_id})")
    } else {
        format!("Successfully demoted [{user_id}](tg://user?id={user_id})")
    };
    tg.send_message(
        ctx.chat_id,
        &text,
        SendParams {
            parse_mode: Some(ParseModeKind::Markdown),
            thread: ctx.thread_id,
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

async fn botadmins(st: &Arc<AppState>, tg: &dyn Tg, ctx: &MsgCtx) -> Result<(), FlowError> {
    if !st.admins.read().unwrap().contains(&ctx.sender_id) {
        tg.send_message(ctx.chat_id, UNALLOWED, thread_params(ctx))
            .await?;
        return Ok(());
    }
    let admins = st.db.get_bot_admins().await?;
    let list: Vec<String> = admins.iter().map(|a| a.mention()).collect();
    tg.send_message(
        ctx.chat_id,
        &format!("*List of the bot admins:*\n{}", list.join("\n")),
        SendParams {
            parse_mode: Some(ParseModeKind::Markdown),
            thread: ctx.thread_id,
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

fn rss_bytes() -> i64 {
    std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|statm| {
            statm
                .split_whitespace()
                .nth(1)
                .and_then(|pages| pages.parse::<i64>().ok())
                .map(|pages| pages * 4096)
        })
        .unwrap_or(0)
}

async fn stats(st: &Arc<AppState>, tg: &dyn Tg, ctx: &MsgCtx) -> Result<(), FlowError> {
    if !st.admins.read().unwrap().contains(&ctx.sender_id) {
        tg.send_message(ctx.chat_id, UNALLOWED, thread_params(ctx))
            .await?;
        return Ok(());
    }
    let users = st.db.get_count(CountTable::Users).await?;
    let chats = st.db.get_count(CountTable::Chats).await?;
    let messages = st.db.get_count(CountTable::Messages).await?;
    let sessions = st.db.get_count(CountTable::Sessions).await?;
    let (cached_sessions, cached_markovs) = (
        st.chat_sessions.lock().unwrap().len(),
        st.markovs.lock().unwrap().len(),
    );
    let uptime = st.started.elapsed().as_secs();
    let db_size = std::fs::metadata(crate::db::DB_PATH)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let text = format!(
        "*Users*: `{users}`\n*Chats*: `{chats}`\n*Messages*: `{messages}`\n*Sessions*: `{sessions}`\n*Cached sessions*: `{cached_sessions}`\n*Cached markovs*: `{cached_markovs}`\n*Uptime*: `{uptime}s`\n*Database size*: `{}`\n*Memory usage (RSS)*: `{}`\n",
        human_bytes_b(db_size),
        human_bytes_b(rss_bytes()),
    );
    tg.send_message(
        ctx.chat_id,
        &text,
        SendParams {
            parse_mode: Some(ParseModeKind::Markdown),
            thread: ctx.thread_id,
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

async fn banpeer(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    args: &[String],
    command: &str,
) -> Result<(), FlowError> {
    if args.is_empty() {
        return Ok(());
    }
    if !st.admins.read().unwrap().contains(&ctx.sender_id) {
        tg.send_message(ctx.chat_id, UNALLOWED, thread_params(ctx))
            .await?;
        return Ok(());
    }
    let peer_id: i64 = match args[0].parse() {
        Ok(id) => id,
        Err(e) => {
            tg.send_message(
                ctx.chat_id,
                &format!("An error occurred: <code>{e}</code>"),
                SendParams {
                    parse_mode: Some(ParseModeKind::Html),
                    thread: ctx.thread_id,
                    ..Default::default()
                },
            )
            .await?;
            return Ok(());
        }
    };
    if peer_id == ctx.sender_id {
        tg.send_message(
            ctx.chat_id,
            "You are not allowed to ban yourself",
            thread_params(ctx),
        )
        .await?;
        return Ok(());
    }
    let banned = command == "banpeer";
    let set_result = if peer_id < 0 {
        st.db.set_banned_chat(peer_id, banned).await.map(|_| ())
    } else {
        st.db.set_banned_user(peer_id, banned).await.map(|_| ())
    };
    if let Err(e) = set_result {
        tg.send_message(
            ctx.chat_id,
            &format!("An error occurred: <code>{e}</code>"),
            SendParams {
                parse_mode: Some(ParseModeKind::Html),
                thread: ctx.thread_id,
                ..Default::default()
            },
        )
        .await?;
        return Ok(());
    }
    if banned {
        st.banned.write().unwrap().insert(peer_id);
    } else {
        st.banned.write().unwrap().remove(&peer_id);
    }
    let text = if banned {
        format!("Successfully banned [{peer_id}](tg://user?id={peer_id})")
    } else {
        format!("Successfully unbanned [{peer_id}](tg://user?id={peer_id})")
    };
    tg.send_message(
        ctx.chat_id,
        &text,
        SendParams {
            parse_mode: Some(ParseModeKind::Markdown),
            thread: ctx.thread_id,
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

async fn enable_disable(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    command: &str,
    db_user: &User,
) -> Result<(), FlowError> {
    if ctx.is_group && !is_sender_admin(st, tg, ctx).await {
        tg.send_message(ctx.chat_id, UNALLOWED, thread_params(ctx))
            .await?;
        return Ok(());
    }
    st.db.set_enabled(ctx.chat_id, command == "enable").await?;
    if command == "enable" {
        let mut user = db_user.clone();
        user.consented = true;
        st.db.update_user(&user).await?;
    }
    let text = if command == "enable" {
        "Successfully enabled learning in this chat".to_owned()
    } else {
        let mut text =
            "Successfully disabled learning in this chat. If you want to enable it, send /enable."
                .to_owned();
        if !ctx.is_group {
            text += "\nNote: the bot will still learn in groups where it is enabled. If you don't want this, check out /manageconsent.";
        }
        text
    };
    tg.send_message(ctx.chat_id, &text, thread_params(ctx))
        .await?;
    Ok(())
}

async fn percentage(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    args: &[String],
    db_user: &User,
) -> Result<(), FlowError> {
    if ctx.is_group && !is_sender_admin(st, tg, ctx).await {
        tg.send_message(ctx.chat_id, UNALLOWED, thread_params(ctx))
            .await?;
        return Ok(());
    }
    let mut chat = st.db.get_or_insert_chat(ctx.chat_id, false).await?;
    if args.is_empty() {
        tg.send_message(
            ctx.chat_id,
            &format!(
                "This command needs an argument. Example: `/percentage 40` (default: `30`)\nCurrent percentage: `{}`%",
                chat.percentage
            ),
            SendParams { parse_mode: Some(ParseModeKind::Markdown), thread: ctx.thread_id, ..Default::default() },
        )
        .await?;
        return Ok(());
    }
    if !ctx.is_group && !db_user.consented {
        consent_notice(st, tg, ctx).await?;
        return Ok(());
    }
    let parsed = args[0]
        .trim_matches(|c: char| c.is_whitespace() || c == '%')
        .parse::<i64>();
    match parsed {
        Ok(value) => {
            if !(0..=100).contains(&value) {
                tg.send_message(
                    ctx.chat_id,
                    "Percentage must be a number between 0 and 100",
                    thread_params(ctx),
                )
                .await?;
                return Ok(());
            }
            chat.percentage = value;
            st.db.update_chat(&chat).await?;
            tg.send_message(
                ctx.chat_id,
                &format!("Percentage has been successfully updated to `{value}%`"),
                SendParams {
                    parse_mode: Some(ParseModeKind::Markdown),
                    thread: ctx.thread_id,
                    ..Default::default()
                },
            )
            .await?;
        }
        Err(_) => {
            tg.send_message(
                ctx.chat_id,
                "The value you inserted is not a number",
                thread_params(ctx),
            )
            .await?;
        }
    }
    Ok(())
}

async fn markov_command(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    args: &[String],
    command: &str,
    db_user: &User,
) -> Result<(), FlowError> {
    let chat = st.db.get_or_insert_chat(ctx.chat_id, false).await?;
    if !chat.enabled {
        tg.send_message(
            ctx.chat_id,
            "Learning is not enabled in this chat. Enable it with /enable (for groups: admins only)",
            thread_params(ctx),
        )
        .await?;
        return Ok(());
    }
    if !db_user.consented {
        consent_notice(st, tg, ctx).await?;
        return Ok(());
    }
    let cached_session = st.get_cached_session(ctx.chat_id).await?;
    if (cached_session.chat.markov_disabled
        || (command == "quote" && cached_session.chat.quotes_disabled))
        && !is_sender_admin(st, tg, ctx).await
    {
        return Ok(());
    }
    st.ensure_markov(&cached_session).await;
    let sample_count = {
        let markovs = st.markovs.lock().unwrap();
        markovs
            .get(&ctx.chat_id)
            .map(|(_, chain)| chain.samples.len())
            .unwrap_or(0)
    };
    if sample_count == 0 {
        tg.send_message(
            ctx.chat_id,
            "Not enough data to generate a sentence",
            thread_params(ctx),
        )
        .await?;
        return Ok(());
    }

    let mut start = args.join(" ");
    if !cached_session.case_sensitive {
        start = start.to_lowercase();
    }
    let begin: Option<String> = if !args.is_empty() && (args[0] != END || args.len() >= 2) {
        Some(start)
    } else {
        None
    };

    let generated = {
        let markovs = st.markovs.lock().unwrap();
        let Some((_, chain)) = markovs.get(&ctx.chat_id) else {
            return Ok(());
        };
        let mut rng = rand::rng();
        match chain.generate(begin.as_deref(), &mut rng) {
            Ok(text) => Some(text),
            // Nim: catch MarkovGenerateError and retry with a plain generation.
            Err(_) => chain.generate(None, &mut rng).ok(),
        }
    };
    let Some(generated) = generated else {
        // The plain retry raised too, which in Nim bubbles to the outer catch.
        tracing::error!("[ERROR] | MarkovGenerateError: not enough samples");
        return Ok(());
    };
    let text = decorate(&cached_session, generated);

    if command == "markov" {
        tg.send_message(
            ctx.chat_id,
            &text,
            SendParams {
                thread: ctx.thread_id,
                reply_to: ctx.reply_to_msg_id,
                ..Default::default()
            },
        )
        .await?;
    } else if !st.is_flood(ctx.chat_id, 3, 20) {
        let png = st.quote.render(&text, &mut rand::rng());
        tg.send_photo(ctx.chat_id, png).await?;
    }
    Ok(())
}

fn sort_candidates(mut options: Vec<String>, length: usize) -> Vec<String> {
    options.sort_by_key(|a| a.len());
    options.reverse();
    for option in options.iter_mut() {
        if option.len() > length {
            // Port of trimUnicode: cut at the byte offset of the `length`-th rune.
            let offset: usize = option.chars().take(length).map(char::len_utf8).sum();
            option.truncate(offset);
        }
    }
    options
}

async fn would_you_rather(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    args: &[String],
    db_user: &User,
) -> Result<(), FlowError> {
    let chat = st.db.get_or_insert_chat(ctx.chat_id, false).await?;
    if !chat.enabled {
        tg.send_message(
            ctx.chat_id,
            "Learning is not enabled in this chat. Enable it with /enable (for groups: admins only)",
            thread_params(ctx),
        )
        .await?;
        return Ok(());
    }
    if !db_user.consented {
        consent_notice(st, tg, ctx).await?;
        return Ok(());
    }
    let cached_session = st.get_cached_session(ctx.chat_id).await?;
    if cached_session.chat.polls_disabled && !is_sender_admin(st, tg, ctx).await {
        return Ok(());
    }
    st.ensure_markov(&cached_session).await;
    let sample_count = {
        let markovs = st.markovs.lock().unwrap();
        markovs
            .get(&ctx.chat_id)
            .map(|(_, chain)| chain.samples.len())
            .unwrap_or(0)
    };
    if sample_count < 10 {
        tg.send_message(
            ctx.chat_id,
            "Not enough data to generate a would you rather poll",
            thread_params(ctx),
        )
        .await?;
        return Ok(());
    }

    let mut rng = rand::rng();
    let mut options: Vec<String> = Vec::new();
    for _ in 0..10 {
        let generated = {
            let markovs = st.markovs.lock().unwrap();
            let Some((_, chain)) = markovs.get(&ctx.chat_id) else {
                break;
            };
            chain.generate(None, &mut rng).ok()
        };
        match generated {
            Some(text) => options.push(decorate(&cached_session, text)),
            None => break,
        }
    }

    // deduplicate(isSorted = false): keep first occurrences in order.
    let mut unique: Vec<String> = Vec::new();
    for option in options {
        if !unique.contains(&option) {
            unique.push(option);
        }
    }
    let options = sort_candidates(unique, 100);

    if options.len() < 2 {
        tg.send_message(
            ctx.chat_id,
            "Not enough data to generate a would you rather poll",
            thread_params(ctx),
        )
        .await?;
        return Ok(());
    }

    let is_anon = args.first().map(String::as_str) == Some("anon");
    tg.send_poll(
        ctx.chat_id,
        &format!("{} Would you rather...", random_emoji()),
        options.into_iter().take(2).collect(),
        ctx.thread_id,
        is_anon,
    )
    .await?;
    Ok(())
}

async fn sessions_keyboard_for(
    st: &Arc<AppState>,
    chat_id: i64,
    sessions: &[Session],
) -> Result<InlineKeyboardMarkup, FlowError> {
    let default_session = st.db.get_default_session(chat_id).await?;
    let mut counts = Vec::with_capacity(sessions.len());
    for session in sessions {
        counts.push(
            st.db
                .get_messages_count(session.uuid.clone())
                .await
                .unwrap_or(0),
        );
    }
    Ok(keyboards::sessions_keyboard(
        chat_id,
        sessions,
        &default_session.uuid,
        &counts,
    ))
}

/// Port of `showSessions`: edits the message with the session list keyboard.
pub async fn show_sessions(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    chat_id: i64,
    message_id: i64,
    sessions: Vec<Session>,
) -> Result<(), FlowError> {
    let kb = sessions_keyboard_for(st, chat_id, &sessions).await?;
    tg.edit_message_text(
        chat_id,
        message_id,
        "*Current sessions in this chat.* Send /delete to delete the current one.",
        EditParams {
            parse_mode: Some(ParseModeKind::Markdown),
            keyboard: Some(&kb),
        },
    )
    .await?;
    Ok(())
}

async fn delete(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    args: &[String],
) -> Result<(), FlowError> {
    if ctx.is_group && !is_sender_admin(st, tg, ctx).await {
        tg.send_message(ctx.chat_id, UNALLOWED, thread_params(ctx))
            .await?;
        return Ok(());
    }
    if st.is_deleting(ctx.chat_id) {
        tg.send_message(
            ctx.chat_id,
            "I am already deleting the messages from my database. Please hold on",
            thread_params(ctx),
        )
        .await?;
        return Ok(());
    }
    if args.first().map(|s| s.to_lowercase()).as_deref() == Some("confirm") {
        let _guard = st.mark_deleting(ctx.chat_id);
        return match delete_confirm(st, tg, ctx).await {
            Ok(()) => Ok(()),
            Err(e) => {
                tg.send_message(
                    ctx.chat_id,
                    &format!("An error occurred. Operation has been aborted.{CREATOR_STRING}"),
                    SendParams {
                        thread: ctx.thread_id,
                        reply_to: Some(ctx.msg_id),
                        ..Default::default()
                    },
                )
                .await?;
                Err(e)
            }
        };
    }
    tg.send_message(
        ctx.chat_id,
        "If you are sure to delete data in this chat (of the current session), send `/delete confirm`. *NOTE*: This cannot be reverted",
        SendParams { parse_mode: Some(ParseModeKind::Markdown), thread: ctx.thread_id, ..Default::default() },
    )
    .await?;
    Ok(())
}

async fn delete_confirm(st: &Arc<AppState>, tg: &dyn Tg, ctx: &MsgCtx) -> Result<(), FlowError> {
    let sent = tg
        .send_message(
            ctx.chat_id,
            "I am deleting data for this session...",
            thread_params(ctx),
        )
        .await?;
    let default_session = st.get_cached_session(ctx.chat_id).await?;
    let deleted = st
        .db
        .delete_messages(default_session.uuid.clone(), ctx.chat_id)
        .await?;

    st.markovs.lock().unwrap().remove(&ctx.chat_id);
    st.evict_cached_session(ctx.chat_id);

    if st.db.get_sessions_count(ctx.chat_id).await? > 1 {
        // Nim: re-cache via getCachedSession, which promotes the first
        // remaining session to default.
        let session = st.get_cached_session(ctx.chat_id).await?;
        st.cache_session(&session);
    }

    tg.edit_message_text(
        ctx.chat_id,
        sent,
        &format!(
            "Operation completed. Successfully deleted `{deleted}` messages from my database!"
        ),
        EditParams {
            parse_mode: Some(ParseModeKind::Markdown),
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

async fn delete_from(
    st: &Arc<AppState>,
    tg: &dyn Tg,
    ctx: &MsgCtx,
    args: &[String],
) -> Result<(), FlowError> {
    if !ctx.is_group {
        tg.send_message(
            ctx.chat_id,
            "This command works only in groups",
            thread_params(ctx),
        )
        .await?;
        return Ok(());
    }
    if !is_sender_admin(st, tg, ctx).await {
        tg.send_message(ctx.chat_id, UNALLOWED, thread_params(ctx))
            .await?;
        return Ok(());
    }
    if args.is_empty() && ctx.reply_to_msg_id.is_none() {
        tg.send_message(
            ctx.chat_id,
            "Send `/delfrom user_id` or use it in reply to someone. It will delete all messages a user sent from the bot's database. *NOTE*: This cannot be reverted",
            SendParams { parse_mode: Some(ParseModeKind::Markdown), thread: ctx.thread_id, ..Default::default() },
        )
        .await?;
        return Ok(());
    }

    let user_id: i64 = if !args.is_empty() {
        match args[0].parse() {
            Ok(id) => id,
            Err(_) => {
                tg.send_message(
                    ctx.chat_id,
                    "Operation failed. Invalid integer (usernames are not allowed).",
                    thread_params(ctx),
                )
                .await?;
                return Ok(());
            }
        }
    } else if let Some(id) = ctx.reply_to_sender_id {
        id
    } else if let Some(id) = ctx.reply_to_sender_chat_id {
        id
    } else {
        tg.send_message(
            ctx.chat_id,
            &format!("Operation failed. No user has been found. {CREATOR_STRING}"),
            thread_params(ctx),
        )
        .await?;
        return Ok(());
    };

    let default_session = st.get_cached_session(ctx.chat_id).await?;
    let count = st
        .db
        .get_user_messages_count(default_session.uuid.clone(), user_id)
        .await?;
    if count < 1 {
        tg.send_message(
            ctx.chat_id,
            "There are 0 messages belonging to the specified user in this chat session. ",
            thread_params(ctx),
        )
        .await?;
        return Ok(());
    }

    let result = async {
        let sent = tg
            .send_message(
                ctx.chat_id,
                "I am deleting data from the specified user for this session...",
                thread_params(ctx),
            )
            .await?;
        let deleted = st.db.delete_from_user_in_chat(default_session.uuid.clone(), user_id).await?;
        st.markovs.lock().unwrap().remove(&ctx.chat_id);
        tg.edit_message_text(
            ctx.chat_id,
            sent,
            &format!(
                "Operation completed. Successfully deleted `{deleted}` messages sent by the specified user from my database!"
            ),
            EditParams { parse_mode: Some(ParseModeKind::Markdown), ..Default::default() },
        )
        .await?;
        Ok(())
    }
    .await;

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            tg.send_message(
                ctx.chat_id,
                &format!("An error occurred (does the user exist?). Operation has been aborted.{CREATOR_STRING}"),
                SendParams { thread: ctx.thread_id, reply_to: Some(ctx.msg_id), ..Default::default() },
            )
            .await?;
            Err(e)
        }
    }
}
