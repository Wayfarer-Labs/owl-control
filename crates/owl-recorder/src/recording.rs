use std::{
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use color_eyre::Result;
use game_process::{Pid, windows::Win32::Foundation::HWND};
use serde::Serialize;

use crate::{
    hardware_id, hardware_specs, input_recorder::InputRecorder, window_recorder::WindowRecorder,
};

pub(crate) struct Recording {
    window_recorder: WindowRecorder,
    input_recorder: InputRecorder,

    metadata_path: PathBuf,
    game_exe: String,
    start_time: SystemTime,
    start_instant: Instant,

    pid: Pid,
    hwnd: HWND,
}

pub(crate) struct MetadataParameters {
    pub(crate) path: PathBuf,
    pub(crate) game_exe: String,
}

pub(crate) struct WindowParameters {
    pub(crate) path: PathBuf,
    pub(crate) pid: Pid,
    pub(crate) hwnd: HWND,
}

pub(crate) struct InputParameters {
    pub(crate) path: PathBuf,
}

impl Recording {
    pub(crate) async fn start(
        MetadataParameters {
            path: metadata_path,
            game_exe,
        }: MetadataParameters,
        WindowParameters {
            path: video_path,
            pid,
            hwnd,
        }: WindowParameters,
        InputParameters { path: csv_path }: InputParameters,
    ) -> Result<Self> {
        let start_time = SystemTime::now();
        let start_instant = Instant::now();

        let window_recorder =
            WindowRecorder::start_recording(&video_path, pid.0, hwnd.0.expose_provenance()).await?;
        let input_recorder = InputRecorder::start(&csv_path).await?;

        Ok(Self {
            window_recorder,
            input_recorder,
            metadata_path,
            game_exe,
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

    pub(crate) async fn stop(self) -> Result<()> {
        self.window_recorder.stop_recording().await?;
        self.input_recorder.stop().await?;

        Self::write_metadata(
            &self.metadata_path,
            self.game_exe,
            self.start_instant,
            self.start_time,
        )
        .await?;
        Ok(())
    }

    async fn write_metadata(
        path: &Path,
        game_exe: String,
        start_instant: Instant,
        start_time: SystemTime,
    ) -> Result<()> {
        let metadata = Self::final_metadata(game_exe, start_instant, start_time).await?;
        let metadata = serde_json::to_string_pretty(&metadata)?;
        tokio::fs::write(path, &metadata).await?;
        Ok(())
    }

    async fn final_metadata(
        game_exe: String,
        start_instant: Instant,
        start_time: SystemTime,
    ) -> Result<Metadata> {
        let duration = start_instant.elapsed().as_secs_f32();

        let start_timestamp = start_time.duration_since(UNIX_EPOCH).unwrap().as_secs();
        let end_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let hardware_id = hardware_id::get()?;

        let hardware_specs = match hardware_specs::get_hardware_specs() {
            Ok(specs) => Some(specs),
            Err(e) => {
                tracing::warn!("Failed to get hardware specs: {}", e);
                None
            }
        };

        Ok(Metadata {
            game_exe,
            session_id: uuid::Uuid::new_v4().to_string(),
            hardware_id,
            hardware_specs,
            start_timestamp,
            end_timestamp,
            duration,
        })
    }
}

#[derive(Serialize)]
struct Metadata {
    game_exe: String,
    session_id: String,
    hardware_id: String,
    hardware_specs: Option<hardware_specs::HardwareSpecs>,
    start_timestamp: u64,
    end_timestamp: u64,
    duration: f32,
}
