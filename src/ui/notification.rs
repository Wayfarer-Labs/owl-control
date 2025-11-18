use windows::{
    Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW},
    core::HSTRING,
};

pub fn error_message_box(body: &str) {
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from(body),
            &HSTRING::from("OWL Control - Error"),
            MB_ICONERROR | MB_TOPMOST | MB_SETFOREGROUND,
        );
    }
}
