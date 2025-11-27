use egui::{Color32, ComboBox, CursorIcon, Label, Response, RichText, Ui};

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

/// Formats a duration rounded to minutes as "Xh Ym" or "Xm".
pub fn format_minutes(duration: std::time::Duration) -> String {
    let total_mins = duration.as_secs() / 60;
    let hours = total_mins / 60;
    let mins = total_mins % 60;

    if hours > 0 {
        if mins > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}h", hours)
        }
    } else {
        format!("{}m", mins.max(1)) // Show at least 1m
    }
}

/// Give a datetime, formats it into a human-readable string (e.g., "2025-03-10 10:00:00").
pub fn format_datetime(dt: chrono::DateTime<chrono::Local>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn dropdown_list(
    ui: &mut Ui,
    label: &str,
    options: &[&str],
    selected: &mut String,
    add_content: impl FnOnce(&mut Ui),
) -> Response {
    ui.horizontal(|ui| {
        ui.label(label);
        ComboBox::from_id_salt(label)
            .selected_text(selected.as_str())
            .show_ui(ui, |ui| {
                for option in options {
                    ui.selectable_value(selected, option.to_string(), *option);
                }
            });
        add_content(ui);
    })
    .response
}

pub fn tooltip(ui: &mut Ui, text: &str, error_override: Option<Color32>) {
    ui.add(Label::new(RichText::new("ℹ").color(
        error_override.unwrap_or(Color32::from_rgb(128, 128, 128)),
    )))
    .on_hover_cursor(CursorIcon::Help)
    .on_hover_text(text);
}
