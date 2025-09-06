use std::{
    path::Path,
    process::Stdio,
    sync::Arc,
    time::Duration,
};

use color_eyre::{
    Result,
    eyre::{Context, OptionExt as _, eyre},
};
use futures_util::Future;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Child as TokioChild,
    sync::Mutex,
    time::timeout,
};

use std::thread;

use libobs_sources::windows::{MonitorCaptureSourceBuilder, MonitorCaptureSourceUpdater};
use libobs_wrapper::{context::{ObsContext, ObsContextReturn}, data::output::ObsOutputRef, encoders::ObsVideoEncoderType, utils::VideoEncoderInfo};
use libobs_wrapper::data::ObsObjectUpdater;
use libobs_wrapper::encoders::ObsContextEncoders;
use libobs_wrapper::sources::ObsSourceBuilder;
use libobs_wrapper::utils::traits::ObsUpdatable;
use libobs_wrapper::utils::{AudioEncoderInfo, ObsPath, OutputInfo, StartupInfo};

// TODO: https://github.com/joshprk/libobs-rs/blob/main/examples/obs-preview/src/main.rs#L27
// probably the storing of the obsoutputref is causing the memleak and crashes.
// instead store the ctx in RW lock and .write().unwrap().output() to reconstruct?
// or instead store the output.stop() returned thread handler and call that when necessary?
pub struct WindowRecorder {
    obs_out: Arc<Mutex<Option<ObsOutputRef>>>,
    // obs_kill_switch: Future<Output = libobs_wrapper::data::output::ObsOutputRef>,
    #[allow(dead_code)]
    recording_path: String,
}

impl WindowRecorder {
    pub async fn start_recording(path: &Path, _pid: u32, _hwnd: usize) -> Result<WindowRecorder> {
        let recording_dir = path.parent()
            .ok_or_eyre("Recording path must have a parent directory")?;
        
        // Convert to absolute path for OBS
        // let mut absolute_recording_path = std::fs::canonicalize(recording_dir)
        //     .wrap_err("Failed to get absolute path for recording directory")?;
        // let recording_path = absolute_recording_path.to_str()
        //     .ok_or_eyre("Path must be valid UTF-8")?;

        let recording_path: &str = path.to_str()
            .ok_or_eyre("Recording path must be valid UTF-8")?;

        tracing::debug!("Starting OBS context");

        // // Start the OBS context
        let startup_info = StartupInfo::default();
        let context = ObsContext::new(startup_info).await?;
        let context = match context {
            ObsContextReturn::Done(c) => Some(c),
            ObsContextReturn::Restart => {
                None
            }
        };

        if context.is_none() {
            println!("OBS has been updated, restarting...");
            return Err(eyre!("OBS restart required"));
        }

        let mut context = context.unwrap();
        let mut scene = context.scene("main").await?;
        let monitors = MonitorCaptureSourceBuilder::get_monitors().map_err(|e| eyre!(e))?;

        let mut monitor_capture = context
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
            // .set_string("path", ObsPath::from_relative("recording.mp4").build())
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
            // .set_string("preset", "hq")
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

        tracing::debug!("OBS recording started successfully");
        let recorder = WindowRecorder {
            obs_out: Arc::new(Mutex::new(Some(output))),
            // obs_kill_switch: output.stop(),
            recording_path: recording_path.to_string(),
        };
        Ok(recorder)
    }

    pub fn listen_to_messages(&self) -> impl Future<Output = Result<()>> + use<> {
        async move {
            // For now, just wait - we could implement proper message handling later
            // The OBS bridge will handle recording state internally
            tokio::time::sleep(Duration::from_millis(100)).await;
            Ok(())
        }
    }

    pub fn stop_recording(&self) {
        tracing::debug!("Stopping OBS recording");
        // tokio::spawn({
        //     let out = self.obs_out.clone();
        //     async move {
        //         if let Ok(mut out_guard) = out.try_lock() {
        //             if let Some(out) = out_guard.as_mut() {
        //                 out.stop().await.ok();
        //             }
        //         }
        //     }
        // });
    }
}

impl Drop for WindowRecorder {
    fn drop(&mut self) {
        tracing::debug!("Shutting down OBS process...");

        tokio::spawn({
            let out = self.obs_out.clone();
            async move {
                if let Ok(mut out_guard) = out.try_lock() {
                    if let Some(out) = out_guard.as_mut() {
                        out.stop().await.ok();
                    }
                    tracing::debug!("Shut down OBS process");
                }
            }
        });
    }
}