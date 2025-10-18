use std::path::Path;

use color_eyre::{
    Result,
    eyre::{Context, OptionExt as _, bail, eyre},
};
use constants::{FPS, RECORDING_HEIGHT, RECORDING_WIDTH};
use windows::Win32::Foundation::HWND;

use libobs_sources::{
    ObsSourceBuilder,
    windows::{GameCaptureSourceBuilder, ObsGameCaptureMode, WindowCaptureSourceBuilder},
};
use libobs_window_helper::WindowSearchMode;
use libobs_wrapper::{
    context::ObsContext,
    data::{output::ObsOutputRef, video::ObsVideoInfoBuilder},
    logger::ObsLogger,
    sources::ObsSourceRef,
    utils::{AudioEncoderInfo, ObsPath, OutputInfo, VideoEncoderInfo},
};

use crate::{
    config::VideoSettings,
    record::recorder::{VideoRecorder, get_recording_base_resolution},
};

const OWL_SCENE_NAME: &str = "owl_data_collection_scene";
const OWL_CAPTURE_NAME: &str = "owl_game_capture";

// Untested! Added for testing purposes, but will probably not be used as
// we want to ensure we're capturing a game and WindowCapture will capture
// non-game content.
const USE_WINDOW_CAPTURE: bool = false;

pub struct ObsEmbeddedRecorder {
    obs_context: ObsContext,
    adapter_index: usize,
    current_output: Option<ObsOutputRef>,
    source: Option<ObsSourceRef>,
}
impl ObsEmbeddedRecorder {
    pub async fn new(adapter_index: usize) -> Result<Self>
    where
        Self: Sized,
    {
        let obs_context = ObsContext::new(
            ObsContext::builder()
                .set_logger(Box::new(TracingObsLogger))
                .set_video_info(
                    ObsVideoInfoBuilder::new()
                        .adapter(adapter_index as u32)
                        .fps_num(FPS)
                        .fps_den(1)
                        .base_width(RECORDING_WIDTH)
                        .base_height(RECORDING_HEIGHT)
                        .output_width(RECORDING_WIDTH)
                        .output_height(RECORDING_HEIGHT)
                        .build(),
                ),
        )
        .await?;

        tracing::debug!("OBS context initialized successfully");
        Ok(Self {
            obs_context,
            adapter_index,
            current_output: None,
            source: None,
        })
    }
}
#[async_trait::async_trait(?Send)]
impl VideoRecorder for ObsEmbeddedRecorder {
    fn id(&self) -> &'static str {
        "ObsEmbedded"
    }

    async fn start_recording(
        &mut self,
        dummy_video_path: &Path,
        pid: u32,
        hwnd: HWND,
        game_exe: &str,
        video_settings: VideoSettings,
    ) -> Result<()> {
        let recording_path: &str = dummy_video_path
            .to_str()
            .ok_or_eyre("Recording path must be valid UTF-8")?;

        tracing::debug!("Starting recording with path: {}", recording_path);

        // Check if already recording
        if self.current_output.is_some() {
            bail!("Recording is already in progress");
        }

        // Set up scene and window capture based on input pid
        let mut scene = self.obs_context.scene(OWL_SCENE_NAME).await?;

        let (base_width, base_height) = get_recording_base_resolution(hwnd)?;
        tracing::info!("Base recording resolution: {base_width}x{base_height}");

        self.obs_context
            .reset_video(
                ObsVideoInfoBuilder::new()
                    .adapter(self.adapter_index as u32)
                    .fps_num(FPS)
                    .fps_den(1)
                    .base_width(base_width)
                    .base_height(base_height)
                    .output_width(RECORDING_WIDTH)
                    .output_height(RECORDING_HEIGHT)
                    .build(),
            )
            .await?;

        let source = if USE_WINDOW_CAPTURE {
            let window =
                WindowCaptureSourceBuilder::get_windows(WindowSearchMode::ExcludeMinimized)
                    .map_err(|e| eyre!(e))?;
            let window = window
                .iter()
                .find(|w| w.0.pid == pid)
                .ok_or_else(|| eyre!("We couldn't find a capturable window for this application (EXE: {game_exe}, PID: {pid}). Please ensure you are capturing a game."))?;

            self.obs_context
                .source_builder::<WindowCaptureSourceBuilder, _>(OWL_CAPTURE_NAME)
                .await?
                .set_window(window)
                .set_capture_audio(true)
                .set_client_area(false) // capture full screen. if this is set to true there's black borders around the window capture.
                .add_to_scene(&mut scene)
                .await?
        } else {
            let window = GameCaptureSourceBuilder::get_windows(WindowSearchMode::ExcludeMinimized)
                .map_err(|e| eyre!(e))?;
            let window = window
                .iter()
                .find(|w| w.pid == pid)
                .ok_or_else(|| eyre!("We couldn't find a capturable window for this application (EXE: {game_exe}, PID: {pid}). Please ensure you are capturing a game."))?;

            if GameCaptureSourceBuilder::is_window_in_use_by_other_instance(window.pid)? {
                bail!(
                    "The window you're trying to record ({game_exe}) is already being captured by another process. Do you have OBS or another instance of OWL Control open?\n\nNote that OBS is no longer required to use OWL Control - please close it if you have it running!",
                );
            }

            if !window.is_game {
                bail!(
                    "The window you're trying to record ({game_exe}) cannot be captured. Please ensure you are capturing a game."
                );
            }

            self.obs_context
                .source_builder::<GameCaptureSourceBuilder, _>(OWL_CAPTURE_NAME)
                .await?
                .set_capture_mode(ObsGameCaptureMode::CaptureSpecificWindow)
                .set_window(window)
                .set_capture_audio(true)
                .add_to_scene(&mut scene)
                .await?
        };

        // Register the source
        scene.set_to_channel(0).await?;

        // Set up output
        let mut output_settings = self.obs_context.data().await?;
        output_settings
            .set_string("path", ObsPath::new(recording_path).build())
            .await?;

        let output_info = OutputInfo::new("ffmpeg_muxer", "output", Some(output_settings), None);
        let mut output = self.obs_context.output(output_info).await?;

        // TODO: it seems that video encoder and audio encoder should only be created once, instead of new ones every time that recording starts.
        // Register the video encoder with encoder-specific settings
        let video_encoder_data = self.obs_context.data().await?;
        let video_encoder_settings = video_settings
            .apply_encoder_settings(video_encoder_data)
            .await?;

        // Get video handler and attach encoder to output
        let video_handler = self.obs_context.get_video_ptr().await?;
        output
            .video_encoder(
                VideoEncoderInfo::new(
                    video_settings.enc_type(),
                    "video_encoder",
                    Some(video_encoder_settings),
                    None,
                ),
                video_handler,
            )
            .await?;

        // Register the audio encoder
        let mut audio_settings = self.obs_context.data().await?;
        audio_settings.set_int("bitrate", 160).await?;

        let audio_info =
            AudioEncoderInfo::new("ffmpeg_aac", "audio_encoder", Some(audio_settings), None);

        let audio_handler = self.obs_context.get_audio_ptr().await?;
        output.audio_encoder(audio_info, 0, audio_handler).await?;

        output.start().await?;

        // Store the output and recording path
        self.current_output = Some(output);

        tracing::debug!("OBS recording started successfully");
        self.source = Some(source);
        Ok(())
    }

    async fn stop_recording(&mut self) -> Result<()> {
        tracing::debug!("Stopping OBS recording...");

        if let Some(mut output) = self.current_output.take() {
            output.stop().await.wrap_err("Failed to stop OBS output")?;
            if let Some(mut scene) = self.obs_context.get_scene(OWL_SCENE_NAME).await
                && let Some(source) = self.source.take()
            {
                scene.remove_source(&source).await?;
            }
            tracing::debug!("OBS recording stopped");
        } else {
            tracing::warn!("No active recording to stop");
        }

        Ok(())
    }
}

#[derive(Debug)]
struct TracingObsLogger;
impl ObsLogger for TracingObsLogger {
    fn log(&mut self, level: libobs_wrapper::enums::ObsLogLevel, msg: String) {
        use libobs_wrapper::enums::ObsLogLevel;
        match level {
            ObsLogLevel::Error => tracing::error!(target: "obs", "{msg}"),
            ObsLogLevel::Warning => tracing::warn!(target: "obs", "{msg}"),
            ObsLogLevel::Info => tracing::info!(target: "obs", "{msg}"),
            ObsLogLevel::Debug => tracing::debug!(target: "obs", "{msg}"),
        }
    }
}
