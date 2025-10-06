use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use color_eyre::Result;
use egui_commonmark::{CommonMarkCache, commonmark_str};
use winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

use crate::{
    app_state::{AppState, AsyncRequest, UiUpdate},
    config::{Credentials, Preferences},
    upload,
};

use eframe::egui;
use egui::ViewportCommand;

mod overlay;
pub mod tray_icon;
mod util;

#[derive(PartialEq)]
enum HotkeyState {
    Chilling,
    ListenStart,
    ListenStop,
}

pub fn start(
    app_state: Arc<AppState>,
    ui_update_rx: tokio::sync::mpsc::Receiver<UiUpdate>,
) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 650.0])
            .with_resizable(true)
            .with_title("OWL Control")
            .with_icon(tray_icon::egui_icon()),
        ..Default::default()
    };

    let _tray_icon = tray_icon::initialize()?;

    let visible = Arc::new(AtomicBool::new(true));

    // launch overlay on seperate thread so non-blocking
    std::thread::spawn({
        let app_state = app_state.clone();
        move || {
            egui_overlay::start(overlay::OverlayApp::new(app_state));
        }
    });

    eframe::run_native(
        "OWL Control",
        options,
        Box::new(move |cc| {
            let RawWindowHandle::Win32(handle) = cc.window_handle().unwrap().as_raw() else {
                panic!("Unsupported platform");
            };

            tray_icon::post_initialize(cc.egui_ctx.clone(), handle, visible.clone());
            let _ = app_state.ui_update_tx.ctx.set(cc.egui_ctx.clone());

            catppuccin_egui::set_theme(&cc.egui_ctx, catppuccin_egui::MACCHIATO);

            cc.egui_ctx.style_mut(|style| {
                let bg_color = egui::Color32::from_rgb(19, 21, 26);
                style.visuals.window_fill = bg_color;
                style.visuals.panel_fill = bg_color;
            });

            Ok(Box::new(MainApp::new(app_state, visible, ui_update_rx)?))
        }),
    )
    .unwrap();

    Ok(())
}

const HEADING_TEXT_SIZE: f32 = 24.0;
const SUBHEADING_TEXT_SIZE: f32 = 16.0;

pub struct MainApp {
    app_state: Arc<AppState>,
    frame: u64,
    /// Receives commands from various tx in other threads to perform some UI update
    ui_update_rx: tokio::sync::mpsc::Receiver<UiUpdate>,

    login_api_key: String,
    authenticated_user_id: Option<Result<String, String>>,
    has_scrolled_to_bottom_of_consent: bool,

    /// Local copy of credentials, used to track UI state before saving to config
    local_credentials: Credentials,
    /// Local copy of preferences, used to track UI state before saving to config
    local_preferences: Preferences,
    /// Time since last requested config edit: we only attempt to save once enough time has passed
    config_last_edit: Option<Instant>,
    /// Is the UI currently listening for user to select a new hotkey for recording shortcut
    hotkey_state: HotkeyState,
    /// Current upload progress, updated from upload bridge via mpsc channel
    current_upload_progress: Option<upload::ProgressData>,

    md_cache: CommonMarkCache,
    visible: Arc<AtomicBool>,
}
impl MainApp {
    fn new(
        app_state: Arc<AppState>,
        visible: Arc<AtomicBool>,
        ui_update_rx: tokio::sync::mpsc::Receiver<UiUpdate>,
    ) -> Result<Self> {
        let (local_credentials, local_preferences) = {
            let configs = app_state.config.read().unwrap();
            (configs.credentials.clone(), configs.preferences.clone())
        };

        // If we're fully authenticated, submit a request to validate our existing API key
        if !local_credentials.api_key.is_empty() && local_credentials.has_consented {
            app_state
                .async_request_tx
                .blocking_send(AsyncRequest::ValidateApiKey {
                    api_key: local_credentials.api_key.clone(),
                })
                .ok();
        }

        Ok(Self {
            app_state,
            frame: 0,
            ui_update_rx,

            login_api_key: local_credentials.api_key.clone(),
            authenticated_user_id: None,
            has_scrolled_to_bottom_of_consent: false,

            local_credentials,
            local_preferences,
            config_last_edit: None,
            hotkey_state: HotkeyState::Chilling,
            current_upload_progress: None,

            md_cache: CommonMarkCache::default(),
            visible,
        })
    }
}
impl MainApp {
    fn go_to_login(&mut self) {
        self.local_credentials.logout();
        self.authenticated_user_id = None;
    }

    fn go_to_consent(&mut self) {
        self.local_credentials.api_key = self.login_api_key.clone();
        self.local_credentials.has_consented = false;
        self.has_scrolled_to_bottom_of_consent = false;
    }

    fn go_to_main(&mut self) {
        self.local_credentials.has_consented = true;
    }

    pub fn login_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Center the content vertically and horizontally
            ui.vertical_centered(|ui| {
                // Extremely ugly bodge. I assume there's a way to do this correctly, but I can't find it at a glance.
                let content_height = 240.0;
                let available_height = ui.available_height();
                ui.add_space((available_height - content_height) / 2.0);

                ui.set_max_width(ui.available_width().min(400.0));
                ui.vertical_centered(|ui| {
                    // Logo/Icon area (placeholder for now)
                    ui.add_space(20.0);

                    // Main heading with better styling
                    ui.heading(
                        egui::RichText::new("Welcome to OWL Control")
                            .size(28.0)
                            .strong()
                            .color(egui::Color32::from_rgb(220, 220, 220)),
                    );

                    ui.add_space(8.0);

                    // Subtitle
                    ui.label(
                        egui::RichText::new("Please enter your API key to continue")
                            .size(16.0)
                            .color(egui::Color32::from_rgb(180, 180, 180)),
                    );

                    ui.add_space(20.0);

                    // API Key input section
                    ui.vertical_centered(|ui| {
                        // Styled text input
                        let text_edit = egui::TextEdit::singleline(&mut self.login_api_key)
                            .desired_width(ui.available_width())
                            .desired_rows(1);

                        ui.add_sized(egui::vec2(ui.available_width(), 40.0), text_edit);

                        ui.add_space(20.0);

                        // Submit button with better styling
                        let submit_button = ui.add_sized(
                            egui::vec2(120.0, 36.0),
                            egui::Button::new(egui::RichText::new("Continue").size(16.0).strong()),
                        );

                        if submit_button.clicked() {
                            self.go_to_consent();
                        }
                    });
                    ui.add_space(20.0);

                    // Help text
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        ui.label(
                            egui::RichText::new("Don't have an API key? Please sign up at ")
                                .size(12.0)
                                .color(egui::Color32::from_rgb(140, 140, 140)),
                        );
                        ui.hyperlink_to(
                            egui::RichText::new("our website.").size(12.0),
                            "https://wayfarerlabs.ai/handler/sign-in",
                        );
                    });
                });
            });
        });
    }

    pub fn consent_view(&mut self, ctx: &egui::Context) {
        let padding = 8;
        let button_font_size = 14.0;

        egui::TopBottomPanel::top("consent_panel_top").show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(padding))
                .show(ui, |ui| {
                    ui.heading(
                        egui::RichText::new("Informed Consent & Terms of Service")
                            .size(HEADING_TEXT_SIZE)
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new("Please read the following information carefully.")
                            .size(SUBHEADING_TEXT_SIZE),
                    );
                });
        });

        egui::TopBottomPanel::bottom("consent_panel_bottom").show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(padding))
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().button_padding = egui::vec2(8.0, 2.0);
                            if ui
                                .add_enabled(
                                    self.has_scrolled_to_bottom_of_consent,
                                    egui::Button::new(
                                        egui::RichText::new("Accept")
                                            .size(button_font_size)
                                            .strong(),
                                    ),
                                )
                                .clicked()
                            {
                                self.go_to_main();
                            }
                            if ui
                                .button(
                                    egui::RichText::new("Cancel")
                                        .size(button_font_size)
                                        .strong(),
                                )
                                .clicked()
                            {
                                self.go_to_login();
                            }
                        });
                    });
                });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::new()
                .inner_margin(egui::Margin::same(padding))
                .show(ui, |ui| {
                    let output = egui::ScrollArea::vertical().show(ui, |ui| {
                        commonmark_str!(
                            ui,
                            &mut self.md_cache,
                            "./crates/owl-recorder/src/ui/consent.md"
                        );
                    });

                    self.has_scrolled_to_bottom_of_consent |= (output.state.offset.y
                        + output.inner_rect.height())
                        >= output.content_size.y;
                });
        });
    }

    pub fn main_view(&mut self, ctx: &egui::Context) {
        const SETTINGS_TEXT_WIDTH: f32 = 150.0;
        const SETTINGS_TEXT_HEIGHT: f32 = 20.0;

        fn add_settings_text(ui: &mut egui::Ui, widget: impl egui::Widget) -> egui::Response {
            ui.allocate_ui_with_layout(
                egui::vec2(SETTINGS_TEXT_WIDTH, SETTINGS_TEXT_HEIGHT),
                egui::Layout {
                    main_dir: egui::Direction::LeftToRight,
                    main_wrap: false,
                    main_align: egui::Align::RIGHT,
                    main_justify: true,
                    cross_align: egui::Align::Center,
                    cross_justify: true,
                },
                |ui| ui.add(widget),
            )
            .inner
        }

        fn add_settings_widget(ui: &mut egui::Ui, widget: impl egui::Widget) -> egui::Response {
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), SETTINGS_TEXT_HEIGHT),
                egui::Layout {
                    main_dir: egui::Direction::LeftToRight,
                    main_wrap: false,
                    main_align: egui::Align::LEFT,
                    main_justify: true,
                    cross_align: egui::Align::Center,
                    cross_justify: true,
                },
                |ui| ui.add(widget),
            )
            .inner
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(
                egui::RichText::new("Settings")
                    .size(HEADING_TEXT_SIZE)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("Configure your recording preferences")
                    .size(SUBHEADING_TEXT_SIZE),
            );
            ui.add_space(10.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Account Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Account").size(18.0).strong());
                    ui.separator();

                    ui.vertical(|ui| {
                        ui.label("User ID:");
                        ui.horizontal(|ui| {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add_sized(
                                            egui::vec2(0.0, SETTINGS_TEXT_HEIGHT),
                                            egui::Button::new("Log out"),
                                        )
                                        .clicked()
                                    {
                                        self.go_to_login();
                                    }

                                    let mut user_id = self
                                        .authenticated_user_id
                                        .clone()
                                        .unwrap_or_else(|| Ok("Authenticating...".to_string()))
                                        .unwrap_or_else(|e| format!("Error: {e}"));
                                    ui.add_sized(
                                        egui::vec2(ui.available_width(), SETTINGS_TEXT_HEIGHT),
                                        egui::TextEdit::singleline(&mut user_id),
                                    );
                                },
                            );
                        });
                    });
                });
                ui.add_space(10.0);

                // Keyboard Shortcuts Section
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Keyboard Shortcuts")
                            .size(18.0)
                            .strong(),
                    );
                    ui.separator();

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Start Recording:"));
                        let button_text = if self.hotkey_state == HotkeyState::ListenStart {
                            "Press any key...".to_string()
                        } else {
                            self.local_preferences.start_recording_key.clone()
                        };

                        if add_settings_widget(ui, egui::Button::new(button_text)).clicked() {
                            self.hotkey_state = HotkeyState::ListenStart;
                        }
                    });

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Stop Recording:"));
                        let button_text = if self.hotkey_state == HotkeyState::ListenStop {
                            "Press any key...".to_string()
                        } else {
                            self.local_preferences.stop_recording_key.clone()
                        };

                        if add_settings_widget(ui, egui::Button::new(button_text)).clicked() {
                            self.hotkey_state = HotkeyState::ListenStop;
                        }
                    });
                });
                ui.add_space(10.0);

                // Overlay Settings Section
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("Recorder Customization")
                            .size(18.0)
                            .strong(),
                    );
                    ui.separator();

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Overlay Opacity:"));
                        let mut stored_opacity = self.local_preferences.overlay_opacity;

                        let mut egui_opacity = stored_opacity as f32 / 255.0 * 100.0;

                        let r = ui
                            .scope(|ui| {
                                // one day egui will make sliders respect their width properly
                                ui.spacing_mut().slider_width = ui.available_width() - 50.0;
                                add_settings_widget(
                                    ui,
                                    egui::Slider::new(&mut egui_opacity, 0.0..=100.0)
                                        .suffix("%")
                                        .integer(),
                                )
                            })
                            .inner;
                        if r.changed() {
                            stored_opacity = (egui_opacity / 100.0 * 255.0) as u8;
                            self.local_preferences.overlay_opacity = stored_opacity;
                        }
                    });

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Recording Audio Cue:"));
                        let honk = self.local_preferences.honk;
                        add_settings_widget(
                            ui,
                            egui::Checkbox::new(
                                &mut self.local_preferences.honk,
                                match honk {
                                    true => "Honk.",
                                    false => "Honk?",
                                },
                            ),
                        );
                    })
                });

                ui.add_space(10.0);

                // Upload Manager Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Upload Manager").size(18.0).strong());
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        let available_width = ui.available_width() - 40.0;
                        let cell_width = available_width / 4.0;

                        let upload_stats = self.app_state.upload_stats.read().unwrap().clone();

                        // Cell 1: Total Uploaded
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                create_upload_cell(
                                    ui,
                                    "📊", // Icon
                                    "Total Uploaded",
                                    &util::format_seconds(
                                        upload_stats.total_duration_uploaded as u64,
                                    ),
                                );
                            },
                        );

                        // Cell 2: Files Uploaded
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                create_upload_cell(
                                    ui,
                                    "📁", // Icon
                                    "Files Uploaded",
                                    &upload_stats.total_files_uploaded.to_string(),
                                );
                            },
                        );

                        // Cell 3: Volume Uploaded
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                create_upload_cell(
                                    ui,
                                    "💾", // Icon
                                    "Volume Uploaded",
                                    &util::format_bytes(upload_stats.total_volume_uploaded),
                                );
                            },
                        );

                        // Cell 4: Last Upload
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                create_upload_cell(
                                    ui,
                                    "🕒", // Icon
                                    "Last Upload",
                                    &upload_stats
                                        .last_upload_date
                                        .as_date()
                                        .map(util::format_datetime)
                                        .unwrap_or_else(|| "Never".to_string()),
                                );
                            },
                        );
                    });

                    // Progress Bar
                    let is_uploading = self.current_upload_progress.is_some();
                    if let Some(progress) = &self.current_upload_progress {
                        ui.add_space(10.0);
                        ui.label(format!(
                            "Current upload: {:.2}% ({}/{})",
                            progress.percent,
                            util::format_bytes(progress.bytes_uploaded),
                            util::format_bytes(progress.total_bytes),
                        ));
                        ui.add(egui::ProgressBar::new(progress.percent as f32 / 100.0));
                        ui.label(format!(
                            "Speed: {:.1} MB/s • ETA: {}",
                            progress.speed_mbps,
                            util::format_seconds(progress.eta_seconds as u64),
                        ));
                    }

                    // Unreliable Connection Setting
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Checkbox::new(
                            &mut self.local_preferences.unreliable_connection,
                            "Optimize for unreliable connections",
                        ));
                    });
                    ui.label(
                        egui::RichText::new(concat!(
                            "Enable this if you have a slow or unstable internet connection. ",
                            "This will use smaller file chunks to improve upload success rates."
                        ))
                        .size(10.0)
                        .color(egui::Color32::from_rgb(128, 128, 128)),
                    );

                    // Upload Button
                    ui.add_space(10.0);
                    ui.add_enabled_ui(!is_uploading, |ui| {
                        if ui
                            .add_sized(
                                egui::vec2(ui.available_width(), 32.0),
                                egui::Button::new(
                                    egui::RichText::new(if is_uploading {
                                        "Upload in Progress..."
                                    } else {
                                        "Upload Recordings"
                                    })
                                    .size(12.0),
                                ),
                            )
                            .clicked()
                        {
                            // Handle upload
                            let app_state = self.app_state.clone();
                            let api_key = self.local_credentials.api_key.clone();
                            let unreliable_connection =
                                self.local_preferences.unreliable_connection;
                            std::thread::spawn(move || {
                                upload::start(app_state, &api_key, unreliable_connection);
                            });
                        }
                    });
                });

                // Logo
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new("Wayfarer Labs")
                            .italics()
                            .color(egui::Color32::GRAY),
                    );
                });
            });
        });
    }
}

impl eframe::App for MainApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        // if user closes the app instead minimize to tray
        if ctx.input(|i| i.viewport().close_requested()) {
            self.visible.store(false, Ordering::Relaxed);
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(ViewportCommand::Visible(false));
        }

        if self.hotkey_state != HotkeyState::Chilling {
            ctx.input(|i| {
                if i.keys_down.len() == 1 {
                    println!("{:?} {:?} {} ", i.keys_down, i.modifiers, i.keys_down.len());
                    let key = i
                        .keys_down
                        .iter()
                        .next()
                        .expect("keycode expected")
                        .name()
                        .to_string();
                    match self.hotkey_state {
                        HotkeyState::ListenStart => {
                            self.local_preferences.start_recording_key = key
                        }
                        HotkeyState::ListenStop => self.local_preferences.stop_recording_key = key,
                        HotkeyState::Chilling => (), // will never hit this, just to make rust compiler happy
                    }
                    self.hotkey_state = HotkeyState::Chilling;
                }
            })
        }

        match self.ui_update_rx.try_recv() {
            Ok(UiUpdate::UpdateUploadProgress(progress_data)) => {
                // handled in main_view directly from app_state
                self.current_upload_progress = progress_data;
            }
            Ok(UiUpdate::UpdateUserId(uid)) => {
                self.authenticated_user_id = Some(uid);
            }
            Err(_) => {}
        };

        let (has_api_key, has_consented) = (
            !self.local_credentials.api_key.is_empty(),
            self.local_credentials.has_consented,
        );

        match (has_api_key, has_consented) {
            (true, true) => self.main_view(ctx),
            (true, false) => self.consent_view(ctx),
            (false, _) => self.login_view(ctx),
        }

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

        self.frame += 1;
    }
}

fn create_upload_cell(ui: &mut egui::Ui, icon: &str, title: &str, value: &str) {
    // Icon
    ui.label(egui::RichText::new(icon).size(28.0));
    ui.add_space(8.0);
    // Title
    ui.label(egui::RichText::new(title).size(12.0).strong());
    ui.add_space(4.0);
    // Value
    ui.label(
        egui::RichText::new(value)
            .size(10.0)
            .color(egui::Color32::from_rgb(128, 128, 128)),
    );
}
