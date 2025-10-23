use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::SystemTime,
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

use crate::{
    config::EncoderSettings,
    record::recorder::{HookEvent, VideoRecorder},
};

const OWL_SCENE_NAME: &str = "owl_data_collection_scene";
const OWL_CAPTURE_NAME: &str = "owl_game_capture";

// Untested! Added for testing purposes, but will probably not be used as
// we want to ensure we're capturing a game and WindowCapture will capture
// non-game content.
const USE_WINDOW_CAPTURE: bool = false;

pub struct ObsEmbeddedRecorder {
    _obs_thread: std::thread::JoinHandle<()>,
    obs_tx: tokio::sync::mpsc::Sender<RecorderMessage>,
}
impl ObsEmbeddedRecorder {
    pub async fn new(adapter_index: usize) -> Result<Self>
    where
        Self: Sized,
    {
        let (obs_tx, obs_rx) = tokio::sync::mpsc::channel(100);
        let (init_success_tx, init_success_rx) = tokio::sync::oneshot::channel();
        let obs_thread =
            std::thread::spawn(move || recorder_thread(adapter_index, obs_rx, init_success_tx));
        // Wait for the OBS context to be initialized, and bail out if it fails
        init_success_rx.await??;

        Ok(Self {
            _obs_thread: obs_thread,
            obs_tx,
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

        tracing::debug!("Starting recording with path: {recording_path}");

        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        self.obs_tx
            .send(RecorderMessage::StartRecording {
                request: RecordingRequest {
                    game_resolution: (base_width, base_height),
                    video_settings,
                    recording_path,
                    game_exe: game_exe.to_string(),
                    pid,
                },
                result_tx,
            })
            .await?;
        result_rx.await??;

        tracing::info!("OBS embedded recording started successfully");

        Ok(())
    }

    async fn stop_recording(&mut self) -> Result<serde_json::Value> {
        tracing::info!("Stopping OBS embedded recording...");

        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        self.obs_tx
            .send(RecorderMessage::StopRecording { result_tx })
            .await?;
        let result = result_rx.await??;

        tracing::info!("OBS embedded recording stopped successfully");

        Ok(result)
    }
}

enum RecorderMessage {
    StartRecording {
        request: RecordingRequest,
        result_tx: tokio::sync::oneshot::Sender<Result<()>>,
    },
    StopRecording {
        result_tx: tokio::sync::oneshot::Sender<Result<serde_json::Value>>,
    },
}

struct RecordingRequest {
    game_resolution: (u32, u32),
    video_settings: EncoderSettings,
    recording_path: String,
    game_exe: String,
    pid: u32,
}

fn recorder_thread(
    adapter_index: usize,
    mut rx: tokio::sync::mpsc::Receiver<RecorderMessage>,
    init_success_tx: tokio::sync::oneshot::Sender<Result<(), libobs_wrapper::utils::ObsError>>,
) {
    let skipped_frames = Arc::new(Mutex::new(None));
    let obs_context = ObsContext::new(
        ObsContext::builder()
            .set_logger(Box::new(TracingObsLogger {
                skipped_frames: skipped_frames.clone(),
            }))
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
    );
    let obs_context = match obs_context {
        Ok(obs_context) => {
            init_success_tx.send(Ok(())).unwrap();
            obs_context
        }
        Err(e) => {
            init_success_tx.send(Err(e)).unwrap();
            return;
        }
    };

    let mut state = RecorderState {
        obs_context,
        adapter_index,
        skipped_frames,
        current_output: None,
        source: None,
        last_encoder_settings: None,
        hook_rx: None,
    };

    while let Some(message) = rx.blocking_recv() {
        match message {
            RecorderMessage::StartRecording { request, result_tx } => {
                result_tx.send(state.start_recording(request)).ok();
            }
            RecorderMessage::StopRecording { result_tx } => {
                result_tx.send(state.stop_recording()).ok();
            }
        }
    }
}

struct RecorderState {
    obs_context: ObsContext,
    adapter_index: usize,
    skipped_frames: Arc<Mutex<Option<SkippedFrames>>>,
    current_output: Option<ObsOutputRef>,
    source: Option<ObsSourceRef>,
    last_encoder_settings: Option<serde_json::Value>,
    hook_rx: Option<tokio::sync::mpsc::Receiver<HookEvent>>,
}
impl RecorderState {
    fn start_recording(&mut self, request: RecordingRequest) -> eyre::Result<()> {
        if self.current_output.is_some() {
            bail!("Recording is already in progress");
        }

        // Set up scene and window capture based on input pid
        let mut scene = self.obs_context.scene(OWL_SCENE_NAME)?;

        self.obs_context.reset_video(
            ObsVideoInfoBuilder::new()
                .adapter(self.adapter_index as u32)
                .fps_num(FPS)
                .fps_den(1)
                .base_width(request.game_resolution.0)
                .base_height(request.game_resolution.1)
                .output_width(RECORDING_WIDTH)
                .output_height(RECORDING_HEIGHT)
                .build(),
        )?;

        let source = build_source(
            &mut self.obs_context,
            request.pid,
            &request.game_exe,
            &mut scene,
        )?;

        // Register a signal to detect when the source is hooked,
        // so we can invalidate non-hooked recordings
        let mut hook_signal_rx = source
            .signal_manager()
            .on_hooked()
            .context("failed to register on_hooked signal")?;

        // unhook signal
        let mut unhook_signal_rx = source
            .signal_manager()
            .on_unhooked()
            .context("failed to register on_hooked signal")?;

        // Spawn a thread to monitor the hook signals and send the hook time
        let (hook_tx, hook_rx) = tokio::sync::mpsc::channel(2);
        // In case it's a socket recorder that doesn't export hook signals or something
        self.hook_rx = match Some(&hook_signal_rx).is_some() {
            true => Some(hook_rx),
            false => None,
        };
        std::thread::spawn(move || {
            if hook_signal_rx.blocking_recv().is_ok() {
                let hook_time = SystemTime::now();
                tracing::info!("Game hooked at: {:?}", hook_time);
                let _ = hook_tx.send(HookEvent::Hooked(hook_time));
            }
            if unhook_signal_rx.blocking_recv().is_ok() {
                let hook_time = SystemTime::now();
                tracing::info!("Game unhooked at: {:?}", hook_time);
                let _ = hook_tx.send(HookEvent::Unhooked(hook_time));
            }
        });

        // Register the source
        scene.set_to_channel(0)?;

        // Set up output
        let mut output_settings = self.obs_context.data()?;
        output_settings.set_string("path", ObsPath::new(&request.recording_path).build())?;

        let output_info = OutputInfo::new("ffmpeg_muxer", "output", Some(output_settings), None);
        let mut output = self.obs_context.output(output_info)?;

        // TODO: it seems that video encoder and audio encoder should only be created once, instead of new ones every time that recording starts.
        // Register the video encoder with encoder-specific settings
        let video_encoder_data = self.obs_context.data()?;
        let video_encoder_settings = request
            .video_settings
            .apply_to_obs_data(video_encoder_data)?;
        self.last_encoder_settings = video_encoder_settings
            .get_json()
            .ok()
            .and_then(|j| serde_json::from_str(&j).ok());
        if let Some(encoder_settings_json) = &mut self.last_encoder_settings {
            if let Some(object) = encoder_settings_json.as_object_mut() {
                object.insert(
                    "encoder".to_string(),
                    match request.video_settings.encoder {
                        VideoEncoderType::X264 => "x264",
                        VideoEncoderType::NvEnc => "nvenc",
                    }
                    .into(),
                );
            }
            tracing::info!("Recording starting with video settings: {encoder_settings_json:?}");
        }

        // Get video handler and attach encoder to output
        let video_handler = self.obs_context.get_video_ptr()?;
        output.video_encoder(
            VideoEncoderInfo::new(
                match request.video_settings.encoder {
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
        let mut audio_settings = self.obs_context.data()?;
        audio_settings.set_int("bitrate", 160)?;

        let audio_info =
            AudioEncoderInfo::new("ffmpeg_aac", "audio_encoder", Some(audio_settings), None);

        let audio_handler = self.obs_context.get_audio_ptr()?;
        output.audio_encoder(audio_info, 0, audio_handler)?;

        // Just before we start, clear out our skipped frame counter
        self.skipped_frames.lock().unwrap().take();

        output.start()?;

        self.current_output = Some(output);
        self.source = Some(source);

        Ok(())
    }

    fn stop_recording(&mut self) -> eyre::Result<serde_json::Value> {
        if let Some(mut output) = self.current_output.take() {
            output.stop().wrap_err("Failed to stop OBS output")?;
            if let Some(mut scene) = self.obs_context.get_scene(OWL_SCENE_NAME)
                && let Some(source) = self.source.take()
            {
                scene.remove_source(&source)?;
            }
            tracing::debug!("OBS recording stopped");
        } else {
            tracing::warn!("No active recording to stop");
        }

        let mut output = self.last_encoder_settings.take().unwrap_or_default();

        // Write video_start and video_end obs hook timings to the metadata
        if let Some(mut hook_rx) = self.hook_rx.take() {
            // unfortunately receivers do not allow us to peek at the value without taking
            let hook_event = hook_rx.try_recv();
            if hook_event.is_err() {
                bail!("Application was never hooked, recording will be blank");
            } else {
                if let Some(object) = output.as_object_mut() {
                    // first event is logically guaranteed to be a hook event
                    if let HookEvent::Hooked(hook_time) = hook_event.unwrap() {
                        let hook_time = hook_time
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs_f64();
                        object.insert("video_start".to_string(), serde_json::to_value(hook_time)?);
                    }
                    // second event should be unhook event
                    if let Ok(HookEvent::Unhooked(hook_time)) = hook_rx.try_recv() {
                        let hook_time = hook_time
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs_f64();
                        object.insert("video_end".to_string(), serde_json::to_value(hook_time)?);
                    }
                }
            }
        }

        // Extremely ugly hack: We want to get the skipped frames percentage from the logs,
        // but that's not guaranteed to be present by the time this function would normally end.
        //
        // So, we wait 200ms to make sure we've cleared it.
        std::thread::sleep(std::time::Duration::from_millis(200));
        if let Some(skipped_frames) = self.skipped_frames.lock().unwrap().take() {
            let percentage = skipped_frames.percentage();
            if percentage > 5.0 {
                bail!(
                    "Too many frames were dropped ({}/{}, {percentage:.2}%), recording is unusable. Please consider using another encoder or tweaking your settings.",
                    skipped_frames.skipped,
                    skipped_frames.total
                );
            }

            if let Some(object) = output.as_object_mut() {
                object.insert(
                    "skipped_frames".to_string(),
                    serde_json::to_value(&skipped_frames)?,
                );
            }
        }

        Ok(output)
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

#[derive(Debug, serde::Serialize)]
struct SkippedFrames {
    skipped: usize,
    total: usize,
}
impl SkippedFrames {
    /// 0-100%
    pub fn percentage(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            (self.skipped as f64 / self.total as f64) * 100.0
        }
    }
}

#[derive(Debug)]
struct TracingObsLogger {
    skipped_frames: Arc<Mutex<Option<SkippedFrames>>>,
}
impl ObsLogger for TracingObsLogger {
    fn log(&mut self, level: libobs_wrapper::enums::ObsLogLevel, msg: String) {
        use libobs_wrapper::enums::ObsLogLevel;
        match level {
            ObsLogLevel::Error => tracing::error!(target: "obs", "{msg}"),
            ObsLogLevel::Warning => tracing::warn!(target: "obs", "{msg}"),
            ObsLogLevel::Info => {
                // HACK: If we encounter a message of the sort
                //   Video stopped, number of skipped frames due to encoding lag: 10758/22640 (47.5%)
                // we parse out the numbers to allow us to determine if it's an acceptable number
                // of skipped frames.
                if msg.contains("number of skipped frames due to encoding lag:")
                    && let Some(frames_data) = parse_skipped_frames(&msg)
                {
                    *self.skipped_frames.lock().unwrap() = Some(frames_data);
                }
                tracing::info!(target: "obs", "{msg}");
            }
            ObsLogLevel::Debug => tracing::debug!(target: "obs", "{msg}"),
        }
    }
}

fn parse_skipped_frames(msg: &str) -> Option<SkippedFrames> {
    // Find the colon and start from there
    let after_colon = msg.split(':').nth(1)?;
    let mut chars = after_colon.chars();

    // Skip to first digit and parse number (skipped frames)
    while let Some(c) = chars.next() {
        if !c.is_ascii_digit() {
            continue;
        }
        let mut num_str = c.to_string();
        num_str.extend(chars.by_ref().take_while(|c| c.is_ascii_digit()));
        let skipped = num_str.parse::<usize>().ok()?;

        // Skip to next digit and parse number (total frames)
        while let Some(c) = chars.next() {
            if !c.is_ascii_digit() {
                continue;
            }

            let mut num_str = c.to_string();
            num_str.extend(chars.by_ref().take_while(|c| c.is_ascii_digit()));
            let total = num_str.parse::<usize>().ok()?;

            return Some(SkippedFrames { skipped, total });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skipped_frames_basic() {
        let msg =
            "Video stopped, number of skipped frames due to encoding lag: 10758/22640 (47.5%)";
        let result = parse_skipped_frames(msg).expect("Failed to parse");

        assert_eq!(result.skipped, 10758);
        assert_eq!(result.total, 22640);
        assert!((result.percentage() - 47.48).abs() < 0.1);
    }
}
