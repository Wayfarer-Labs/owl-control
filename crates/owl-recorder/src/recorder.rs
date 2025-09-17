use std::path::{Path, PathBuf};
use std::sync::Arc;

use color_eyre::{Result, eyre::Context as _};
use tauri_winrt_notification::Toast;
use windows::{
    Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW},
    core::HSTRING,
};

use crate::RecordingState;
use crate::{
    bootstrap_recorder::bootstrap_obs,
    find_game::get_foregrounded_game,
    recording::{InputParameters, MetadataParameters, Recording, WindowParameters},
};
use constants::unsupported_games::UNSUPPORTED_GAMES;

pub(crate) trait RecorderBackend {
    async fn start_recording(dummy_video_path: &Path, _pid: u32, _hwnd: usize) -> Result<Self>
    where
        Self: Sized;
    async fn stop_recording(&self) -> Result<()>;
}
pub(crate) struct Recorder<D, T>
where
    T: RecorderBackend,
{
    recording_dir: D,
    recording: Option<Recording<T>>,
}

impl<D, T> Recorder<D, T>
where
    D: FnMut() -> PathBuf,
    T: RecorderBackend,
{
    pub(crate) async fn new(
        recording_dir: D,
        recording_state: Arc<RecordingState>,
    ) -> Result<Self> {
        // Ensure that the OBS bootstrapper runs
        // TODO: if T is bootstrapper then run
        bootstrap_obs(recording_state).await?;

        Ok(Self {
            recording_dir,
            recording: None,
        })
    }

    pub(crate) fn recording(&self) -> Option<&Recording<T>> {
        self.recording.as_ref()
    }

    pub(crate) async fn start(&mut self) -> Result<()> {
        if self.recording.is_some() {
            return Ok(());
        }

        let recording_location = (self.recording_dir)();

        std::fs::create_dir_all(&recording_location)
            .wrap_err("Failed to create recording directory")?;

        let Some((game_exe, pid, hwnd)) =
            get_foregrounded_game().wrap_err("failed to get foregrounded game")?
        else {
            tracing::warn!("No game window found");
            show_notification(
                "Invalid game",
                "Not recording foreground window.",
                "It's either not a supported game or not fullscreen.",
                NotificationType::Error,
            );
            return Ok(());
        };

        let game_exe_without_extension = game_exe
            .split('.')
            .next()
            .unwrap_or(&game_exe)
            .to_lowercase();
        if let Some(unsupported_game) = UNSUPPORTED_GAMES
            .iter()
            .find(|ug| ug.binaries.contains(&game_exe_without_extension.as_str()))
        {
            show_notification(
                "Unsupported game",
                &format!("{} ({}) is not supported!", unsupported_game.name, game_exe),
                &format!("Reason: {}", unsupported_game.reason),
                NotificationType::Error,
            );
            return Ok(());
        }

        tracing::info!(
            game_exe,
            ?pid,
            ?hwnd,
            recording_location=%recording_location.display(),
            "Starting recording"
        );

        let recording = Recording::start(
            MetadataParameters {
                path: recording_location.join("metadata.json"),
                game_exe: game_exe.clone(),
            },
            WindowParameters {
                path: recording_location.join("recording.mp4"),
                pid,
                hwnd,
            },
            InputParameters {
                path: recording_location.join("inputs.csv"),
            },
        )
        .await;

        let recording = match recording {
            Ok(recording) => recording,
            Err(e) => {
                tracing::error!(game_exe=?game_exe, e=?e, "Failed to start recording");
                show_notification(
                    &format!("Failed to start recording for `{game_exe}`"),
                    &e.to_string(),
                    "",
                    NotificationType::Error,
                );
                return Ok(());
            }
        };

        show_notification(
            "Started recording",
            &format!("Recording `{}`", recording.game_exe()),
            "",
            NotificationType::Info,
        );

        self.recording = Some(recording);

        Ok(())
    }

    pub(crate) async fn seen_input(&mut self, e: input_capture::Event) -> Result<()> {
        let Some(recording) = self.recording.as_mut() else {
            return Ok(());
        };
        recording.seen_input(e).await?;
        Ok(())
    }

    pub(crate) async fn stop(&mut self) -> Result<()> {
        let Some(recording) = self.recording.take() else {
            return Ok(());
        };

        show_notification(
            "Stopped recording",
            &format!("No longer recording `{}`", recording.game_exe()),
            "",
            NotificationType::Info,
        );

        recording.stop().await?;

        Ok(())
    }
}

pub enum NotificationType {
    Info,
    Error,
}
fn show_notification(title: &str, text1: &str, text2: &str, notification_type: NotificationType) {
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
