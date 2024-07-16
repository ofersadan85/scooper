use std::{env, time::SystemTime};

const SCALE_BYTES: [&'static str; 7] = ["B", "KB", "MB", "GB", "TB", "PB", "EB"];

pub fn now() -> u128 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn parsable_env_var<T: std::str::FromStr>(name: &str, default: T) -> T {
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn human_readable_size(size: usize) -> String {
    let base: usize = 1024;
    let max_size: usize = base.pow((SCALE_BYTES.len() - 1) as u32);
    if size > max_size {
        return format!("{} B", size);
    }
    let mut size = size as f64;
    let mut unit = 0;
    let base_f64: f64 = base as f64;
    while size >= base_f64 {
        size /= base_f64;
        unit += 1;
    }
    let unit = SCALE_BYTES[unit]; // We know that unit is within the range of SCALE_BYTES.len()
    let size_fmt = if unit == "B" {
        format!("{:.0}", size)
    } else {
        format!("{:.2}", size)
    };
    format!("{size_fmt} {unit}")
}
