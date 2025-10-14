use color_eyre::eyre;

/// Ensures only one instance of the application is running.
#[cfg(target_os = "windows")]
pub fn ensure_single_instance() -> eyre::Result<()> {
    use windows::{
        Win32::{
            Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
            System::Threading::{CreateMutexW, WaitForSingleObject},
        },
        core::PCWSTR,
    };

    let mutex_name = "OWL-Control-SingleInstance";
    let mutex_name_wide: Vec<u16> = mutex_name
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mutex_handle = CreateMutexW(
            None,
            true, // We own the mutex initially
            PCWSTR(mutex_name_wide.as_ptr()),
        );

        if mutex_handle.is_err() {
            tracing::warn!("Failed to create mutex for single instance check");
            return Ok(());
        }

        let mutex_handle = mutex_handle.unwrap();

        // Try to acquire the mutex (with 0 timeout to check immediately)
        let wait_result = WaitForSingleObject(mutex_handle, 0);

        match wait_result {
            WAIT_OBJECT_0 => {
                // We successfully acquired the mutex, we're the only instance
                // The mutex will be automatically released when the process exits
            }
            WAIT_TIMEOUT => {
                use crate::ui::notification::{NotificationType, show_notification};

                show_notification(
                    "OWL Control",
                    "Another instance of OWL Control is already running.",
                    "Only one instance can run at a time.",
                    NotificationType::Error,
                );
                eyre::bail!("Another instance of OWL Control is already running.");
            }
            _ => {
                tracing::warn!("Unexpected error during single instance check");
                return Ok(());
            }
        }
    }

    Ok(())
}

/// Ensures only one instance of the application is running.
#[cfg(not(target_os = "windows"))]
pub fn ensure_single_instance() -> eyre::Result<()> {
    // On non-Windows platforms, single instance checking is not implemented
    // This could be extended to use file locking or other mechanisms if needed
    Ok(())
}
