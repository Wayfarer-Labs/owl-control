use std::path::PathBuf;

use color_eyre::{Result, eyre::Context as _};
use tauri_winrt_notification::Toast;
#[cfg(feature = "real-video")]
use video_audio_recorder::WindowRecorder;

use crate::{
    find_game::get_foregrounded_game,
    recording::{InputParameters, MetadataParameters, Recording, WindowParameters},
};

pub(crate) struct Recorder {
    recording_dir: PathBuf,
    recording: Option<Recording>,
}

impl Recorder {
    pub(crate) async fn new(recording_dir: PathBuf) -> Result<Self> {
        // Initialize the OBS bootstrapper as early as possible
        #[cfg(feature = "real-video")]
        WindowRecorder::instance().await?;

        Ok(Self {
            recording_dir,
            recording: None,
        })
    }

    pub(crate) fn recording(&self) -> Option<&Recording> {
        self.recording.as_ref()
    }

    pub(crate) async fn start(&mut self) -> Result<()> {
        if self.recording.is_some() {
            return Ok(());
        }

        let recording_location = &self.recording_dir;

        std::fs::create_dir_all(recording_location)
            .wrap_err("Failed to create recording directory")?;

        let Some((game_exe, pid, hwnd)) =
            get_foregrounded_game().wrap_err("failed to get foregrounded game")?
        else {
            tracing::warn!("No game window found");
            Self::show_invalid_game_notification();
            return Ok(());
        };

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
                game_exe,
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
        .await?;

        Self::show_start_notification(recording.game_exe());

        self.recording = Some(recording);

        Ok(())
    }

    pub(crate) async fn seen_input(&mut self, e: raw_input::Event) -> Result<()> {
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

        Self::show_stop_notification(recording.game_exe());

        recording.stop().await?;

        Ok(())
    }

    fn show_start_notification(exe_name: &str) {
        if let Err(e) = Toast::new(Toast::POWERSHELL_APP_ID)
            .title("Started recording")
            .text1(&format!("Recording {exe_name}"))
            .sound(None)
            .show()
        {
            tracing::error!("Failed to show start notification: {e}");
        };
    }

    fn show_invalid_game_notification() {
        if let Err(e) = Toast::new(Toast::POWERSHELL_APP_ID)
            .title("Invalid game")
            .text1(&format!("Not recording foreground window."))
            .text2("It's either not a supported game or not fullscreen.")
            .sound(None)
            .show()
        {
            tracing::error!("Failed to show invalid game notification: {e}");
        };
    }

    fn show_stop_notification(exe_name: &str) {
        if let Err(e) = Toast::new(Toast::POWERSHELL_APP_ID)
            .title("Stopped recording")
            .text1(&format!("No longer recording {exe_name}"))
            .sound(None)
            .show()
        {
            tracing::error!("Failed to show stop notification: {e}");
        };
    }
}
