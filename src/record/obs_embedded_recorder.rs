use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use color_eyre::{
    Result,
    eyre::{self, Context, OptionExt as _, bail, eyre},
};
use constants::{FPS, RECORDING_HEIGHT, RECORDING_WIDTH, encoding::VideoEncoderType};
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
    logger::ObsLogger,
    scenes::ObsSceneRef,
    sources::ObsSourceRef,
    utils::{AudioEncoderInfo, ObsPath, OutputInfo, VideoEncoderInfo},
};

use crate::{config::EncoderSettings, record::recorder::VideoRecorder};

const OWL_SCENE_NAME: &str = "owl_data_collection_scene";
const OWL_CAPTURE_NAME: &str = "owl_game_capture";

// Untested! Added for testing purposes, but will probably not be used as
// we want to ensure we're capturing a game and WindowCapture will capture
// non-game content.
const USE_WINDOW_CAPTURE: bool = false;

pub struct ObsEmbeddedRecorder {
    adapter_index: usize,
    last_encoder_settings: Option<serde_json::Value>,
    hook_successful: Arc<AtomicBool>,
    obs_data: Option<ObsData>,
}
impl Drop for ObsEmbeddedRecorder {
    fn drop(&mut self) {
        if let Some(obs_data) = self.obs_data.take() {
            // Move the ObsData into a blocking context,
            // and let it detach, so that it can drop in its own time
            tokio::task::spawn_blocking(move || {
                std::mem::drop(obs_data);
            });
        }
    }
}
// Separate struct that can be dropped in a Tokio-blocking context
// as to avoid blocking a Tokio thread on drop
struct ObsData {
    obs_context: ObsContext,
    current_output: Option<ObsOutputRef>,
    source: Option<ObsSourceRef>,
}
impl ObsEmbeddedRecorder {
    pub async fn new(adapter_index: usize) -> Result<Self>
    where
        Self: Sized,
    {
        let obs_context = tokio::task::spawn_blocking(move || {
            ObsContext::new(
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
        })
        .await??;

        tracing::debug!("OBS context initialized successfully");
        Ok(Self {
            adapter_index,
            obs_data: Some(ObsData {
                obs_context,
                current_output: None,
                source: None,
            }),
            last_encoder_settings: None,
            hook_successful: Arc::new(AtomicBool::new(false)),
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
        _hwnd: HWND,
        game_exe: &str,
        video_settings: EncoderSettings,
        (base_width, base_height): (u32, u32),
    ) -> Result<()> {
        let recording_path = dummy_video_path
            .to_str()
            .ok_or_eyre("Recording path must be valid UTF-8")?
            .to_string();

        tracing::debug!("Starting recording with path: {}", recording_path);

        let Some(obs_data) = &mut self.obs_data else {
            bail!("No OBS data to start recording");
        };

        // Check if already recording
        if obs_data.current_output.is_some() {
            bail!("Recording is already in progress");
        }

        let mut obs_context = obs_data.obs_context.clone();
        let hook_successful = self.hook_successful.clone();
        let adapter_index = self.adapter_index;
        let game_exe = game_exe.to_string();

        let (output, source, last_encoder_settings) = tokio::task::spawn_blocking(move || {
            // Set up scene and window capture based on input pid
            let mut scene = obs_context.scene(OWL_SCENE_NAME)?;

            obs_context.reset_video(
                ObsVideoInfoBuilder::new()
                    .adapter(adapter_index as u32)
                    .fps_num(FPS)
                    .fps_den(1)
                    .base_width(base_width)
                    .base_height(base_height)
                    .output_width(RECORDING_WIDTH)
                    .output_height(RECORDING_HEIGHT)
                    .build(),
            )?;

            let source = build_source(&mut obs_context, pid, &game_exe, &mut scene)?;

            // Register a signal to detect when the source is hooked,
            // so we can invalidate non-hooked recordings
            hook_successful.store(false, Ordering::SeqCst);
            tokio::spawn({
                let mut on_hooked = source
                    .signal_manager()
                    .on_hooked()
                    .context("failed to register on_hooked signal")?;
                async move {
                    if on_hooked.recv().await.is_ok() {
                        tracing::info!(
                            "Game capture source was able to successfully hook the game, recording will be valid"
                        );
                        hook_successful.store(true, Ordering::SeqCst);
                    }
                }
            });

            // Register the source
            scene.set_to_channel(0)?;

            // Set up output
            let mut output_settings = obs_context.data()?;
            output_settings.set_string("path", ObsPath::new(&recording_path).build())?;

            let output_info =
                OutputInfo::new("ffmpeg_muxer", "output", Some(output_settings), None);
            let mut output = obs_context.output(output_info)?;

            // TODO: it seems that video encoder and audio encoder should only be created once, instead of new ones every time that recording starts.
            // Register the video encoder with encoder-specific settings
            let video_encoder_data = obs_context.data()?;
            let video_encoder_settings = video_settings.apply_to_obs_data(video_encoder_data)?;
            let mut last_encoder_settings: Option<serde_json::Value> = video_encoder_settings
                .get_json()
                .ok()
                .and_then(|j| serde_json::from_str(&j).ok());
            if let Some(last_encoder_settings) = &mut last_encoder_settings {
                if let Some(object) = last_encoder_settings.as_object_mut() {
                    object.insert(
                        "encoder".to_string(),
                        match video_settings.encoder {
                            VideoEncoderType::X264 => "x264",
                            VideoEncoderType::NvEnc => "nvenc",
                        }
                        .into(),
                    );
                }
                tracing::info!("Recording starting with video settings: {last_encoder_settings:?}");
            }

            // Get video handler and attach encoder to output
            let video_handler = obs_context.get_video_ptr()?;
            output.video_encoder(
                VideoEncoderInfo::new(
                    match video_settings.encoder {
                        VideoEncoderType::X264 => ObsVideoEncoderType::OBS_X264,
                        VideoEncoderType::NvEnc => ObsVideoEncoderType::FFMPEG_NVENC,
                    },
                    "video_encoder",
                    Some(video_encoder_settings),
                    None,
                ),
                video_handler,
            )?;

            // Register the audio encoder
            let mut audio_settings = obs_context.data()?;
            audio_settings.set_int("bitrate", 160)?;

            let audio_info =
                AudioEncoderInfo::new("ffmpeg_aac", "audio_encoder", Some(audio_settings), None);

            let audio_handler = obs_context.get_audio_ptr()?;
            output.audio_encoder(audio_info, 0, audio_handler)?;

            output.start()?;

            eyre::Ok((output, source, last_encoder_settings))
        })
        .await??;

        obs_data.current_output = Some(output);
        obs_data.source = Some(source);
        self.last_encoder_settings = last_encoder_settings;

        tracing::debug!("OBS recording started successfully");

        Ok(())
    }

    async fn stop_recording(&mut self) -> Result<serde_json::Value> {
        tracing::debug!("Stopping OBS recording...");

        let Some(obs_data) = &mut self.obs_data else {
            bail!("No OBS data to stop recording");
        };

        if let Some(mut output) = obs_data.current_output.take() {
            let mut context = obs_data.obs_context.clone();
            let source = obs_data.source.take();
            tokio::task::spawn_blocking(move || {
                output.stop().wrap_err("Failed to stop OBS output")?;
                if let Some(mut scene) = context.get_scene(OWL_SCENE_NAME)
                    && let Some(source) = source
                {
                    scene.remove_source(&source)?;
                }

                eyre::Ok(())
            })
            .await??;
            tracing::debug!("OBS recording stopped");
        } else {
            tracing::warn!("No active recording to stop");
        }

        if !self.hook_successful.load(Ordering::SeqCst) {
            bail!("Application was never hooked, recording will be blank");
        }

        Ok(self.last_encoder_settings.take().unwrap_or_default())
    }
}

fn build_source(
    obs_context: &mut ObsContext,
    pid: u32,
    game_exe: &str,
    scene: &mut ObsSceneRef,
) -> Result<ObsSourceRef> {
    let result = if USE_WINDOW_CAPTURE {
        let window = WindowCaptureSourceBuilder::get_windows(WindowSearchMode::ExcludeMinimized)
            .map_err(|e| eyre!(e))?;
        let window = window
            .iter()
            .find(|w| w.0.pid == pid)
            .ok_or_else(|| eyre!("We couldn't find a capturable window for this application (EXE: {game_exe}, PID: {pid}). Please ensure you are capturing a game."))?;

        obs_context
            .source_builder::<WindowCaptureSourceBuilder, _>(OWL_CAPTURE_NAME)?
            .set_window(window)
            .set_capture_audio(true)
            .set_client_area(false) // capture full screen. if this is set to true there's black borders around the window capture.
            .add_to_scene(scene)
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

        obs_context
            .source_builder::<GameCaptureSourceBuilder, _>(OWL_CAPTURE_NAME)?
            .set_capture_mode(ObsGameCaptureMode::CaptureSpecificWindow)
            .set_window(window)
            .set_capture_audio(true)
            .add_to_scene(scene)
    };

    Ok(result?)
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
