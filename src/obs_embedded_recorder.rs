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
    encoders::ObsVideoEncoderType,
    sources::ObsSourceRef,
    utils::{AudioEncoderInfo, ObsPath, OutputInfo, VideoEncoderInfo},
};

use crate::recorder::{VideoRecorder, get_recording_base_resolution};

const OWL_SCENE_NAME: &str = "owl_data_collection_scene";
const OWL_CAPTURE_NAME: &str = "owl_game_capture";

const VIDEO_BITRATE: u32 = 2500;

// Untested! Added for testing purposes, but will probably not be used as
// we want to ensure we're capturing a game and WindowCapture will capture
// non-game content.
const USE_WINDOW_CAPTURE: bool = false;

pub struct ObsEmbeddedRecorder {
    obs_context: ObsContext,
    current_output: Option<ObsOutputRef>,
    source: Option<ObsSourceRef>,
    last_recorded_pid: Option<u32>,
}
impl ObsEmbeddedRecorder {
    pub async fn new() -> Result<Self>
    where
        Self: Sized,
    {
        let obs_context = ObsContext::new(
            ObsContext::builder().set_video_info(
                ObsVideoInfoBuilder::new()
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
            current_output: None,
            source: None,
            last_recorded_pid: None,
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
        _game_exe: &str,
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
        let mut scene = if let Some(scene) = self.obs_context.get_scene(OWL_SCENE_NAME).await {
            scene
        } else {
            self.obs_context.scene(OWL_SCENE_NAME).await?
        };

        let (base_width, base_height) = get_recording_base_resolution(hwnd)?;
        tracing::info!("Base recording resolution: {base_width}x{base_height}");

        self.obs_context
            .reset_video(
                ObsVideoInfoBuilder::new()
                    .fps_num(FPS)
                    .fps_den(1)
                    .base_width(base_width)
                    .base_height(base_height)
                    .output_width(RECORDING_WIDTH)
                    .output_height(RECORDING_HEIGHT)
                    .build(),
            )
            .await?;

        // Check if we need to remove the source due to PID change, allowing later code to detect that
        // there is no more source and instantiate a new one with the changed PID.
        if let Some(source) = scene.get_source_mut(OWL_CAPTURE_NAME).await {
            match self.last_recorded_pid {
                Some(last_pid) if last_pid != pid => {
                    tracing::info!(
                        "PID changed from {} to {}, will recreate capture source",
                        last_pid,
                        pid
                    );
                    scene.remove_source(&source).await?;
                }
                None => {
                    tracing::warn!(
                        "Existing source found but no last PID recorded (wtf?) will recreate to be safe"
                    );
                    scene.remove_source(&source).await?;
                }
                _ => (), // PID's matching, don't need to do anything
            };
        }

        let source = match scene.get_source_mut(OWL_CAPTURE_NAME).await {
            Some(source) => {
                tracing::info!("Recording start, reusing previous source with same PID");
                source
            }
            None => {
                // capture source has not been initialised before, so create a new one here
                tracing::info!(
                    "Recording start, creating new capture source for PID {}",
                    pid
                );
                if USE_WINDOW_CAPTURE {
                    let window =
                        WindowCaptureSourceBuilder::get_windows(WindowSearchMode::ExcludeMinimized)
                            .map_err(|e| eyre!(e))?;
                    let window = window
                        .iter()
                        .find(|w| w.0.pid == pid)
                        .ok_or_else(|| eyre!("No window found with PID: {}", pid))?;

                    self.obs_context
                        .source_builder::<WindowCaptureSourceBuilder, _>(OWL_CAPTURE_NAME)
                        .await?
                        .set_window(window)
                        .set_capture_audio(true)
                        .set_client_area(false) // capture full screen. if this is set to true there's black borders around the window capture.
                        .add_to_scene(&mut scene)
                        .await?
                } else {
                    let window =
                        GameCaptureSourceBuilder::get_windows(WindowSearchMode::ExcludeMinimized)
                            .map_err(|e| eyre!(e))?;
                    let window = window
                        .iter()
                        .find(|w| w.pid == pid)
                        .ok_or_else(|| eyre!("No window found with PID: {}", pid))?;

                    if GameCaptureSourceBuilder::is_window_in_use_by_other_instance(window.pid)? {
                        bail!(
                            "The window ({}) you're trying to record is already being captured by another process. Do you have OBS or another instance of OWL Control open?",
                            window.full_exe
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
                }
            }
        };

        // Store the current PID for future comparisons
        self.last_recorded_pid = Some(pid);

        // Register the source
        scene.set_to_channel(0).await?;

        // Set up output
        let mut output_settings = self.obs_context.data().await?;
        output_settings
            .set_string("path", ObsPath::new(recording_path).build())
            .await?;

        let output_info = OutputInfo::new("ffmpeg_muxer", "output", Some(output_settings), None);
        let mut output = self.obs_context.output(output_info).await?;

        if output.get_video_encoders().await.is_empty() {
            // Register the video encoder
            let mut video_settings = self.obs_context.data().await?;
            video_settings
                .bulk_update()
                .set_int("bf", 2)
                .set_bool("psycho_aq", true)
                .set_bool("lookahead", true)
                .set_string("profile", "high")
                .set_string("preset", "hq")
                .set_string("rate_control", "cbr")
                .set_int("bitrate", VIDEO_BITRATE.into())
                .update()
                .await?;

            // Get video handler and attach encoder to output
            let video_handler = self.obs_context.get_video_ptr().await?;
            output
                .video_encoder(
                    VideoEncoderInfo::new(
                        ObsVideoEncoderType::OBS_X264,
                        "video_encoder",
                        Some(video_settings),
                        None,
                    ),
                    video_handler,
                )
                .await?;
        }

        // Register the audio encoder
        if output.audio_encoders().read().await.is_empty() {
            let mut audio_settings = self.obs_context.data().await?;
            audio_settings.set_int("bitrate", 160).await?;

            let audio_info =
                AudioEncoderInfo::new("ffmpeg_aac", "audio_encoder", Some(audio_settings), None);

            let audio_handler = self.obs_context.get_audio_ptr().await?;
            output.audio_encoder(audio_info, 0, audio_handler).await?;
        }

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
            tracing::debug!("OBS recording stopped");
        } else {
            tracing::warn!("No active recording to stop");
        }

        Ok(())
    }
}
