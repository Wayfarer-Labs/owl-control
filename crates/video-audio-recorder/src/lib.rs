use std::{
    path::Path,
    sync::Arc,
};

use color_eyre::{
    Result,
    eyre::{Context, OptionExt as _, eyre},
};
use tokio::
    sync::{Mutex, OnceCell}
;

use libobs_sources::windows::{MonitorCaptureSourceBuilder, WindowCaptureSourceBuilder};
use libobs_window_helper::{WindowSearchMode};
use libobs_sources::ObsSourceBuilder;
use libobs_wrapper::{context::{ObsContext, ObsContextReturn}, data::output::ObsOutputRef, encoders::ObsVideoEncoderType, utils::VideoEncoderInfo};
use libobs_wrapper::utils::{AudioEncoderInfo, ObsPath, OutputInfo, StartupInfo};

static WINDOW_RECORDER: OnceCell<WindowRecorder> = OnceCell::const_new();

pub struct WindowRecorder {
    obs_context: Arc<Mutex<ObsContext>>,
    current_output: Arc<Mutex<Option<ObsOutputRef>>>,
    recording_path: Arc<Mutex<Option<String>>>,
}

/// WindowRecorder is a singleton (yes, shoot me if you care) that manages the OBS context and recordings.
/// Why a singleton? Well libobs is buggy asf and doesn't like being initialized multiple times,
/// previous implementation attempting to reinitialize the context always crashed at the first rerecord attempt.
/// Instead we initialize a single ObsContext when first called and then reuse it for all future
/// ObsOutput constructions, and it hasn't broken yet so that's a good sign.
impl WindowRecorder {
    pub async fn instance() -> Result<&'static WindowRecorder> {
        WINDOW_RECORDER.get_or_try_init(|| async {
            Self::new().await
        }).await
    }
    
    async fn new() -> Result<Self> {
        let startup_info = StartupInfo::default();
        let context = ObsContext::new(startup_info).await?;
        let context = match context {
            ObsContextReturn::Done(c) => c,
            ObsContextReturn::Restart => {
                return Err(eyre!("OBS restart required during initialization"));
            }
        };

        tracing::debug!("OBS context initialized successfully");

        Ok(WindowRecorder {
            obs_context: Arc::new(Mutex::new(context)),
            current_output: Arc::new(Mutex::new(None)),
            recording_path: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn start_recording(&self, path: &Path, _pid: u32, _hwnd: usize) -> Result<()> {
        let recording_path: &str = path.to_str()
            .ok_or_eyre("Recording path must be valid UTF-8")?;

        tracing::debug!("Starting recording with path: {}", recording_path);

        // Check if already recording
        {
            let current_output = self.current_output.lock().await;
            if current_output.is_some() {
                return Err(eyre!("Recording is already in progress"));
            }
        }

        let mut context = self.obs_context.lock().await;
        
        // Set up scene and window capture based on input pid
        let mut scene = context.scene("main").await?;

        // TODO: for some reason the window capture results in recording with wrong resolution? The application capture is smaller than the window size, resulting in black borders, whereas monitor capture works fine?

        // let window = WindowCaptureSourceBuilder::get_windows(WindowSearchMode::ExcludeMinimized).map_err(|e| eyre!(e))?;
        // let window = window.iter().find(|w| w.pid == _pid).ok_or_else(|| eyre!("No window found with PID: {}", _pid))?;
        
        // let mut _window_capture = context.source_builder::<WindowCaptureSourceBuilder, _>("window_capture")
        //     .await?
        //     .set_window(window)
        //     .set_capture_audio(true)
        //     .set_client_area(true)
        //     .add_to_scene(&mut scene)
        //     .await?;

        let monitors = MonitorCaptureSourceBuilder::get_monitors().map_err(|e| eyre!(e))?;
        let mut _monitor_capture = context
            .source_builder::<MonitorCaptureSourceBuilder, _>("Monitor Capture")
            .await?
            .set_monitor(&monitors[0])
            .add_to_scene(&mut scene)
            .await?;

        // Register the source
        scene.set_to_channel(0).await?;

        // Set up output
        let mut output_settings = context.data().await?;
        output_settings
            .set_string("path", ObsPath::new(recording_path).build())
            .await?;

        let output_info = OutputInfo::new("ffmpeg_muxer", "output", Some(output_settings), None);
        let mut output = context.output(output_info).await?;

        // Register the video encoder
        let mut video_settings = context.data().await?;
        video_settings
            .bulk_update()
            .set_int("bf", 2)
            .set_bool("psycho_aq", true)
            .set_bool("lookahead", true)
            .set_string("profile", "high")
            .set_string("preset", "hq")
            .set_string("rate_control", "cbr")
            .set_int("bitrate", 10000)
            .update()
            .await?;

        // Get video handler and attach encoder to output
        let video_handler = context.get_video_ptr().await?;
        output.video_encoder(
            VideoEncoderInfo::new(ObsVideoEncoderType::OBS_X264, "video_encoder", 
                                Some(video_settings), None),
            video_handler
        ).await?;

        // Register the audio encoder
        let mut audio_settings = context.data().await?;
        audio_settings.set_int("bitrate", 160).await?;

        let audio_info =
            AudioEncoderInfo::new("ffmpeg_aac", "audio_encoder", Some(audio_settings), None);

        let audio_handler = context.get_audio_ptr().await?;
        output.audio_encoder(audio_info, 0, audio_handler).await?;

        output.start().await?;

        // Store the output and recording path
        *self.current_output.lock().await = Some(output);
        *self.recording_path.lock().await = Some(recording_path.to_string());

        tracing::debug!("OBS recording started successfully");
        Ok(())
    }

    pub async fn is_recording(&self) -> bool {
        self.current_output.lock().await.is_some()
    }

    /// Get the current recording path
    pub async fn get_recording_path(&self) -> Option<String> {
        self.recording_path.lock().await.clone()
    }

    pub async fn stop_recording(&self) -> Result<()> {
        tracing::debug!("Stopping OBS recording...");
        let mut current_output = self.current_output.lock().await;
        if let Some(mut output) = current_output.take() {
            output.stop().await.wrap_err("Failed to stop OBS output")?;
            tracing::debug!("OBS recording stopped");
        } else {
            tracing::warn!("No active recording to stop");
        }
        // Clear the recording path
        *self.recording_path.lock().await = None;
        Ok(())
    }
}

impl WindowRecorder {
    /// Static convenience methods for accessing the singleton
    pub async fn start_recording_static(path: &Path, pid: u32, hwnd: usize) -> Result<()> {
        let recorder = Self::instance().await?;
        recorder.start_recording(path, pid, hwnd).await
    }

    pub async fn stop_recording_static() -> Result<()> {
        let recorder = Self::instance().await?;
        recorder.stop_recording().await
    }

    pub async fn is_recording_static() -> Result<bool> {
        let recorder = Self::instance().await?;
        Ok(recorder.is_recording().await)
    }
}
