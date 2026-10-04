//! Port of `src/utils/human_bytes.nim`.

pub fn human_bytes(mut num: i64, suffix: &str) -> String {
    for unit in ["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi"] {
        if num.abs() < 1024 {
            return format!("{num}{unit}{suffix}");
        }
        num /= 1024;
    }
    format!("{num}Yi{suffix}")
}

pub fn human_bytes_b(num: i64) -> String {
    human_bytes(num, "B")
}
