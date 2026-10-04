use std::sync::Arc;
use std::time::Duration;

use teloxide::prelude::*;

use markinim::config;
use markinim::db::{DATA_FOLDER, DB_PATH, Db};
use markinim::handlers::{TeloxideBot, handle_update};
use markinim::quote::QuoteAssets;
use markinim::state::AppState;

#[tokio::main]
async fn main() {
    let cfg = config::load();
    if cfg.token.is_empty() {
        eprintln!("[ERROR]: Token not provided. Check secret.ini or environment variables");
        quit(1);
    }

    let level = if cfg.logging {
        tracing::level_filters::LevelFilter::INFO
    } else {
        tracing::level_filters::LevelFilter::ERROR
    };
    tracing_subscriber::fmt()
        .with_max_level(level)
        .with_writer(std::io::stderr)
        .init();

    let db = match Db::open(std::path::Path::new(DB_PATH)) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("[ERROR]: cannot open {DB_PATH}: {e}");
            quit(1);
        }
    };
    let quote = match QuoteAssets::load() {
        Ok(assets) => assets,
        Err(e) => {
            eprintln!("[ERROR]: {e}");
            quit(1);
        }
    };
    let st = Arc::new(AppState::new(db, cfg.clone(), quote));
    let _ = std::fs::create_dir_all(DATA_FOLDER);

    let bot = Bot::new(cfg.token.clone());

    let me = match bot.get_me().await {
        Ok(me) => me,
        Err(e) => {
            eprintln!("[ERROR]: getMe failed: {e}");
            quit(1);
        }
    };
    let username = me
        .user
        .username
        .clone()
        .unwrap_or_default()
        .trim()
        .to_owned();
    st.set_bot_info(username.clone(), me.user.id.0 as i64);
    eprintln!("Running... Bot username: {username}");

    if let Some(admin) = cfg.admin
        && let Ok(user) = st.db.set_admin(admin, true).await
    {
        st.admins.write().unwrap().insert(user.user_id);
    }
    if let Ok(admins) = st.db.get_bot_admins().await {
        let mut set = st.admins.write().unwrap();
        for admin in admins {
            set.insert(admin.user_id);
        }
    }
    if let Ok(banned) = st.db.get_banned_users().await {
        let mut set = st.banned.write().unwrap();
        for user in banned {
            set.insert(user.user_id);
        }
    }

    // Port of cleanerWorker, on a 30 second cadence (the Nim loop slept 30ms).
    let sweeper_state = st.clone();
    let sweeper_bot = bot.clone();
    tokio::spawn(async move {
        let tg = TeloxideBot(sweeper_bot);
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            sweeper_state.sweep(&tg).await;
        }
    });

    let tg = TeloxideBot(bot.clone());
    let mut offset: i32 = 0;
    loop {
        let poll = {
            let req = bot.get_updates();
            let req = if offset == 0 { req } else { req.offset(offset) };
            req.timeout(25)
        };
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                eprintln!("\nQuitting...\nProgram has run for {} seconds.", st.started.elapsed().as_secs());
                quit(0);
            }
            result = poll.send() => match result {
                Ok(updates) => {
                    for update in updates {
                        offset = ((update.id.0 as i64) + 1).min(i32::MAX as i64) as i32;
                        handle_update(&st, &tg, update).await;
                    }
                }
                Err(e) => {
                    tracing::error!("Fatal error occurred. Restarting the bot...");
                    tracing::error!("getCurrentExceptionMsg(): {e}");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            },
        }
    }
}

fn quit(code: i32) -> ! {
    std::process::exit(code)
}
