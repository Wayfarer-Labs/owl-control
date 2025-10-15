use crate::{
    api::{UserUpload, UserUploadStatistics},
    app_state::{AsyncRequest, GitHubRelease},
    ui::{HEADING_TEXT_SIZE, HotkeyRebindTarget, MainApp, SUBHEADING_TEXT_SIZE, util},
};

use constants::{GH_ORG, GH_REPO};

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

            // Show new release warning if available
            if let Some(release) = &self.newer_release_available {
                newer_release_available(ui, release);

                ui.add_space(15.0);
            }

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

                    let user_uploads = self.app_state.user_uploads.read().unwrap().clone();
                    ui.horizontal(|ui| {
                        upload_stats(
                            ui,
                            user_uploads
                                .as_ref()
                                .map(|u| (&u.statistics, u.uploads.as_slice())),
                        );
                    });
                    ui.add_space(8.0);

                    egui::CollapsingHeader::new(egui::RichText::new("History").size(16.0))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.add_space(4.0);
                            uploads_view(ui, user_uploads.as_ref().map(|u| u.uploads.as_slice()));
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
                            opener::open_browser(format!(
                                "https://github.com/{GH_ORG}/{GH_REPO}/blob/main/GAMES.md"
                            ))
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
                        ui.hyperlink_to(
                            egui::RichText::new("Wayfarer Labs")
                                .italics()
                                .color(egui::Color32::LIGHT_BLUE),
                            "https://wayfarerlabs.ai/",
                        );
                    });
                });
            });
        });
    }
}

fn newer_release_available(ui: &mut egui::Ui, release: &GitHubRelease) {
    egui::Frame::default()
        .fill(egui::Color32::DARK_GREEN)
        .inner_margin(egui::Margin::same(15))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("New Release Available!")
                        .size(20.0)
                        .strong(),
                );

                // Release name
                ui.label(egui::RichText::new(&release.name).size(16.0).strong());

                // Release date if available
                if let Some(date) = &release.release_date {
                    ui.label(
                        egui::RichText::new(format!("Released: {}", date.format("%B %d, %Y")))
                            .size(12.0),
                    );
                }

                ui.add_space(8.0);

                // Download button
                if ui
                    .add_sized(
                        egui::vec2(200.0, 35.0),
                        egui::Button::new(
                            egui::RichText::new("Download Now")
                                .size(14.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(40, 167, 69)), // Green button
                    )
                    .clicked()
                {
                    #[allow(clippy::collapsible_if)]
                    if let Err(e) = opener::open_browser(&release.url) {
                        tracing::error!("Failed to open release URL: {}", e);
                    }
                }
            });
        });
}

fn upload_stats(
    ui: &mut egui::Ui,
    stats_and_uploads: Option<(&UserUploadStatistics, &[UserUpload])>,
) {
    let (stats, uploads) = stats_and_uploads.unzip();
    let available_width = ui.available_width() - 40.0;
    let cell_width = available_width / 4.0;

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

    // Cell 1: Total Uploaded
    ui.allocate_ui_with_layout(
        egui::vec2(cell_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            create_upload_cell(
                ui,
                "📊", // Icon
                "Total Uploaded",
                &stats
                    .map(|s| util::format_seconds(s.total_video_time.seconds as u64))
                    .unwrap_or_else(|| "Loading...".to_string()),
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
                &stats
                    .map(|s| s.total_uploads.to_string())
                    .unwrap_or_else(|| "Loading...".to_string()),
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
                &util::format_bytes(stats.map(|s| s.total_data.bytes).unwrap_or_else(|| 0)),
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
                &uploads
                    .map(|u| {
                        u.first()
                            .map(|upload| upload.created_at.with_timezone(&chrono::Local))
                            .map(util::format_datetime)
                            .unwrap_or_else(|| "Never".to_string())
                    })
                    .unwrap_or_else(|| "Loading...".to_string()),
            );
        },
    );
    ui.add_space(10.0);
}

fn uploads_view(ui: &mut egui::Ui, uploads: Option<&[UserUpload]>) {
    // Scrollable upload history section
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 4,
            right: 12,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            let height = 60.0;
            let Some(uploads) = uploads else {
                ui.vertical_centered(|ui| {
                    ui.add(egui::widgets::Spinner::new().size(height));
                });
                return;
            };

            egui::ScrollArea::vertical()
                .max_height(height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if uploads.is_empty() {
                        ui.label("No uploads yet");
                        return;
                    }
                    for upload in uploads.iter() {
                        egui::Frame::new()
                            .fill(ui.visuals().faint_bg_color)
                            .inner_margin(4.0)
                            .corner_radius(4.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Filename
                                    ui.add(egui::TextEdit::singleline(&mut upload.id.as_str()));

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            // Timestamp
                                            let local_time =
                                                upload.created_at.with_timezone(&chrono::Local);
                                            ui.label(
                                                local_time.format("%Y-%m-%d %H:%M:%S").to_string(),
                                            );

                                            // File size
                                            ui.label(format!("{:.2} MB", upload.file_size_mb));

                                            // Duration if available
                                            if let Some(duration) = upload.video_duration_seconds {
                                                ui.label(format!("{:.1}s", duration));
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(4.0);
                    }
                });
        });
}
