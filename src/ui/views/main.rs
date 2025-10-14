use crate::{
    app_state::AsyncRequest,
    config::UploadStats,
    ui::{HEADING_TEXT_SIZE, HotkeyRebindTarget, MainApp, SUBHEADING_TEXT_SIZE, util},
};

impl MainApp {
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

        fn add_settings_ui<R>(
            ui: &mut egui::Ui,
            add_contents: impl FnOnce(&mut egui::Ui) -> R,
        ) -> egui::InnerResponse<R> {
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
                add_contents,
            )
        }

        fn add_settings_widget(ui: &mut egui::Ui, widget: impl egui::Widget) -> egui::Response {
            add_settings_ui(ui, |ui| ui.add(widget)).inner
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

                                    let user_id = self
                                        .authenticated_user_id
                                        .clone()
                                        .unwrap_or_else(|| Ok("Authenticating...".to_string()))
                                        .unwrap_or_else(|e| format!("Error: {e}"));
                                    ui.add_sized(
                                        egui::vec2(ui.available_width(), SETTINGS_TEXT_HEIGHT),
                                        egui::TextEdit::singleline(&mut user_id.as_str()),
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
                        let button_text = if self.listening_for_hotkey_rebind
                            == Some(HotkeyRebindTarget::Start)
                        {
                            "Press any key...".to_string()
                        } else {
                            self.local_preferences.start_recording_key.clone()
                        };

                        if add_settings_widget(ui, egui::Button::new(button_text)).clicked() {
                            self.listening_for_hotkey_rebind = Some(HotkeyRebindTarget::Start);
                        }
                    });

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Stop Recording:"));
                        let button_text =
                            if self.listening_for_hotkey_rebind == Some(HotkeyRebindTarget::Stop) {
                                "Press any key...".to_string()
                            } else {
                                self.local_preferences.stop_recording_key.clone()
                            };

                        if add_settings_widget(ui, egui::Button::new(button_text)).clicked() {
                            self.listening_for_hotkey_rebind = Some(HotkeyRebindTarget::Stop);
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
                        add_settings_text(ui, egui::Label::new("Overlay Location:"));
                        add_settings_ui(ui, |ui| {
                            egui::ComboBox::from_id_salt("overlay_location")
                                .selected_text(self.local_preferences.overlay_location.to_string())
                                .show_ui(ui, |ui| {
                                    for location in crate::config::OverlayLocation::ALL {
                                        ui.selectable_value(
                                            &mut self.local_preferences.overlay_location,
                                            location,
                                            location.to_string(),
                                        );
                                    }
                                });
                        });
                    });

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
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("Upload Manager").size(18.0).strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(egui::RichText::new("Open Recordings Folder").size(12.0))
                                .clicked()
                            {
                                self.app_state
                                    .async_request_tx
                                    .blocking_send(AsyncRequest::OpenDataDump)
                                    .ok();
                            }
                        });
                    });
                    ui.separator();
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        let stats = self.app_state.upload_stats.read().unwrap().clone();
                        if let Some(stats) = stats {
                            upload_stats(ui, &stats);
                        } else {
                            ui.label(
                                egui::RichText::new("Loading upload stats...")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(128, 128, 128)),
                            );
                        }
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
                            self.last_upload_error = None;
                            self.app_state
                                .async_request_tx
                                .blocking_send(AsyncRequest::UploadData)
                                .ok();
                        }
                        if let Some(error) = &self.last_upload_error {
                            ui.label(
                                egui::RichText::new(error)
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(255, 0, 0)),
                            );
                        }
                    });
                });

                // Logo
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        if ui.button("FAQ").clicked() {
                            opener::open_browser(
                                "https://github.com/Wayfarer-Labs/owl-control/blob/main/GAMES.md",
                            )
                            .ok();
                        }
                        if ui.button("Logs").clicked() {
                            self.app_state
                                .async_request_tx
                                .blocking_send(AsyncRequest::OpenLog)
                                .ok();
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new("Wayfarer Labs")
                                .italics()
                                .color(egui::Color32::LIGHT_BLUE),
                        );
                    });
                });
            });
        });
    }
}

fn upload_stats(ui: &mut egui::Ui, upload_stats: &UploadStats) {
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
                &util::format_seconds(upload_stats.total_duration_uploaded as u64),
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
                    .map(util::format_datetime)
                    .unwrap_or_else(|| "Never".to_string()),
            );
        },
    );

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
}
