pub mod models;
pub mod queries;
pub mod schema;

pub use models::{Chat, CountTable, Message, Session, User};
pub use queries::Db;

/// Database path relative to the working directory (Docker mounts `./data`
/// there); the `/stats` command reads its size from here too.
pub const DB_PATH: &str = "data/markov.db";
pub const DATA_FOLDER: &str = "data";
