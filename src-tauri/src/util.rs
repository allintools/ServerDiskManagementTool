use chrono::Utc;
use uuid::Uuid;

pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

pub fn new_id() -> String {
    Uuid::new_v4().to_string()
}

pub fn now_secs() -> i64 {
    Utc::now().timestamp()
}

pub fn clamp_i64(value: i64, min: i64, max: i64) -> i64 {
    value.clamp(min, max)
}

pub fn short_text(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    let mut out: String = input.chars().take(max).collect();
    out.push('…');
    out
}
