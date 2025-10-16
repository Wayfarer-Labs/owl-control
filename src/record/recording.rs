use std::{
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use color_eyre::{Result, eyre::ContextCompat};
use egui_wgpu::wgpu;
use game_process::{Pid, windows::Win32::Foundation::HWND};

use crate::{
    output_types::Metadata,
    record::{input_recorder::InputRecorder, recorder::VideoRecorder},
    system::{hardware_id, hardware_specs},
};

pub(crate) struct Recording {
    input_recorder: InputRecorder,

    recording_location: PathBuf,
    metadata_path: PathBuf,
    game_exe: String,
    game_resolution: (u32, u32),
    start_time: SystemTime,
    start_instant: Instant,

    pid: Pid,
    hwnd: HWND,
}

impl Recording {
    pub(crate) async fn start(
        video_recorder: &mut dyn VideoRecorder,
        recording_location: PathBuf,
        game_exe: String,
        pid: Pid,
        hwnd: HWND,
    ) -> Result<Self> {
        let start_time = SystemTime::now();
        let start_instant = Instant::now();

        let game_resolution = get_recording_base_resolution(hwnd)?;
        tracing::info!("Game resolution: {game_resolution:?}");

        let metadata_path = recording_location.join(constants::filename::recording::METADATA);
        let video_path = recording_location.join(constants::filename::recording::VIDEO);
        let csv_path = recording_location.join(constants::filename::recording::INPUTS);

        video_recorder
            .start_recording(&video_path, pid.0, hwnd, &game_exe, game_resolution)
            .await?;
        let input_recorder = InputRecorder::start(&csv_path).await?;

        Ok(Self {
            input_recorder,
            recording_location,
            metadata_path,
            game_exe,
            game_resolution,
            start_time,
            start_instant,

            pid,
            hwnd,
        })
    }

    #[allow(dead_code)]
    pub(crate) fn game_exe(&self) -> &str {
        &self.game_exe
    }

    #[allow(dead_code)]
    pub(crate) fn start_time(&self) -> SystemTime {
        self.start_time
    }

    #[allow(dead_code)]
    pub(crate) fn start_instant(&self) -> Instant {
        self.start_instant
    }

    #[allow(dead_code)]
    pub(crate) fn elapsed(&self) -> std::time::Duration {
        self.start_instant.elapsed()
    }

    #[allow(dead_code)]
    pub(crate) fn pid(&self) -> Pid {
        self.pid
    }

    #[allow(dead_code)]
    pub(crate) fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub(crate) async fn seen_input(&mut self, e: input_capture::Event) -> Result<()> {
        self.input_recorder.seen_input(e).await
    }

    pub(crate) async fn write_focus(&mut self, focused: bool) -> Result<()> {
        // write alt tab status to the input tracker
        self.input_recorder.write_focus(focused).await
    }

    pub(crate) async fn stop(
        self,
        recorder: &mut dyn VideoRecorder,
        adapter_infos: &[wgpu::AdapterInfo],
    ) -> Result<()> {
        let result = recorder.stop_recording().await;
        self.input_recorder.stop().await?;

        let metadata = Self::final_metadata(
            self.game_exe,
            self.game_resolution,
            self.start_instant,
            self.start_time,
            adapter_infos,
        )
        .await?;
        let metadata = serde_json::to_string_pretty(&metadata)?;
        tokio::fs::write(&self.metadata_path, &metadata).await?;

        if let Err(e) = result {
            tracing::error!("Error while stopping recording, invalidating recording: {e}");
            tokio::fs::write(
                self.recording_location
                    .join(constants::filename::recording::INVALID),
                e.to_string(),
            )
            .await?;
        }

        Ok(())
    }

    async fn final_metadata(
        game_exe: String,
        game_resolution: (u32, u32),
        start_instant: Instant,
        start_time: SystemTime,
        adapter_infos: &[wgpu::AdapterInfo],
    ) -> Result<Metadata> {
        let duration = start_instant.elapsed().as_secs_f32();

        let start_timestamp = start_time.duration_since(UNIX_EPOCH).unwrap().as_secs();
        let end_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let hardware_id = hardware_id::get()?;

        let hardware_specs = match hardware_specs::get_hardware_specs(
            adapter_infos
                .iter()
                .map(|a| hardware_specs::GpuSpecs::from_name(&a.name))
                .collect(),
        ) {
            Ok(specs) => Some(specs),
            Err(e) => {
                tracing::warn!("Failed to get hardware specs: {}", e);
                None
            }
        };

        Ok(Metadata {
            game_exe,
            game_resolution: Some(game_resolution),
            owl_control_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            owl_control_commit: Some(
                git_version::git_version!(
                    args = ["--abbrev=40", "--always", "--dirty=-modified"],
                    fallback = "unknown"
                )
                .to_string(),
            ),
            session_id: uuid::Uuid::new_v4().to_string(),
            hardware_id,
            hardware_specs,
            start_timestamp,
            end_timestamp,
            duration,
            input_stats: None,
        })
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
            hardware_specs::get_primary_monitor_resolution()
                .context("Failed to get primary monitor resolution")
        }
    }
}
