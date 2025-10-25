use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use color_eyre::{
    Result,
    eyre::{self, Context, OptionExt as _, bail, eyre},
};
use constants::{FPS, RECORDING_HEIGHT, RECORDING_WIDTH, encoding::VideoEncoderType};
use windows::Win32::Foundation::HWND;

use libobs_sources::{
    ObsObjectUpdater, ObsSourceBuilder,
    windows::{
        GameCaptureSourceBuilder, GameCaptureSourceUpdater, ObsGameCaptureMode,
        WindowCaptureSourceBuilder, WindowCaptureSourceUpdater,
    },
};
use libobs_window_helper::WindowSearchMode;
use libobs_wrapper::{
    context::ObsContext,
    data::{
        output::ObsOutputRef,
        video::{ObsVideoInfo, ObsVideoInfoBuilder},
    },
    encoders::{ObsContextEncoders, ObsVideoEncoderType},
    enums::ObsScaleType,
    logger::ObsLogger,
    scenes::ObsSceneRef,
    sources::ObsSourceRef,
    utils::{AudioEncoderInfo, ObsPath, OutputInfo, VideoEncoderInfo, traits::ObsUpdatable},
};

use crate::{
    config::EncoderSettings,
    output_types::InputEventType,
    record::{input_recorder::InputEventStream, recorder::VideoRecorder},
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
    available_encoders: Vec<VideoEncoderType>,
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
        let available_encoders = init_success_rx.await??;

        Ok(Self {
            _obs_thread: obs_thread,
            obs_tx,
            available_encoders,
        })
    }
}
#[async_trait::async_trait(?Send)]
impl VideoRecorder for ObsEmbeddedRecorder {
    fn id(&self) -> &'static str {
        "ObsEmbedded"
    }

    fn available_encoders(&self) -> &[VideoEncoderType] {
        &self.available_encoders
    }

    async fn start_recording(
        &mut self,
        dummy_video_path: &Path,
        pid: u32,
        _hwnd: HWND,
        game_exe: &str,
        video_settings: EncoderSettings,
        (base_width, base_height): (u32, u32),
        event_stream: InputEventStream,
    ) -> Result<()> {
        let recording_path = dummy_video_path
            .to_str()
            .ok_or_eyre("Recording path must be valid UTF-8")?
            .to_string();

        tracing::debug!("Starting recording with path: {recording_path}");

        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        self.obs_tx
            .send(RecorderMessage::StartRecording {
                request: Box::new(RecordingRequest {
                    game_resolution: (base_width, base_height),
                    video_settings,
                    recording_path,
                    game_exe: game_exe.to_string(),
                    pid,
                    event_stream,
                }),
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
        request: Box<RecordingRequest>,
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
    event_stream: InputEventStream,
}

pub fn vet_to_obs_vet(vet: VideoEncoderType) -> ObsVideoEncoderType {
    match vet {
        VideoEncoderType::X264 => ObsVideoEncoderType::OBS_X264,
        VideoEncoderType::NvEnc => ObsVideoEncoderType::OBS_NVENC_H264_TEX,
        VideoEncoderType::Amf => ObsVideoEncoderType::H264_TEXTURE_AMF,
        VideoEncoderType::Qsv => ObsVideoEncoderType::OBS_QSV11_V2,
    }
}

pub fn obs_vet_to_vet(vet: &ObsVideoEncoderType) -> Option<VideoEncoderType> {
    match vet {
        ObsVideoEncoderType::OBS_X264 => Some(VideoEncoderType::X264),
        ObsVideoEncoderType::OBS_NVENC_H264_TEX => Some(VideoEncoderType::NvEnc),
        ObsVideoEncoderType::H264_TEXTURE_AMF => Some(VideoEncoderType::Amf),
        ObsVideoEncoderType::OBS_QSV11_V2 => Some(VideoEncoderType::Qsv),
        _ => None,
    }
}

fn recorder_thread(
    adapter_index: usize,
    mut rx: tokio::sync::mpsc::Receiver<RecorderMessage>,
    init_success_tx: tokio::sync::oneshot::Sender<
        Result<Vec<VideoEncoderType>, libobs_wrapper::utils::ObsError>,
    >,
) {
    let skipped_frames = Arc::new(Mutex::new(None));
    let obs_context = ObsContext::new(
        ObsContext::builder()
            .set_logger(Box::new(TracingObsLogger {
                skipped_frames: skipped_frames.clone(),
            }))
            .set_video_info(video_info(
                adapter_index,
                (RECORDING_WIDTH, RECORDING_HEIGHT),
            )),
    );
    let obs_context = match obs_context {
        Ok(obs_context) => {
            let available_encoders = obs_context.available_video_encoders().map(|es| {
                es.into_iter()
                    .filter_map(|e| obs_vet_to_vet(e.get_encoder_id()))
                    .collect::<Vec<_>>()
            });
            let available_encoders = match available_encoders {
                Ok(available_encoders) => available_encoders,
                Err(e) => {
                    tracing::error!(
                        "Failed to get available video encoders, assuming x264 only: {e}"
                    );
                    vec![VideoEncoderType::X264]
                }
            };
            init_success_tx.send(Ok(available_encoders)).unwrap();
            obs_context
        }
        Err(e) => {
            init_success_tx.send(Err(e)).unwrap();
            return;
        }
    };

    let mut state = RecorderState {
        adapter_index,
        skipped_frames,
        current_output: None,
        source: None,
        last_encoder_settings: None,
        was_hooked: Arc::new(AtomicBool::new(false)),
        last_video_encoder_type: None,
        last_game_exe: None,
        is_recording: false,

        obs_context,
    };

    let mut last_shutdown_tx = None;
    while let Some(message) = rx.blocking_recv() {
        match message {
            RecorderMessage::StartRecording { request, result_tx } => {
                let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

                result_tx
                    .send(state.start_recording(request, shutdown_rx))
                    .ok();
                last_shutdown_tx = Some(shutdown_tx);
            }
            RecorderMessage::StopRecording { result_tx } => {
                result_tx
                    .send(state.stop_recording(last_shutdown_tx.take()))
                    .ok();
            }
        }
    }
}

struct RecorderState {
    adapter_index: usize,
    skipped_frames: Arc<Mutex<Option<SkippedFrames>>>,
    current_output: Option<ObsOutputRef>,
    source: Option<ObsSourceRef>,
    last_encoder_settings: Option<serde_json::Value>,
    was_hooked: Arc<AtomicBool>,
    last_video_encoder_type: Option<VideoEncoderType>,
    last_game_exe: Option<String>,
    is_recording: bool,

    // This needs to be last as it needs to be dropped last
    obs_context: ObsContext,
}
impl RecorderState {
    fn start_recording(
        &mut self,
        request: Box<RecordingRequest>,
        mut shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) -> eyre::Result<()> {
        if self.is_recording {
            bail!("Recording is already in progress");
        }

        // Set up scene and window capture based on input pid
        let mut scene = if let Some(scene) = self.obs_context.get_scene(OWL_SCENE_NAME) {
            tracing::info!("Reusing existing scene");
            scene
        } else {
            tracing::info!("Creating new scene");
            self.obs_context.scene(OWL_SCENE_NAME)?
        };

        self.obs_context
            .reset_video(video_info(self.adapter_index, request.game_resolution))?;

        let source = prepare_source(
            &mut self.obs_context,
            request.pid,
            &request.game_exe,
            &mut scene,
            self.source.take(),
        )?;

        // Register the source
        scene.set_to_channel(0)?;

        // Set up output
        let mut output_settings = self.obs_context.data()?;
        output_settings.set_string("path", ObsPath::new(&request.recording_path).build())?;

        // Register the video encoder with encoder-specific settings
        let video_encoder_data = self.obs_context.data()?;
        let video_encoder_settings = request
            .video_settings
            .apply_to_obs_data(video_encoder_data)?;

        let output = if self.current_output.is_none()
            || self.last_video_encoder_type != Some(request.video_settings.encoder)
        {
            // We don't have an output, or the video encoder type has changed, so we need to create a new output
            //
            // TODO: once https://github.com/joshprk/libobs-rs/issues/38 is available, see if we can
            // update the output with a new encoder if the encoder type changes
            tracing::info!(
                "Creating new output with encoder type: {}",
                request.video_settings.encoder.id()
            );
            let output_info =
                OutputInfo::new("ffmpeg_muxer", "output", Some(output_settings), None);
            let mut output = self.obs_context.output(output_info)?;

            // Get video handler and attach encoder to output
            let video_handler = self.obs_context.get_video_ptr()?;
            output.video_encoder(
                VideoEncoderInfo::new(
                    vet_to_obs_vet(request.video_settings.encoder),
                    "video_encoder",
                    Some(video_encoder_settings.clone()),
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

            output
        } else {
            tracing::info!("Reusing existing output");
            let mut output = self.current_output.take().unwrap();
            output.update_settings(output_settings)?;
            output
        };

        self.last_video_encoder_type = Some(request.video_settings.encoder);

        // Listen for signals to pass onto the event stream
        self.was_hooked.store(false, Ordering::Relaxed);
        std::thread::spawn({
            let event_stream = request.event_stream;
            let was_hooked = self.was_hooked.clone();

            // output
            let mut start_signal_rx = output
                .signal_manager()
                .on_start()
                .context("failed to register output on_start signal")?;
            let mut stop_signal_rx = output
                .signal_manager()
                .on_stop()
                .context("failed to register output on_stop signal")?;

            // source
            let mut hook_signal_rx = source
                .signal_manager()
                .on_hooked()
                .context("failed to register source on_hooked signal")?;
            let mut unhook_signal_rx = source
                .signal_manager()
                .on_unhooked()
                .context("failed to register source on_unhooked signal")?;

            let last_game_exe = self.last_game_exe.clone();
            let game_exe = request.game_exe.clone();

            move || {
                let initial_time = Instant::now();
                futures::executor::block_on(async {
                    // Seems a bit dubious to use a tokio::select with
                    // a tokio oneshot in a non-Tokio context, but it seems to work
                    loop {
                        tokio::select! {
                            r = start_signal_rx.recv() => {
                                if r.is_ok() {
                                    if last_game_exe.as_ref().is_some_and(|g| g == &game_exe) {
                                        tracing::warn!("Video started again for last game, assuming we're already hooked");
                                        let _ = event_stream.send(InputEventType::HookStart);
                                        was_hooked.store(true, Ordering::Relaxed);
                                    }

                                    tracing::info!("Video started at {}s", initial_time.elapsed().as_secs_f64());
                                    let _ = event_stream.send(InputEventType::VideoStart);
                                }
                            }
                            r = stop_signal_rx.recv() => {
                                if r.is_ok() {
                                    tracing::info!("Video ended at {}s", initial_time.elapsed().as_secs_f64());
                                    let _ = event_stream.send(InputEventType::VideoEnd);
                                }
                            }
                            r = hook_signal_rx.recv() => {
                                if r.is_ok() {
                                    tracing::info!("Game hooked at {}s", initial_time.elapsed().as_secs_f64());
                                    let _ = event_stream.send(InputEventType::HookStart);
                                    was_hooked.store(true, Ordering::Relaxed);
                                }
                            }
                            r = unhook_signal_rx.recv() => {
                                if r.is_ok() {
                                    tracing::info!("Game unhooked at {}s", initial_time.elapsed().as_secs_f64());
                                    let _ = event_stream.send(InputEventType::HookEnd);
                                }
                            }
                            _ = &mut shutdown_rx => {
                                return;
                            }
                        }
                    }
                });
                tracing::info!("Game hook monitoring thread closed");
            }
        });

        // Update our last encoder settings
        self.last_encoder_settings = video_encoder_settings
            .get_json()
            .ok()
            .and_then(|j| serde_json::from_str(&j).ok());
        if let Some(encoder_settings_json) = &mut self.last_encoder_settings {
            if let Some(object) = encoder_settings_json.as_object_mut() {
                object.insert(
                    "encoder".to_string(),
                    request.video_settings.encoder.id().into(),
                );
            }
            tracing::info!("Recording starting with video settings: {encoder_settings_json:?}");
        }

        // Just before we start, clear out our skipped frame counter
        self.skipped_frames.lock().unwrap().take();

        output.start()?;

        self.current_output = Some(output);
        self.source = Some(source);
        self.last_game_exe = Some(request.game_exe.clone());
        self.is_recording = true;

        Ok(())
    }

    fn stop_recording(
        &mut self,
        last_shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    ) -> eyre::Result<serde_json::Value> {
        if let Some(output) = self.current_output.as_mut()
            && self.is_recording
        {
            output.stop().wrap_err("Failed to stop OBS output")?;
            tracing::debug!("OBS recording stopped");
            self.is_recording = false;
        } else {
            tracing::warn!("No active recording to stop");
        }

        let mut settings = self.last_encoder_settings.take().unwrap_or_default();

        if !self.was_hooked.load(Ordering::Relaxed) {
            bail!("Application was never hooked, recording will be blank");
        }

        if let Some(shutdown_tx) = last_shutdown_tx {
            shutdown_tx.send(()).ok();
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

            if let Some(object) = settings.as_object_mut() {
                object.insert(
                    "skipped_frames".to_string(),
                    serde_json::to_value(&skipped_frames)?,
                );
            }
        }

        Ok(settings)
    }
}

fn video_info(adapter_index: usize, (base_width, base_height): (u32, u32)) -> ObsVideoInfo {
    ObsVideoInfoBuilder::new()
        .adapter(adapter_index as u32)
        .fps_num(FPS)
        .fps_den(1)
        .base_width(base_width)
        .base_height(base_height)
        .output_width(RECORDING_WIDTH)
        .output_height(RECORDING_HEIGHT)
        .scale_type(ObsScaleType::Bicubic)
        .build()
}

fn prepare_source(
    obs_context: &mut ObsContext,
    pid: u32,
    game_exe: &str,
    scene: &mut ObsSceneRef,
    mut last_source: Option<ObsSourceRef>,
) -> Result<ObsSourceRef> {
    let capture_audio = true;

    let result = if USE_WINDOW_CAPTURE {
        let window = WindowCaptureSourceBuilder::get_windows(WindowSearchMode::ExcludeMinimized)
            .map_err(|e| eyre!(e))?;
        let window = window
            .iter()
            .find(|w| w.0.pid == pid)
            .ok_or_else(|| eyre!("We couldn't find a capturable window for this application (EXE: {game_exe}, PID: {pid}). Please ensure you are capturing a game."))?;

        // capture full screen. if this is set to true there's black borders around the window capture.
        let client_area = false;

        if let Some(mut source) = last_source.take() {
            tracing::info!("Reusing existing window capture source");
            source
                .create_updater::<WindowCaptureSourceUpdater>()?
                .set_window(window)
                .set_capture_audio(capture_audio)
                .set_client_area(client_area)
                .update()?;
            Ok(source)
        } else {
            tracing::info!("Creating new window capture source");
            obs_context
                .source_builder::<WindowCaptureSourceBuilder, _>(OWL_CAPTURE_NAME)?
                .set_window(window)
                .set_capture_audio(capture_audio)
                .set_client_area(client_area)
                .add_to_scene(scene)
        }
    } else {
        let window = GameCaptureSourceBuilder::get_windows(WindowSearchMode::ExcludeMinimized)
            .map_err(|e| eyre!(e))?;
        let window = window
            .iter()
            .find(|w| w.pid == pid)
            .ok_or_else(|| eyre!("We couldn't find a capturable window for this application (EXE: {game_exe}, PID: {pid}). Please ensure you are capturing a game."))?;

        if !window.is_game {
            bail!(
                "The window you're trying to record ({game_exe}) cannot be captured. Please ensure you are capturing a game."
            );
        }

        let capture_mode = ObsGameCaptureMode::CaptureSpecificWindow;

        if let Some(mut source) = last_source.take() {
            tracing::info!("Reusing existing game capture source");
            source
                .create_updater::<GameCaptureSourceUpdater>()?
                .set_capture_mode(capture_mode)
                .set_window_raw(window.obs_id.as_str())
                .set_capture_audio(capture_audio)
                .update()?;
            Ok(source)
        } else {
            tracing::info!("Creating new game capture source");

            if GameCaptureSourceBuilder::is_window_in_use_by_other_instance(window.pid)? {
                // We should only check this if we're creating a new source, as "another process" could be us otherwise
                bail!(
                    "The window you're trying to record ({game_exe}) is already being captured by another process. Do you have OBS or another instance of OWL Control open?\n\nNote that OBS is no longer required to use OWL Control - please close it if you have it running!",
                );
            }

            obs_context
                .source_builder::<GameCaptureSourceBuilder, _>(OWL_CAPTURE_NAME)?
                .set_capture_mode(capture_mode)
                .set_window(window)
                .set_capture_audio(capture_audio)
                .add_to_scene(scene)
        }
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
