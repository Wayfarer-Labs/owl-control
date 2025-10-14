use std::{path::Path, time::Duration};

use color_eyre::{
    Result,
    eyre::{Context, OptionExt as _},
};
use constants::{FPS, RECORDING_HEIGHT, RECORDING_WIDTH};
use obws::{
    Client,
    requests::{
        config::SetVideoSettings,
        inputs::{InputId, SetSettings, Volume},
        profiles::SetParameter,
        scene_items::{Position, Scale, SceneItemTransform, SetTransform},
        scenes::SceneId,
    },
};
use windows::Win32::Foundation::HWND;

use crate::record::recorder::{VideoRecorder, get_recording_base_resolution};

const OWL_PROFILE_NAME: &str = "owl_data_recorder";
const OWL_SCENE_NAME: &str = "owl_data_collection_scene";
const OWL_CAPTURE_NAME: &str = "owl_game_capture";

const VIDEO_BITRATE: u32 = 2500;
const SET_ENCODER: bool = false;

pub struct ObsSocketRecorder {
    // Use an Option to allow it to be consumed within the destructor
    client: Option<Client>,
}
impl ObsSocketRecorder {
    pub async fn new() -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Self { client: None })
    }
}
#[async_trait::async_trait(?Send)]
impl VideoRecorder for ObsSocketRecorder {
    fn id(&self) -> &'static str {
        "ObsSocket"
    }

    async fn start_recording(
        &mut self,
        dummy_video_path: &Path,
        _pid: u32,
        hwnd: HWND,
        game_exe: &str,
    ) -> Result<()> {
        // Connect to OBS
        let client = Client::connect("localhost", 4455, None::<&str>)
            .await
            .wrap_err("Failed to connect to OBS. Is it running?")?;

        let recording_path = dummy_video_path
            .parent()
            .ok_or_eyre("Video path must have a parent directory")?;
        let recording_path = std::fs::canonicalize(recording_path)
            .wrap_err("Failed to get absolute path for recording directory")?;

        // Pull out sub-APIs for easier access
        let profiles = client.profiles();
        let inputs = client.inputs();
        let scenes = client.scenes();
        let scene_items = client.scene_items();
        let config = client.config();

        // Get current profiles
        let all_profiles = profiles.list().await.wrap_err("Failed to get profiles")?;

        // Create and activate OWL profile
        if !all_profiles
            .profiles
            .contains(&OWL_PROFILE_NAME.to_string())
        {
            profiles
                .create(OWL_PROFILE_NAME)
                .await
                .wrap_err("Failed to create profile")?;
        }
        profiles
            .set_current(OWL_PROFILE_NAME)
            .await
            .wrap_err("Failed to set current profile")?;

        // Create and activate OWL scene
        let all_scenes = scenes.list().await.wrap_err("Failed to get scenes")?;
        if !all_scenes
            .scenes
            .iter()
            .any(|scene| scene.id.name == OWL_SCENE_NAME)
        {
            scenes
                .create(OWL_SCENE_NAME)
                .await
                .wrap_err("Failed to create scene")?;
        }
        scenes
            .set_current_program_scene(OWL_SCENE_NAME)
            .await
            .wrap_err("Failed to set current program scene")?;

        // Create OWL capture input
        let all_inputs = inputs.list(None).await.wrap_err("Failed to get inputs")?;
        let input_settings = {
            serde_json::json!({
                "capture_mode": "window",
                "window": get_obs_window_encoding(hwnd, game_exe),
                "priority": 2 /* WINDOW_PRIORITY_EXE */,
                "capture_audio": true,
            })
        };
        if let Some(input) = all_inputs
            .iter()
            .find(|input| input.id.name == OWL_CAPTURE_NAME)
        {
            inputs
                .set_settings(SetSettings {
                    input: input.id.clone().into(),
                    settings: &input_settings,
                    overlay: Some(false),
                })
                .await
                .wrap_err("Failed to set input settings")?;
        } else {
            inputs
                .create(obws::requests::inputs::Create {
                    scene: SceneId::Name(OWL_SCENE_NAME),
                    input: OWL_CAPTURE_NAME,
                    kind: "game_capture",
                    settings: Some(input_settings),
                    enabled: Some(true),
                })
                .await
                .wrap_err("Failed to create input")?;
        }

        let _ = inputs
            .set_volume(InputId::Name("Mic/Aux"), Volume::Db(-100.0))
            .await;
        let _ = inputs
            .set_volume(InputId::Name("Desktop Audio"), Volume::Db(-100.0))
            .await;

        for (category, name, value) in [
            ("SimpleOutput", "RecQuality", "Stream"),
            ("SimpleOutput", "VBitrate", &VIDEO_BITRATE.to_string()),
            ("Output", "Mode", "Simple"),
            ("SimpleOutput", "RecFormat2", "mp4"),
        ] {
            profiles
                .set_parameter(SetParameter {
                    category,
                    name,
                    value: Some(value),
                })
                .await
                .wrap_err_with(|| format!("Failed to set parameter {name}: {value}"))?;
        }

        // Set recording path
        {
            let normalized_recording_path = recording_path
                .to_str()
                .ok_or_eyre("Path must be valid UTF-8")?
                // Strip out the \\?\ prefix
                .replace("\\\\?\\", "");

            profiles
                .set_parameter(SetParameter {
                    category: "SimpleOutput",
                    name: "FilePath",
                    value: Some(&normalized_recording_path),
                })
                .await
                .wrap_err("Failed to set FilePath")?;
        }

        // Give OBS a moment to process the path change
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify the path was set correctly
        let current_path = profiles
            .parameter("SimpleOutput", "FilePath")
            .await
            .wrap_err("Failed to get FilePath")?;
        tracing::info!("OBS confirmed recording path: {:?}", current_path.value);

        // Monitor/resolution info
        let resolution = get_recording_base_resolution(hwnd)?;

        // Log resolution for debugging
        tracing::info!("Base recording resolution: {resolution:?}");

        // Set video settings
        config
            .set_video_settings(SetVideoSettings {
                fps_numerator: Some(FPS),
                fps_denominator: Some(1),
                base_width: Some(resolution.0 as u32),
                base_height: Some(resolution.1 as u32),
                output_width: Some(RECORDING_WIDTH),
                output_height: Some(RECORDING_HEIGHT),
            })
            .await
            .wrap_err("Failed to set video settings")?;

        // Find the owl game capture scene id
        let item_list = scene_items
            .list(SceneId::Name(OWL_SCENE_NAME))
            .await
            .wrap_err("Failed to get scene items")?;
        let owl_gc_id = item_list
            .iter()
            .find(|item| item.source_name == OWL_CAPTURE_NAME)
            .ok_or_eyre("Failed to find owl game capture scene item")?
            .id;
        scene_items
            .set_transform(SetTransform {
                scene: SceneId::Name(OWL_SCENE_NAME),
                item_id: owl_gc_id,
                transform: SceneItemTransform {
                    position: Some(Position {
                        x: Some(0.0),
                        y: Some(0.0),
                    }),
                    rotation: Some(0.0),
                    scale: Some(Scale {
                        x: Some(1.0),
                        y: Some(1.0),
                    }),
                    alignment: None,
                    bounds: None,
                    crop: None,
                },
            })
            .await
            .wrap_err("Failed to set owl game capture scene item transform")?;

        if SET_ENCODER {
            tracing::info!("Setting custom encoder settings");
            profiles
                .set_parameter(SetParameter {
                    category: "SimpleOutput",
                    name: "StreamEncoder",
                    value: Some("x264"),
                })
                .await
                .wrap_err("Failed to set StreamEncoder")?;
            profiles
                .set_parameter(SetParameter {
                    category: "SimpleOutput",
                    name: "Preset",
                    value: Some("veryfast"),
                })
                .await
                .wrap_err("Failed to set Preset")?;
        } else {
            tracing::info!("Using user's default encoder settings");
        }

        client
            .recording()
            .start()
            .await
            .wrap_err("Failed to start recording")?;
        tracing::info!("OBS recording started successfully");

        self.client = Some(client);

        Ok(())
    }

    async fn stop_recording(&mut self) -> Result<()> {
        tracing::info!("Stopping OBS recording");
        if let Some(client) = &self.client {
            // Log, but do not explode if it fails
            if let Err(e) = client.recording().stop().await {
                tracing::error!("Failed to stop recording: {e}");
            }
        }
        tracing::info!("OBS recording stopped successfully");
        Ok(())
    }
}
impl Drop for ObsSocketRecorder {
    fn drop(&mut self) {
        tracing::info!("Shutting down OBS socket recorder");
        let client = self.client.take();
        tokio::spawn(async move {
            if let Some(client) = &client {
                // Log, but do not explode if it fails
                if let Err(e) = client.recording().stop().await {
                    tracing::error!("Failed to stop recording: {e}");
                }
            }
        });
    }
}

fn get_obs_window_encoding(hwnd: HWND, game_exe: &str) -> String {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetWindowTextLengthW, GetWindowTextW,
    };

    // Get window title
    let title_len = unsafe { GetWindowTextLengthW(hwnd) };
    let mut title = String::new();
    if title_len > 0 {
        let mut buf = vec![0u16; (title_len + 1) as usize];
        let copied = unsafe { GetWindowTextW(hwnd, &mut buf) };
        if copied > 0 {
            if let Some(end) = buf.iter().position(|&c| c == 0) {
                title = String::from_utf16_lossy(&buf[..end]);
            } else {
                title = String::from_utf16_lossy(&buf);
            }
        }
    }

    // Get window class
    let mut class_buf = [0u16; 256];
    let class_len = unsafe { GetClassNameW(hwnd, &mut class_buf) };
    let class = if class_len > 0 {
        String::from_utf16_lossy(&class_buf[..class_len as usize])
    } else {
        String::new()
    };

    format!(
        "{}:{}:{}",
        sanitize_string_for_obs(&title),
        sanitize_string_for_obs(&class),
        sanitize_string_for_obs(game_exe)
    )
}

/// Sanitize a string for OBS to avoid interfering with the separators it uses
///
/// https://github.com/obsproject/obs-studio/blob/0b1229632063a13dfd26cf1cd9dd43431d8c68f6/libobs/util/windows/window-helpers.c#L8-L12
fn sanitize_string_for_obs(s: &str) -> String {
    s.replace('#', "#22").replace(':', "#3A")
}
