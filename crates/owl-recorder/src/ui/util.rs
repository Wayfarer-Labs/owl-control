/// Formats a byte count into a human-readable string (e.g., "1.2 MB").
pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let k = 1024_f64;
    let sizes = ["B", "KB", "MB", "GB"];
    let bytes_f = bytes as f64;
    let i = (bytes_f.ln() / k.ln()).floor() as usize;
    let i = i.min(sizes.len() - 1);
    let value = bytes_f / k.powi(i as i32);
    format!("{:.1} {}", value, sizes[i])
}

/// Formats seconds into a human-readable string (e.g., "1h 2m 30s").
pub fn format_seconds(total_seconds: u64) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    match (hours, minutes) {
        (0, 0) => format!("{seconds}s"),
        (0, _) => format!("{minutes}m {seconds}s"),
        (_, _) => format!("{hours}h {minutes}m {seconds}s"),
    }
}

/// Given a RFC3339 date string, formats it into a human-readable string (e.g., "10/03/2025 at 10:00:00 AM").
pub fn format_rfc3339_date(date: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(date)
        .map(|dt| dt.with_timezone(&chrono::Local))
        .map(|dt| format!("{} at {}", dt.format("%m/%d/%Y"), dt.format("%I:%M:%S %p")))
        .unwrap_or_else(|_| "Unknown".to_string())
}
