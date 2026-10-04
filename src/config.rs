//! Configuration loading, mirroring the Nim `parseconfig` semantics:
//! `secret.ini` next to the working directory wins over environment variables.

#[derive(Debug, Clone)]
pub struct Config {
    pub token: String,
    pub admin: Option<i64>,
    pub logging: bool,
    pub keep_last: i64,
}

fn unquote(raw: &str) -> String {
    let v = raw.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        return v[1..v.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
    }
    v.to_string()
}

/// Returns the value of `key` inside the `[config]` section of `secret.ini`,
/// if the file exists and the key is present.
fn ini_value(ini: &Option<String>, key: &str) -> Option<String> {
    let content = ini.as_ref()?;
    let mut in_section = false;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_section = line[1..line.len() - 1]
                .trim()
                .eq_ignore_ascii_case("config");
            continue;
        }
        if let Some((k, v)) = line.split_once('=')
            && in_section
            && k.trim().eq_ignore_ascii_case(key)
        {
            return Some(unquote(v));
        }
    }
    None
}

pub fn load() -> Config {
    load_from("secret.ini")
}

pub fn load_from(path: &str) -> Config {
    let ini = std::fs::read_to_string(path).ok();
    let get = |key: &str, env: &str| -> Option<String> {
        ini_value(&ini, key).or_else(|| std::env::var(env).ok().filter(|v| !v.is_empty()))
    };

    let admin = get("admin", "ADMIN_ID").and_then(|v| match v.trim().parse::<i64>() {
        Ok(id) => Some(id),
        Err(e) => {
            eprintln!("[ERROR]: Invalid ADMIN_ID value: {e}");
            None
        }
    });
    let keep_last = get("keeplast", "KEEP_LAST")
        .and_then(|v| v.trim().parse::<i64>().ok())
        .unwrap_or(1500);

    Config {
        token: get("token", "BOT_TOKEN").unwrap_or_default(),
        admin,
        logging: get("logging", "LOGGING")
            .map(|v| v.trim() == "1")
            .unwrap_or(false),
        keep_last,
    }
}
