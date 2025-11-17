use std::{
    path::PathBuf,
    time::{Instant, SystemTime},
};

use color_eyre::{Result, eyre::ContextCompat};
use egui_wgpu::wgpu;
use game_process::{Pid, windows::Win32::Foundation::HWND};
use input_capture::InputCapture;

use crate::{
    config::EncoderSettings,
    record::{
        input_recorder::{InputEventStream, InputEventWriter},
        recorder::VideoRecorder,
    },
    system::hardware_specs,
};

use super::local_recording::LocalRecording;

pub(crate) struct Recording {
    input_writer: InputEventWriter,
    input_stream: InputEventStream,

    recording_location: PathBuf,
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
        video_settings: EncoderSettings,
        input_capture: &InputCapture,
    ) -> Result<Self> {
        let start_time = SystemTime::now();
        let start_instant = Instant::now();

        let game_resolution = get_recording_base_resolution(hwnd)?;
        tracing::info!("Game resolution: {game_resolution:?}");

        let video_path = recording_location.join(constants::filename::recording::VIDEO);
        let csv_path = recording_location.join(constants::filename::recording::INPUTS);

        let (input_writer, input_stream) =
            InputEventWriter::start(&csv_path, input_capture).await?;
        video_recorder
            .start_recording(
                &video_path,
                pid.0,
                hwnd,
                &game_exe,
                video_settings,
                game_resolution,
                input_stream.clone(),
            )
            .await?;

        Ok(Self {
            input_writer,
            input_stream,
            recording_location,
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

    pub(crate) fn game_resolution(&self) -> (u32, u32) {
        self.game_resolution
    }

    pub(crate) fn get_window_name(&self) -> Option<String> {
        use game_process::windows::Win32::UI::WindowsAndMessaging::{
            GetWindowTextLengthW, GetWindowTextW,
        };

        let title_len = unsafe { GetWindowTextLengthW(self.hwnd) };
        if title_len > 0 {
            let mut buf = vec![0u16; (title_len + 1) as usize];
            let copied = unsafe { GetWindowTextW(self.hwnd, &mut buf) };
            if copied > 0 {
                if let Some(end) = buf.iter().position(|&c| c == 0) {
                    return Some(String::from_utf16_lossy(&buf[..end]));
                } else {
                    return Some(String::from_utf16_lossy(&buf));
                }
            }
        }
        None
    }

    pub(crate) fn input_stream(&self) -> &InputEventStream {
        &self.input_stream
    }

    /// Flush all pending input events to disk
    pub(crate) async fn flush_input_events(&mut self) -> Result<()> {
        self.input_writer.flush().await
    }

    pub(crate) async fn stop(
        self,
        recorder: &mut dyn VideoRecorder,
        adapter_infos: &[wgpu::AdapterInfo],
        input_capture: &InputCapture,
    ) -> Result<()> {
        let window_name = self.get_window_name();
        let result = recorder.stop_recording().await;
        self.input_writer.stop(input_capture).await?;

        if let Err(e) = result {
            tracing::error!("Error while stopping recording, invalidating recording: {e}");
            tokio::fs::write(
                self.recording_location
                    .join(constants::filename::recording::INVALID),
                e.to_string(),
            )
            .await?;
        } else {
            let gamepads = input_capture.gamepads();
            LocalRecording::write_metadata_and_validate(
                self.recording_location,
                self.game_exe,
                self.game_resolution,
                self.start_instant,
                self.start_time,
                window_name,
                adapter_infos,
                gamepads,
                recorder.id(),
                result.as_ref().ok().cloned(),
            )
            .await?;
        }

        Ok(())
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
