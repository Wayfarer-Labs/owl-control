use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use color_eyre::Result;
use winit::raw_window_handle::{HasWindowHandle as _, RawWindowHandle};

use crate::{
    app_state::{AppState, Command, CommandReceiver},
    auth_service::ApiClient,
    config::{Credentials, Preferences, UploadStats},
    upload_manager::{is_upload_bridge_running, start_upload_bridge},
};

use eframe::egui;
use egui::ViewportCommand;

mod overlay;
pub mod tray_icon;

pub fn start(app_state: Arc<AppState>, rx: CommandReceiver) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([600.0, 600.0])
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

            Ok(Box::new(MainApp::new(app_state, visible, rx)?))
        }),
    )
    .unwrap();

    Ok(())
}

pub struct MainApp {
    app_state: Arc<AppState>,
    frame: u64,
    /// Receives commands from various tx in other threads to perform some UI update
    rx: CommandReceiver,

    api_client: ApiClient,
    login_api_key: String,
    has_scrolled_to_bottom_of_consent: bool,

    /// Local copy of credentials, used to track UI state before saving to config
    local_credentials: Credentials,
    /// Local copy of preferences, used to track UI state before saving to config
    local_preferences: Preferences,
    /// Time since last requested config edit: we only attempt to save once enough time has passed
    config_last_edit: Option<Instant>,

    upload_stats: UploadStats,
    visible: Arc<AtomicBool>,
}
impl MainApp {
    fn new(
        app_state: Arc<AppState>,
        visible: Arc<AtomicBool>,
        rx: CommandReceiver,
    ) -> Result<Self> {
        let local_credentials: Credentials;
        let local_preferences: Preferences;
        {
            let configs = app_state.config.read().unwrap();
            local_credentials = configs.credentials.clone();
            local_preferences = configs.preferences.clone();
        }
        let mut api_client = ApiClient::new(app_state.tx.clone());
        api_client.validate_api_key(&local_credentials.api_key.clone());
        Ok(Self {
            app_state,
            frame: 0,
            rx,

            api_client,
            login_api_key: local_credentials.api_key.clone(),
            has_scrolled_to_bottom_of_consent: false,

            local_credentials,
            local_preferences,
            config_last_edit: None,

            upload_stats: UploadStats::new()?,
            visible,
        })
    }
}
impl MainApp {
    pub fn login_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(egui::RichText::new("Login").size(36.0).strong());
            ui.label(egui::RichText::new("Please enter your API key.").size(20.0));

            ui.add(egui::TextEdit::singleline(&mut self.login_api_key));

            if ui.button("Submit").clicked() {
                self.local_credentials.api_key = self.login_api_key.clone();
                self.local_credentials.has_consented = false;
                self.has_scrolled_to_bottom_of_consent = false;
            }
        });
    }

    pub fn consent_view(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("consent_panel_top").show(ctx, |ui| {
            ui.heading(
                egui::RichText::new("Informed Consent & Terms of Service")
                    .size(36.0)
                    .strong(),
            );
            ui.label(
                egui::RichText::new("Please read the following information carefully.").size(20.0),
            );
        });

        egui::TopBottomPanel::bottom("consent_panel_bottom").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    self.local_credentials.logout();
                }

                if ui
                    .add_enabled(
                        self.has_scrolled_to_bottom_of_consent,
                        egui::Button::new("Accept"),
                    )
                    .clicked()
                {
                    self.local_credentials.has_consented = true;
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let output = egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(include_str!("consent.md"));
            });

            self.has_scrolled_to_bottom_of_consent |=
                (output.state.offset.y + output.inner_rect.height()) >= output.content_size.y;
        });
    }

    pub fn main_view(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(egui::RichText::new("Settings").size(36.0).strong());
            ui.label(egui::RichText::new("Configure your recording preferences").size(20.0));
            ui.add_space(10.0);

            egui::ScrollArea::vertical().show(ui, |ui| {
                // Account Section
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Account").size(18.0).strong());
                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label(format!("User ID: {}", &self.local_credentials.user_id));
                        if ui.button("Log out").clicked() {
                            self.local_credentials.logout();
                        }
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

                    // TODO: eventually implement a better keyboard shortcut system
                    ui.horizontal(|ui| {
                        ui.label("Start Recording:");
                        // ui.code(&mut self.local_preferences.start_recording_key);
                        ui.add_sized(
                            [60.0, 15.0],
                            egui::TextEdit::singleline(
                                &mut self.local_preferences.start_recording_key,
                            ),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.label("Stop Recording:");
                        // ui.code(&mut self.local_preferences.stop_recording_key);
                        ui.add_sized(
                            [60.0, 15.0],
                            egui::TextEdit::singleline(
                                &mut self.local_preferences.stop_recording_key,
                            ),
                        );
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
                        ui.label("Overlay Opacity:");
                        let mut stored_opacity = self.local_preferences.overlay_opacity;

                        let mut egui_opacity = stored_opacity as f32 / 255.0 * 100.0;
                        let r = ui.add(
                            egui::Slider::new(&mut egui_opacity, 0.0..=100.0)
                                .suffix("%")
                                .integer(),
                        );
                        if r.changed() {
                            stored_opacity = (egui_opacity / 100.0 * 255.0) as u8;
                            self.local_preferences.overlay_opacity = stored_opacity;
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label("Recording Audio Cue:");
                        let honk = self.local_preferences.honk;
                        ui.add(egui::Checkbox::new(
                            &mut self.local_preferences.honk,
                            match honk {
                                true => "Honk.",
                                false => "Honk?",
                            },
                        ));
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

                        // Cell 1: Total Uploaded
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::Center),
                            |ui| {
                                create_upload_cell(
                                    ui,
                                    "📊", // Icon
                                    "Total Uploaded",
                                    &self.upload_stats.get_total_duration_uploaded(),
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
                                    &self.upload_stats.get_total_files_uploaded(),
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
                                    &self.upload_stats.get_total_volume_uploaded(),
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
                                    &self.upload_stats.get_last_upload_date(),
                                );
                            },
                        );
                    });

                    ui.add_space(10.0);
                    if ui
                        .add_sized(
                            egui::vec2(ui.available_width(), 32.0),
                            egui::Button::new(
                                egui::RichText::new("Upload Recordings").size(12.0).strong(),
                            ),
                        )
                        .clicked()
                    {
                        // Handle upload
                        if !is_upload_bridge_running() {
                            let api_key = self.local_credentials.api_key.clone();
                            std::thread::spawn(move || {
                                start_upload_bridge(&api_key);
                            });
                        }
                    }
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

        match &self.rx.try_recv() {
            Ok(Command::UpdateUserID(uid)) => {
                println!("received uid {}", uid);
                self.local_credentials.user_id = uid.to_string();
            }
            _ => {}
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
