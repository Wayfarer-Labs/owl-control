use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use color_eyre::{
    Result,
    eyre::{Context as _, OptionExt as _, bail},
};
use windows::Win32::Foundation::HWND;

use crate::{
    app_state::{AppState, RecordingStatus},
    config::RecordingBackend,
    record::{
        obs_embedded_recorder::ObsEmbeddedRecorder,
        obs_socket_recorder::ObsSocketRecorder,
        recording::{InputParameters, MetadataParameters, Recording, WindowParameters},
    },
    system::hardware_specs::get_primary_monitor_resolution,
    ui::notification::{NotificationType, show_notification},
};
use constants::{MIN_FREE_SPACE_MB, unsupported_games::UNSUPPORTED_GAMES};

#[async_trait::async_trait(?Send)]
pub trait VideoRecorder {
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
pub struct Recorder {
    recording_dir: Box<dyn FnMut() -> PathBuf>,
    recording: Option<Recording>,
    app_state: Arc<AppState>,
    video_recorder: Box<dyn VideoRecorder>,
}

impl Recorder {
    pub async fn new(
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

    pub fn recording(&self) -> Option<&Recording> {
        self.recording.as_ref()
    }

    pub async fn start(&mut self) -> Result<()> {
        if self.recording.is_some() {
            return Ok(());
        }

        let recording_location = (self.recording_dir)();

        std::fs::create_dir_all(&recording_location)
            .wrap_err("Failed to create directory for recording. Did you install OWL Control to a location where your account is allowed to write files?")?;

        let free_space_mb = get_free_space_in_mb(&recording_location);
        if let Some(free_space_mb) = free_space_mb
            && free_space_mb < MIN_FREE_SPACE_MB
        {
            bail!(
                "There is not enough free space on the disk to record. Please free up some space. Required: at least {MIN_FREE_SPACE_MB} MB, available: {free_space_mb} MB"
            );
        }

        let Some((game_exe, pid, hwnd)) =
            get_foregrounded_game().wrap_err("failed to get foregrounded game")?
        else {
            bail!(
                "You do not have a game window in focus. Please focus on a game window and try again."
            );
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
            bail!(
                "{} ({}) is not supported! Reason: {}",
                unsupported_game.name,
                game_exe,
                unsupported_game.reason
            );
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
                return Err(e);
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

    pub async fn seen_input(&mut self, e: input_capture::Event) -> Result<()> {
        let Some(recording) = self.recording.as_mut() else {
            return Ok(());
        };
        recording.seen_input(e).await?;
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
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

fn get_foregrounded_game() -> Result<Option<(String, game_process::Pid, HWND)>> {
    let (hwnd, pid) = game_process::foreground_window()?;

    let exe_path = game_process::exe_name_for_pid(pid)?;
    let exe_name = exe_path
        .file_name()
        .ok_or_eyre("Failed to get file name from exe path")?
        .to_str()
        .ok_or_eyre("Failed to convert exe name to unicode string")?
        .to_owned();

    Ok(Some((exe_name, pid, hwnd)))
}
