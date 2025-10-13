use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use color_eyre::eyre::OptionExt as _;
use color_eyre::{Result, eyre::Context as _};
use tauri_winrt_notification::Toast;
use windows::Win32::Foundation::HWND;
use windows::{
    Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MessageBoxW},
    core::HSTRING,
};

use crate::hardware_specs::get_primary_monitor_resolution;
use crate::{
    app_state::{AppState, RecordingStatus},
    config::RecordingBackend,
    find_game::get_foregrounded_game,
    obs_embedded_recorder::ObsEmbeddedRecorder,
    obs_socket_recorder::ObsSocketRecorder,
    recording::{InputParameters, MetadataParameters, Recording, WindowParameters},
};
use constants::{MIN_FREE_SPACE_MB, unsupported_games::UNSUPPORTED_GAMES};

#[async_trait::async_trait(?Send)]
pub(crate) trait VideoRecorder {
    fn id(&self) -> &'static str;

    async fn start_recording(
        &mut self,
        dummy_video_path: &Path,
        pid: u32,
        hwnd: HWND,
        game_exe: &str,
    ) -> Result<()>;
    async fn stop_recording(&mut self) -> Result<()>;
}
pub(crate) struct Recorder {
    recording_dir: Box<dyn FnMut() -> PathBuf>,
    recording: Option<Recording>,
    app_state: Arc<AppState>,
    video_recorder: Box<dyn VideoRecorder>,
}

impl Recorder {
    pub(crate) async fn new(
        recording_dir: Box<dyn FnMut() -> PathBuf>,
        app_state: Arc<AppState>,
    ) -> Result<Self> {
        let backend = app_state
            .config
            .read()
            .unwrap()
            .preferences
            .recording_backend;

        let video_recorder: Box<dyn VideoRecorder> = match backend {
            RecordingBackend::Embedded => Box::new(ObsEmbeddedRecorder::new().await?),
            RecordingBackend::Socket => Box::new(ObsSocketRecorder::new().await?),
        };

        tracing::info!("Using {} as video recorder", video_recorder.id());
        Ok(Self {
            recording_dir,
            recording: None,
            app_state,
            video_recorder,
        })
    }

    pub(crate) fn recording(&self) -> Option<&Recording> {
        self.recording.as_ref()
    }

    pub(crate) async fn start(&mut self) -> Result<()> {
        if self.recording.is_some() {
            return Ok(());
        }

        let recording_location = (self.recording_dir)();

        std::fs::create_dir_all(&recording_location)
            .wrap_err("Failed to create recording directory")?;

        let free_space_mb = get_free_space_in_mb(&recording_location);
        if let Some(free_space_mb) = free_space_mb
            && free_space_mb < MIN_FREE_SPACE_MB
        {
            show_notification(
                "Not enough free space",
                "There is not enough free space on the disk to record. Please free up some space.",
                &format!(
                    "Required: at least {MIN_FREE_SPACE_MB} MB, available: {free_space_mb} MB"
                ),
                NotificationType::Error,
            );
            return Ok(());
        }

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
            self.video_recorder.as_mut(),
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
            &format!("Recording `{game_exe}`"),
            "",
            NotificationType::Info,
        );

        self.recording = Some(recording);
        *self.app_state.state.write().unwrap() = RecordingStatus::Recording {
            start_time: Instant::now(),
            game_exe,
        };
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

        recording.stop(self.video_recorder.as_mut()).await?;
        *self.app_state.state.write().unwrap() = RecordingStatus::Stopped;
        Ok(())
    }
}

fn get_free_space_in_mb(path: &std::path::Path) -> Option<u64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let path = dunce::canonicalize(path).ok()?;

    // Find the disk with the longest matching mount point
    disks
        .iter()
        .filter(|disk| path.starts_with(disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space() / 1024 / 1024)
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

pub fn get_recording_base_resolution(hwnd: HWND) -> Result<(u32, u32)> {
    use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::GetClientRect};

    /// Returns the size (width, height) of the inner area of a window given its HWND.
    /// Returns None if the window does not exist or the call fails.
    fn get_window_inner_size(hwnd: HWND) -> Option<(u32, u32)> {
        unsafe {
            let mut rect = RECT::default();
            GetClientRect(hwnd, &mut rect).ok()?;
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            Some((width as u32, height as u32))
        }
    }

    match get_window_inner_size(hwnd) {
        Some(size) => Ok(size),
        None => {
            tracing::info!("Failed to get window inner size, using primary monitor resolution");
            get_primary_monitor_resolution().ok_or_eyre("Failed to get primary monitor resolution")
        }
    }
}
