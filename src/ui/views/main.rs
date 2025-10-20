use std::time::{Duration, Instant};

use crate::{
    api::{UserUpload, UserUploadStatistics},
    app_state::{AsyncRequest, GitHubRelease},
    config::{EncoderSettings, FfmpegNvencSettings, ObsX264Settings, RecordingBackend},
    ui::{HEADING_TEXT_SIZE, HotkeyRebindTarget, MainApp, SUBHEADING_TEXT_SIZE, util},
};

use constants::encoding::{SUPPORTED_VIDEO_ENCODERS, VideoEncoderType};
use constants::{GH_ORG, GH_REPO};

#[derive(Default)]
pub(crate) struct MainViewState {
    last_obs_check: Option<(std::time::Instant, bool)>,
}

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

        if self.main_view_state.last_obs_check.is_none()
            || self
                .main_view_state
                .last_obs_check
                .is_some_and(|(last, _)| last.elapsed() > Duration::from_secs(1))
        {
            self.main_view_state.last_obs_check = Some((Instant::now(), is_obs_running()));
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

            // Show OBS warning if necessary
            if self.local_preferences.recording_backend == RecordingBackend::Embedded
                && self
                    .main_view_state
                    .last_obs_check
                    .is_some_and(|(_, is_obs_running)| is_obs_running)
            {
                obs_running_warning(ui);

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
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("Keyboard Shortcuts")
                                .size(18.0)
                                .strong(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new("ℹ").size(16.0).color(egui::Color32::from_rgb(128, 128, 128)))
                            )
                            .on_hover_cursor(egui::CursorIcon::Help)
                            .on_hover_text("Tip: You can set separate hotkeys for starting and stopping recording. By default, the start key will toggle recording.");
                        });
                    });
                    ui.separator();

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new(if self.local_preferences.stop_hotkey_enabled {
                            "Start Recording:"
                        } else {
                            "Toggle Recording:"
                        }));
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

                    let stop_hotkey_enabled = self.local_preferences.stop_hotkey_enabled;
                    if stop_hotkey_enabled {
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
                    }

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Stop Hotkey:"));
                        add_settings_widget(
                            ui,
                            egui::Checkbox::new(
                                &mut self.local_preferences.stop_hotkey_enabled,
                                match stop_hotkey_enabled {
                                    true => "Enabled",
                                    false => "Disabled",
                                },
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
                    });

                    ui.horizontal(|ui| {
                        add_settings_text(ui, egui::Label::new("Video Encoder:"));
                        add_settings_ui(ui, |ui| {
                            let encoder_name = self.local_preferences.encoder.encoder.to_string();
                            egui::ComboBox::from_id_salt("video_encoder")
                                .selected_text(&encoder_name)
                                .width(150.0)
                                .show_ui(ui, |ui| {
                                    for encoder in SUPPORTED_VIDEO_ENCODERS {
                                        ui.selectable_value(
                                            &mut self.local_preferences.encoder.encoder,
                                            encoder,
                                            encoder.to_string(),
                                        );
                                    }
                                });

                            if ui.button("⚙ Settings").clicked() {
                                self.encoder_settings_window_open = true;
                            }
                        });
                    });
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

                    // Unified Recordings Section
                    let local_recordings = self.app_state.local_recordings.read().unwrap();
                    let failed_count = local_recordings.iter()
                        .filter(|r| matches!(r, crate::upload::LocalRecording::Invalid { .. }))
                        .count();
                    egui::CollapsingHeader::new(
                        if failed_count > 0 {
                            egui::RichText::new(format!("Upload Tracker ({} failed)", failed_count))
                                .size(16.0)
                        } else {
                            egui::RichText::new("Upload Tracker").size(16.0)
                        }
                    )
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.add_space(4.0);

                        // Unified view with both successful and failed recordings
                        unified_recordings_view(
                            ui,
                            user_uploads.as_ref().map(|u| u.uploads.as_slice()),
                            &local_recordings,
                            &self.app_state,
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
                    ui.add_space(5.0);
                    ui.horizontal(|ui| {
                        ui.add(egui::Checkbox::new(
                            &mut self.local_preferences.unreliable_connection,
                            "Optimize for unreliable connections",
                        ));
                        ui.add(
                            egui::Label::new(egui::RichText::new("ℹ").size(14.0).color(egui::Color32::from_rgb(128, 128, 128)))
                        )
                        .on_hover_cursor(egui::CursorIcon::Help)
                        .on_hover_text(concat!(
                            "Enable this if you have a slow or unstable internet connection. ",
                            "This will use smaller file chunks to improve upload success rates."
                        ));
                    });

                    // Delete Uploaded Recordings Setting
                    ui.horizontal(|ui| {
                        ui.add(egui::Checkbox::new(
                            &mut self.local_preferences.delete_uploaded_files,
                            "Delete recordings after successful upload",
                        ));
                        ui.add(
                            egui::Label::new(egui::RichText::new("ℹ").size(14.0).color(egui::Color32::from_rgb(128, 128, 128)))
                        )
                        .on_hover_cursor(egui::CursorIcon::Help)
                        .on_hover_text(concat!(
                            "Automatically delete local recordings after they have been successfully uploaded. ",
                            "Invalid uploads, as well as existing uploads, will not be deleted."
                        ));
                    });

                    // Upload Button
                    ui.add_space(5.0);
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

        // Encoder Settings Window
        egui::Window::new(format!(
            "{} Settings",
            self.local_preferences.encoder.encoder
        ))
        .open(&mut self.encoder_settings_window_open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            encoder_settings_window(ui, &mut self.local_preferences.encoder);
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

/// Check if any OBS Studio processes are currently running
fn is_obs_running() -> bool {
    let mut is_obs_running = false;

    game_process::for_each_process(|process| {
        let exe_name = unsafe { std::ffi::CStr::from_ptr(process.szExeFile.as_ptr()) };
        let Some(file_name) = exe_name
            .to_str()
            .ok()
            .map(std::path::Path::new)
            .and_then(|p| p.file_name())
            .and_then(|f| f.to_str())
            .map(|f| f.to_ascii_lowercase())
        else {
            return true;
        };

        if ["obs.exe", "obs64.exe", "obs32.exe"].contains(&file_name.as_str()) {
            is_obs_running = true;
            return false;
        }

        true
    })
    .ok();

    is_obs_running
}

fn obs_running_warning(ui: &mut egui::Ui) {
    egui::Frame::default()
        .fill(egui::Color32::from_rgb(220, 53, 69))
        .inner_margin(egui::Margin::same(15))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("OBS Studio Detected!")
                        .size(20.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                );

                ui.add_space(8.0);

                ui.label(
                    egui::RichText::new(
                        "OBS Studio is currently running and may conflict with OWL Control. \
                         Please close OBS Studio before using OWL Control for the best experience.",
                    )
                    .size(14.0)
                    .color(egui::Color32::WHITE),
                );
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

enum RecordingEntry<'a> {
    Successful(&'a UserUpload),
    Local(&'a crate::upload::LocalRecording),
}

impl<'a> RecordingEntry<'a> {
    fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            RecordingEntry::Successful(upload) => upload.created_at,
            RecordingEntry::Local(recording) => recording
                .timestamp()
                .map(chrono::DateTime::<chrono::Utc>::from)
                .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from(std::time::UNIX_EPOCH)),
        }
    }
}

fn unified_recordings_view(
    ui: &mut egui::Ui,
    uploads: Option<&[UserUpload]>,
    local_recordings: &[crate::upload::LocalRecording],
    app_state: &crate::app_state::AppState,
) {
    const FONTSIZE: f32 = 13.0;
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 4,
            right: 12,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            let height = 150.0;

            // Show spinner if still loading
            if uploads.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add(egui::widgets::Spinner::new().size(height));
                });
                return;
            }

            // Delete All Failed button (only show if there are failed recordings)
            let failed_count = local_recordings.iter()
                .filter(|r| matches!(r, crate::upload::LocalRecording::Invalid { .. }))
                .count();
            if failed_count > 0 {
                if ui
                    .add_sized(
                        egui::vec2(ui.available_width(), 28.0),
                        egui::Button::new(
                            egui::RichText::new("Delete Failed Recordings")
                                .size(FONTSIZE)
                                .color(egui::Color32::WHITE),
                        )
                        .fill(egui::Color32::from_rgb(180, 60, 60)),
                    )
                    .clicked()
                {
                    // Send async request to delete all invalid recordings
                    app_state
                        .async_request_tx
                        .blocking_send(crate::app_state::AsyncRequest::DeleteAllInvalidRecordings)
                        .ok();
                }
                ui.add_space(8.0);
            }

            // Merge and sort recordings
            let mut entries: Vec<RecordingEntry> = Vec::new();

            if let Some(uploads) = uploads {
                entries.extend(uploads.iter().map(RecordingEntry::Successful));
            }
            entries.extend(local_recordings.iter().map(RecordingEntry::Local));

            // Sort by timestamp, most recent first
            entries.sort_by_key(|b| std::cmp::Reverse(b.timestamp()));

            egui::ScrollArea::vertical()
                .max_height(height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if entries.is_empty() {
                        ui.label("No recordings yet");
                        return;
                    }

                    for entry in entries.iter() {
                        match entry {
                            RecordingEntry::Successful(upload) => {
                                // Successful upload entry
                                egui::Frame::new()
                                    .fill(ui.visuals().faint_bg_color)
                                    .inner_margin(4.0)
                                    .corner_radius(4.0)
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            // Success indicator
                                            ui.label(
                                                egui::RichText::new("✔")
                                                    .size(FONTSIZE)
                                                    .color(egui::Color32::from_rgb(100, 255, 100)),
                                            );

                                            // Filename
                                            ui.label(egui::RichText::new(upload.id.as_str())
                                                        .size(FONTSIZE));

                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    // Timestamp
                                                    let local_time =
                                                        upload.created_at.with_timezone(&chrono::Local);
                                                    ui.label(
                                                        egui::RichText::new(local_time.format("%Y-%m-%d %H:%M:%S").to_string())
                                                        .size(FONTSIZE)
                                                    );

                                                    // File size
                                                    ui.label(egui::RichText::new(format!("{:.2} MB", upload.file_size_mb))
                                                        .size(FONTSIZE));

                                                    // Duration if available
                                                    if let Some(duration) = upload.video_duration_seconds {
                                                        ui.label(egui::RichText::new(format!("{:.1}s", duration))
                                                        .size(FONTSIZE));
                                                    }
                                                },
                                            );
                                        });
                                    });
                            }
                            RecordingEntry::Local(recording) => {
                                match recording {
                                    crate::upload::LocalRecording::Invalid { folder_name, folder_path, error_reasons, timestamp } => {
                                        // Failed upload entry
                                        egui::Frame::new()
                                            .fill(egui::Color32::from_rgb(80, 40, 40))
                                            .inner_margin(4.0)
                                            .corner_radius(4.0)
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    // Failure indicator
                                                    ui.label(
                                                        egui::RichText::new("❌")
                                                            .size(FONTSIZE)
                                                            .color(egui::Color32::from_rgb(255, 100, 100)),
                                                    );

                                                    // Folder name (clickable to open folder)
                                                    if ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(folder_name)
                                                                .size(FONTSIZE)
                                                                .color(egui::Color32::from_rgb(255, 200, 200))
                                                                .underline()
                                                        )
                                                        .sense(egui::Sense::click())
                                                    )
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .clicked()
                                                    {
                                                        app_state
                                                            .async_request_tx
                                                            .blocking_send(
                                                                crate::app_state::AsyncRequest::OpenFolder(
                                                                    folder_path.clone()
                                                                ),
                                                            )
                                                            .ok();
                                                    }

                                                    // Info icon with error tooltip
                                                    ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new("ℹ")
                                                                .size(FONTSIZE)
                                                                .color(egui::Color32::from_rgb(255, 150, 150))
                                                        )
                                                    )
                                                    .on_hover_cursor(egui::CursorIcon::Help)
                                                    .on_hover_text({
                                                        let mut tooltip_text = String::from("Validation errors:\n");
                                                        for (i, reason) in error_reasons.iter().enumerate() {
                                                            if i > 0 {
                                                                tooltip_text.push('\n');
                                                            }
                                                            tooltip_text.push_str("• ");
                                                            tooltip_text.push_str(reason);
                                                        }
                                                        tooltip_text
                                                    });

                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(egui::Align::Center),
                                                        |ui| {
                                                            // Timestamp if available
                                                            if let Some(timestamp) = timestamp {
                                                                let datetime =
                                                                    chrono::DateTime::<chrono::Utc>::from(*timestamp);
                                                                let local_time =
                                                                    datetime.with_timezone(&chrono::Local);
                                                                ui.label(
                                                                    egui::RichText::new(
                                                                        local_time
                                                                            .format("%Y-%m-%d %H:%M:%S")
                                                                            .to_string(),
                                                                    )
                                                                    .size(FONTSIZE)
                                                                    .color(egui::Color32::from_rgb(
                                                                        200, 200, 200,
                                                                    )),
                                                                );
                                                            }

                                                            // Delete button
                                                            if ui
                                                                .add_sized(
                                                                    egui::vec2(60.0, 20.0),
                                                                    egui::Button::new(
                                                                        egui::RichText::new("Delete")
                                                                            .size(FONTSIZE)
                                                                            .color(egui::Color32::WHITE),
                                                                    )
                                                                    .fill(egui::Color32::from_rgb(180, 60, 60)),
                                                                )
                                                                .clicked()
                                                            {
                                                                if let Err(e) =
                                                                    std::fs::remove_dir_all(folder_path)
                                                                {
                                                                    tracing::error!(
                                                                        "Failed to delete invalid recording folder {}: {:?}",
                                                                        folder_path.display(),
                                                                        e
                                                                    );
                                                                } else {
                                                                    tracing::info!(
                                                                        "Deleted invalid recording folder: {}",
                                                                        folder_path.display()
                                                                    );
                                                                    app_state
                                                                        .async_request_tx
                                                                        .blocking_send(
                                                                            crate::app_state::AsyncRequest::LoadLocalRecordings,
                                                                        )
                                                                        .ok();
                                                                }
                                                            }
                                                        },
                                                    );
                                                });
                                            });
                                    }
                                    crate::upload::LocalRecording::Unuploaded { folder_name, folder_path, timestamp } => {
                                        // Unuploaded entry
                                        egui::Frame::new()
                                            .fill(egui::Color32::from_rgb(60, 60, 80))
                                            .inner_margin(4.0)
                                            .corner_radius(4.0)
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    // Pending indicator
                                                    ui.label(
                                                        egui::RichText::new("⏳")
                                                            .size(FONTSIZE)
                                                            .color(egui::Color32::from_rgb(200, 200, 255)),
                                                    );

                                                    // Folder name (clickable to open folder)
                                                    if ui.add(
                                                        egui::Label::new(
                                                            egui::RichText::new(folder_name)
                                                                .size(FONTSIZE)
                                                                .color(egui::Color32::from_rgb(200, 200, 255))
                                                                .underline()
                                                        )
                                                        .sense(egui::Sense::click())
                                                    )
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .clicked()
                                                    {
                                                        app_state
                                                            .async_request_tx
                                                            .blocking_send(
                                                                crate::app_state::AsyncRequest::OpenFolder(
                                                                    folder_path.clone()
                                                                ),
                                                            )
                                                            .ok();
                                                    }

                                                    // "Pending upload" label
                                                    ui.label(
                                                        egui::RichText::new("(pending upload)")
                                                            .size(FONTSIZE - 1.0)
                                                            .color(egui::Color32::from_rgb(150, 150, 200))
                                                            .italics()
                                                    );

                                                    ui.with_layout(
                                                        egui::Layout::right_to_left(egui::Align::Center),
                                                        |ui| {
                                                            // Timestamp if available
                                                            if let Some(timestamp) = timestamp {
                                                                let datetime =
                                                                    chrono::DateTime::<chrono::Utc>::from(*timestamp);
                                                                let local_time =
                                                                    datetime.with_timezone(&chrono::Local);
                                                                ui.label(
                                                                    egui::RichText::new(
                                                                        local_time
                                                                            .format("%Y-%m-%d %H:%M:%S")
                                                                            .to_string(),
                                                                    )
                                                                    .size(FONTSIZE)
                                                                    .color(egui::Color32::from_rgb(
                                                                        200, 200, 200,
                                                                    )),
                                                                );
                                                            }

                                                            // Delete button
                                                            if ui
                                                                .add_sized(
                                                                    egui::vec2(60.0, 20.0),
                                                                    egui::Button::new(
                                                                        egui::RichText::new("Delete")
                                                                            .size(FONTSIZE)
                                                                            .color(egui::Color32::WHITE),
                                                                    )
                                                                    .fill(egui::Color32::from_rgb(100, 100, 120)),
                                                                )
                                                                .clicked()
                                                            {
                                                                if let Err(e) =
                                                                    std::fs::remove_dir_all(folder_path)
                                                                {
                                                                    tracing::error!(
                                                                        "Failed to delete unuploaded recording folder {}: {:?}",
                                                                        folder_path.display(),
                                                                        e
                                                                    );
                                                                } else {
                                                                    tracing::info!(
                                                                        "Deleted unuploaded recording folder: {}",
                                                                        folder_path.display()
                                                                    );
                                                                    app_state
                                                                        .async_request_tx
                                                                        .blocking_send(
                                                                            crate::app_state::AsyncRequest::LoadLocalRecordings,
                                                                        )
                                                                        .ok();
                                                                }
                                                            }
                                                        },
                                                    );
                                                });
                                            });
                                    }
                                }
                            }
                        }
                        ui.add_space(4.0);
                    }
                });
        });
}

fn encoder_settings_window(ui: &mut egui::Ui, encoder_settings: &mut EncoderSettings) {
    match encoder_settings.encoder {
        VideoEncoderType::X264 => encoder_settings_x264(ui, &mut encoder_settings.x264),
        VideoEncoderType::NvEnc => encoder_settings_nvenc(ui, &mut encoder_settings.nvenc),
    }
}

fn encoder_settings_x264(ui: &mut egui::Ui, x264_settings: &mut ObsX264Settings) {
    dropdown_list(
        ui,
        "Preset:",
        &constants::encoding::X264_PRESETS,
        &mut x264_settings.preset,
    );
}

fn encoder_settings_nvenc(ui: &mut egui::Ui, nvenc_settings: &mut FfmpegNvencSettings) {
    dropdown_list(
        ui,
        "Preset:",
        &constants::encoding::NVENC_PRESETS,
        &mut nvenc_settings.preset2,
    );

    ui.add_space(5.0);
    dropdown_list(
        ui,
        "Tune:",
        &constants::encoding::NVENC_TUNE_OPTIONS,
        &mut nvenc_settings.tune,
    );
}

fn dropdown_list(
    ui: &mut egui::Ui,
    label: &str,
    options: &[&str],
    selected: &mut String,
) -> egui::Response {
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(label)
            .selected_text(selected.as_str())
            .show_ui(ui, |ui| {
                for option in options {
                    ui.selectable_value(selected, option.to_string(), *option);
                }
            });
    })
    .response
}
