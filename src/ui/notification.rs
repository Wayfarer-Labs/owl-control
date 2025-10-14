use tauri_winrt_notification::{HSTRING, Toast};
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW};

pub enum NotificationType {
    Info,
    Error,
}
pub fn show_notification(
    title: &str,
    text1: &str,
    text2: &str,
    notification_type: NotificationType,
) {
    match notification_type {
        NotificationType::Info => {
            let mut toast = Toast::new(Toast::POWERSHELL_APP_ID);
            if !title.is_empty() {
                toast = toast.title(title);
            }
            if !text1.is_empty() {
                toast = toast.text1(text1);
            }
            if !text2.is_empty() {
                toast = toast.text2(text2);
            }
            if let Err(e) = toast.sound(None).show() {
                tracing::error!(
                    "Failed to show notification (title: {title}, text1: {text1}, text2: {text2}): {e}"
                );
            }
        }
        NotificationType::Error => unsafe {
            MessageBoxW(
                None,
                &HSTRING::from(format!("{text1}\n{text2}")),
                &HSTRING::from(title),
                MB_ICONERROR,
            );
        },
    }
}
