use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use color_eyre::Result;
use constants::encoding::VideoEncoderType;
use egui_commonmark::CommonMarkCache;
use winit::{event::WindowEvent, event_loop::ActiveEventLoop};

use crate::{
    app_state::{
        AppState, AsyncRequest, GitHubRelease, HotkeyRebindTarget, ListeningForNewHotkey, UiUpdate,
        UiUpdateUnreliable,
    },
    config::{Credentials, Preferences},
    system::keycode::virtual_keycode_to_name,
    ui::{tray_icon::TrayIconState, views},
};

pub mod consent;
pub mod login;
pub mod main;

const HEADING_TEXT_SIZE: f32 = 24.0;
const SUBHEADING_TEXT_SIZE: f32 = 16.0;

pub struct App {
    app_state: Arc<AppState>,
    /// Receives commands from various tx in other threads to perform some UI update
    /// that don't need to be processed immediately.
    ui_update_unreliable_rx: tokio::sync::broadcast::Receiver<UiUpdateUnreliable>,

    /// Available video encoders, updated from tokio thread via mpsc channel
    available_video_encoders: Vec<VideoEncoderType>,

    /// Available sound cues from the cues folder
    available_cues: Vec<String>,

    login_api_key: String,
    is_authenticating_login_api_key: bool,
    authenticated_user_id: Option<Result<String, String>>,
    has_scrolled_to_bottom_of_consent: bool,

    /// Local copy of credentials, used to track UI state before saving to config
    local_credentials: Credentials,
    /// Local copy of preferences, used to track UI state before saving to config
    local_preferences: Preferences,
    /// Time since last requested config edit: we only attempt to save once enough time has passed
    config_last_edit: Option<Instant>,

    /// A newer release is available, updated from tokio thread via mpsc channel
    newer_release_available: Option<GitHubRelease>,

    md_cache: CommonMarkCache,
    visible: Arc<AtomicBool>,
    stopped_rx: tokio::sync::broadcast::Receiver<()>,
    stopped_tx: tokio::sync::broadcast::Sender<()>,
    has_stopped: bool,

    main_view_state: views::main::MainViewState,

    tray_icon: TrayIconState,
    is_recording: bool,

    /// Whether the encoder settings window is open
    encoder_settings_window_open: bool,
}
impl App {
    pub fn new(
        app_state: Arc<AppState>,
        visible: Arc<AtomicBool>,
        stopped_rx: tokio::sync::broadcast::Receiver<()>,
        stopped_tx: tokio::sync::broadcast::Sender<()>,
        ui_update_unreliable_rx: tokio::sync::broadcast::Receiver<UiUpdateUnreliable>,
        tray_icon: TrayIconState,
    ) -> Result<Self> {
        tracing::debug!("views::App::new() called");
        tracing::debug!("Loading credentials and preferences");
        let (local_credentials, local_preferences) = {
            let configs = app_state.config.read().unwrap();
            (configs.credentials.clone(), configs.preferences.clone())
        };

        // If we're fully authenticated, submit a request to validate our existing API key
        tracing::debug!("Checking if API key validation needed");
        if !local_credentials.api_key.is_empty() && local_credentials.has_consented {
            tracing::debug!("Submitting API key validation request");
            app_state
                .async_request_tx
                .blocking_send(AsyncRequest::ValidateApiKey {
                    api_key: local_credentials.api_key.clone(),
                })
                .ok();
        }

        tracing::debug!("Loading available audio cues");
        tracing::debug!("views::App::new() complete");
        Ok(Self {
            app_state,
            ui_update_unreliable_rx,

            login_api_key: local_credentials.api_key.clone(),
            is_authenticating_login_api_key: false,
            authenticated_user_id: None,
            has_scrolled_to_bottom_of_consent: false,

            available_video_encoders: vec![],
            available_cues: crate::assets::get_available_cues(),

            local_credentials,
            local_preferences,
            config_last_edit: None,

            newer_release_available: None,

            md_cache: CommonMarkCache::default(),
            visible,
            stopped_rx,
            stopped_tx,
            has_stopped: false,

            main_view_state: views::main::MainViewState::default(),

            tray_icon,
            is_recording: false,

            encoder_settings_window_open: false,
        })
    }

    pub fn should_close(&self) -> bool {
        self.has_stopped
    }

    pub fn handle_ui_update(&mut self, update: UiUpdate, ctx: &egui::Context) {
        match update {
            UiUpdate::ForceUpdate => {
                ctx.request_repaint();
            }
            UiUpdate::UpdateAvailableVideoEncoders(encoders) => {
                self.available_video_encoders = encoders;
            }
            UiUpdate::UpdateUserId(uid) => {
                let was_successful = uid.is_ok();
                self.authenticated_user_id = Some(uid);
                self.is_authenticating_login_api_key = false;
                if was_successful && !self.local_credentials.has_consented {
                    self.go_to_consent();
                }
            }
            UiUpdate::UploadFailed(error) => {
                self.main_view_state
                    .upload_manager
                    .update_last_upload_error(Some(error));
            }
            UiUpdate::UpdateRecordingState(recording) => {
                self.tray_icon.set_icon_recording(recording);
                self.is_recording = recording;
            }
            UiUpdate::UpdateNewerReleaseAvailable(release) => {
                self.newer_release_available = Some(release);
            }
            UiUpdate::UpdateUserUploads(uploads) => {
                self.main_view_state
                    .upload_manager
                    .update_user_uploads(uploads.uploads);
            }
            UiUpdate::UpdateLocalRecordings(recordings) => {
                self.main_view_state
                    .upload_manager
                    .update_local_recordings(recordings);
            }
            UiUpdate::FolderPickerResult { old_path, new_path } => {
                // Check if there are any recordings in the old location
                if old_path.exists()
                    && std::fs::read_dir(&old_path).is_ok_and(|dir| {
                        dir.filter_map(Result::ok)
                            .any(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                    })
                    && old_path != new_path
                {
                    // Show confirmation dialog to ask about moving files
                    self.main_view_state.pending_move_location = Some((old_path, new_path));
                } else {
                    // No recordings to move, just update the location
                    self.local_preferences.recording_location = new_path;
                }
            }
        }
    }

    pub fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, event: &WindowEvent) {
        loop {
            match self.ui_update_unreliable_rx.try_recv() {
                Ok(UiUpdateUnreliable::UpdateUploadProgress(progress_data)) => {
                    self.main_view_state
                        .upload_manager
                        .update_current_upload_progress(progress_data);
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => {
                    tracing::warn!("UiUpdateUnreliable channel lagged, dropping message");
                }
                Err(
                    tokio::sync::broadcast::error::TryRecvError::Empty
                    | tokio::sync::broadcast::error::TryRecvError::Closed,
                ) => {
                    break;
                }
            }
        }

        if self.stopped_rx.try_recv().is_ok() {
            tracing::info!("MainApp received stop signal");
            self.has_stopped = true;
            event_loop.exit();
            return;
        }

        // if user closes the app instead minimize to tray
        if matches!(event, WindowEvent::CloseRequested) && !self.has_stopped {
            self.visible.store(false, Ordering::Relaxed);
            // we handle visibility in the App level
        }

        // Handle hotkey rebinds
        let listening_for_new_hotkey = *self.app_state.listening_for_new_hotkey.read().unwrap();
        if let ListeningForNewHotkey::Captured { target, key } = listening_for_new_hotkey {
            if let Some(key_name) = virtual_keycode_to_name(key) {
                let rebind_target = match target {
                    HotkeyRebindTarget::Start => &mut self.local_preferences.start_recording_key,
                    HotkeyRebindTarget::Stop => &mut self.local_preferences.stop_recording_key,
                };
                *rebind_target = key_name.to_string();

                *self.app_state.listening_for_new_hotkey.write().unwrap() =
                    ListeningForNewHotkey::NotListening;
            } else {
                // Invalid hotkey? Try again
                *self.app_state.listening_for_new_hotkey.write().unwrap() =
                    ListeningForNewHotkey::Listening { target };
            }
        }
    }

    pub fn resumed(&mut self, ctx: &egui::Context, window: Arc<winit::window::Window>) {
        catppuccin_egui::set_theme(ctx, catppuccin_egui::MACCHIATO);

        ctx.style_mut(|style| {
            let bg_color = egui::Color32::from_rgb(19, 21, 26);
            style.visuals.window_fill = bg_color;
            style.visuals.panel_fill = bg_color;
        });

        let _ = self.app_state.ui_update_tx.ctx.set(ctx.clone());

        self.tray_icon.post_initialize(
            ctx.clone(),
            window,
            self.visible.clone(),
            self.stopped_tx.clone(),
            self.app_state.ui_update_tx.clone(),
        );
    }

    pub fn copy_in_app_config(&mut self) {
        let config = self.app_state.config.read().unwrap();
        if config.credentials != self.local_credentials {
            self.local_credentials = config.credentials.clone();
        }
        if config.preferences != self.local_preferences {
            self.local_preferences = config.preferences.clone();
        }
    }

    pub fn copy_out_local_config(&mut self) {
        // Queue up a save if any state has changed
        {
            let mut config = self.app_state.config.write().unwrap();
            let mut requires_save = false;
            if config.credentials != self.local_credentials {
                config.credentials = self.local_credentials.clone();
                requires_save = true;
            }
            if config.preferences != self.local_preferences {
                config.preferences = self.local_preferences.clone();
                requires_save = true;
            }
            if requires_save {
                self.config_last_edit = Some(Instant::now());
            }
        }

        if self
            .config_last_edit
            .is_some_and(|t| t.elapsed() > Duration::from_millis(250))
        {
            let _ = self.app_state.config.read().unwrap().save();
            self.config_last_edit = None;
        }
    }

    pub fn render(&mut self, ctx: &egui::Context) {
        let (has_api_key, has_consented) = (
            !self.local_credentials.api_key.is_empty(),
            self.local_credentials.has_consented,
        );

        match (has_api_key, has_consented) {
            (true, true) => self.main_view(ctx),
            (true, false) => self.consent_view(ctx),
            (false, _) => self.login_view(ctx),
        }
    }
}
impl App {
    fn go_to_login(&mut self) {
        self.local_credentials.logout();
        self.authenticated_user_id = None;
        self.is_authenticating_login_api_key = false;
    }

    fn go_to_consent(&mut self) {
        self.local_credentials.api_key = self.login_api_key.clone();
        self.local_credentials.has_consented = false;
        self.has_scrolled_to_bottom_of_consent = false;
    }

    fn go_to_main(&mut self) {
        self.local_credentials.has_consented = true;
    }
}
