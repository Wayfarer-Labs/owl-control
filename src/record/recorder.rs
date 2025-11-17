use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use color_eyre::{
    Result,
    eyre::{Context as _, OptionExt as _, bail},
};
use egui_wgpu::wgpu::DeviceType;
use input_capture::InputCapture;
use windows::Win32::Foundation::HWND;

use crate::{
    app_state::{AppState, RecordingStatus},
    config::{EncoderSettings, RecordingBackend},
    output_types::InputEventType,
    record::{
        LocalRecording, input_recorder::InputEventStream,
        obs_embedded_recorder::ObsEmbeddedRecorder, obs_socket_recorder::ObsSocketRecorder,
        recording::Recording,
    },
};
use constants::{
    MIN_FREE_SPACE_MB, encoding::VideoEncoderType, unsupported_games::UnsupportedGames,
};

#[async_trait::async_trait(?Send)]
pub trait VideoRecorder {
    fn id(&self) -> &'static str;
    fn available_encoders(&self) -> &[VideoEncoderType];

    #[allow(clippy::too_many_arguments)]
    async fn start_recording(
        &mut self,
        dummy_video_path: &Path,
        pid: u32,
        hwnd: HWND,
        game_exe: &str,
        video_settings: EncoderSettings,
        game_resolution: (u32, u32),
        event_stream: InputEventStream,
    ) -> Result<()>;
    /// Result contains any additional metadata the recorder wants to return about the recording
    /// If this returns an error, the recording will be invalidated with the error message
    async fn stop_recording(&mut self) -> Result<serde_json::Value>;
    /// Called periodically for any work the recorder might need to do
    async fn poll(&mut self);
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

        // Incredibly ugly hack: assume that the first dGPU is the one we want,
        // and that this list agrees with OBS's. There's no real guarantee that
        // this is the case, and that the target game is even running on the dGPU,
        // but it's a first-pass solution for now.
        //
        // TODO: Investigate what OBS actually does here. I spent over an hour
        // pouring through the OBS source code and couldn't find anything of
        // note with regards to how it chooses the adapter; I might have to
        // reach out to an OBS developer if this becomes an issue again.
        let adapter_index = app_state
            .adapter_infos
            .iter()
            .position(|a| a.device_type == DeviceType::DiscreteGpu)
            .unwrap_or_default();

        tracing::info!(
            "Initializing recorder with adapter index {adapter_index} ({:?})",
            app_state.adapter_infos[adapter_index]
        );

        let video_recorder: Box<dyn VideoRecorder> = match backend {
            RecordingBackend::Embedded => Box::new(ObsEmbeddedRecorder::new(adapter_index).await?),
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

    pub fn available_video_encoders(&self) -> &[VideoEncoderType] {
        self.video_recorder.available_encoders()
    }

    pub async fn start(
        &mut self,
        input_capture: &InputCapture,
        unsupported_games: &UnsupportedGames,
    ) -> Result<()> {
        if self.recording.is_some() {
            return Ok(());
        }

        let recording_location = (self.recording_dir)();

        LocalRecording::create_at(&recording_location)
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
        if let Some(unsupported_game) = unsupported_games.get(game_exe_without_extension) {
            bail!(
                "{} ({}) is not supported! Reason: {}",
                unsupported_game.name,
                game_exe,
                unsupported_game.reason
            );
        }

        if let Err(error) = is_process_game_shaped(pid) {
            bail!(
                "This application ({game_exe}) doesn't look like a game. Please contact us if you think this is a mistake. Error: {error}"
            );
        }

        tracing::info!(
            game_exe,
            ?pid,
            ?hwnd,
            recording_location=%recording_location.display(),
            "Starting recording"
        );

        let video_settings = self
            .app_state
            .config
            .read()
            .unwrap()
            .preferences
            .encoder
            .clone();

        let recording = Recording::start(
            self.video_recorder.as_mut(),
            recording_location.clone(),
            game_exe.clone(),
            pid,
            hwnd,
            video_settings,
            input_capture,
        )
        .await;

        let recording = match recording {
            Ok(recording) => recording,
            Err(e) => {
                tracing::error!(game_exe=?game_exe, e=?e, "Failed to start recording");
                return Err(e);
            }
        };

        self.recording = Some(recording);
        *self.app_state.state.write().unwrap() = RecordingStatus::Recording {
            start_time: Instant::now(),
            game_exe,
        };
        Ok(())
    }

    pub async fn seen_input(&mut self, e: input_capture::Event) -> Result<()> {
        let Some(recording) = self.recording.as_ref() else {
            return Ok(());
        };
        recording
            .input_stream()
            .send(InputEventType::from_input_event(e)?)?;
        Ok(())
    }

    /// Flush all pending input events to disk
    pub async fn flush_input_events(&mut self) -> Result<()> {
        let Some(recording) = self.recording.as_mut() else {
            return Ok(());
        };
        recording.flush_input_events().await
    }

    pub async fn stop(&mut self, input_capture: &InputCapture) -> Result<()> {
        let Some(recording) = self.recording.take() else {
            return Ok(());
        };

        recording
            .stop(
                self.video_recorder.as_mut(),
                &self.app_state.adapter_infos,
                input_capture,
            )
            .await?;
        *self.app_state.state.write().unwrap() = RecordingStatus::Stopped;

        tracing::info!("Recording stopped");
        Ok(())
    }

    pub async fn poll(&mut self) {
        self.video_recorder.poll().await;
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

pub fn get_foregrounded_game() -> Result<Option<(String, game_process::Pid, HWND)>> {
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

fn is_process_game_shaped(pid: game_process::Pid) -> Result<()> {
    // We've seen reports of this failing with certain games (e.g. League of Legends),
    // so this "fails safe" for now. It's possible that we don't actually want to
    // capture any games that this would be tripped up by, but it's hard to say that
    // without more evidence. I would assume the primary factor involved here is
    // the presence of an anticheat or an antitamper that obscures the retrieval of modules.
    match game_process::get_modules(pid) {
        Ok(modules) => {
            let mut has_graphics_api = false;
            for module in modules {
                let module = module.to_lowercase();

                // Check for Direct3D DLLs
                if module.contains("d3d")
                    || module.contains("dxgi")
                    || module.contains("d3d11")
                    || module.contains("d3d12")
                    || module.contains("d3d9")
                {
                    has_graphics_api = true;
                }

                // Check for OpenGL DLLs
                if module.contains("opengl32")
                    || module.contains("gdi32")
                    || module.contains("glu32")
                    || module.contains("opengl")
                {
                    has_graphics_api = true;
                }

                // Check for Vulkan DLLs
                if module.contains("vulkan")
                    || module.contains("vulkan-1")
                    || module.contains("vulkan32")
                {
                    has_graphics_api = true;
                }
            }

            if !has_graphics_api {
                bail!(
                    "this application doesn't use any graphics APIs (DirectX, OpenGL, or Vulkan)"
                );
            }
        }
        Err(e) => {
            tracing::warn!(?e, pid=?pid, "Failed to get modules for process");
        }
    }

    Ok(())
}
