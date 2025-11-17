use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::{
    app_state::{
        AppState, AsyncRequest, ForegroundedGame, GitHubRelease, HotkeyRebindTarget,
        ListeningForNewHotkey, RecordingStatus,
    },
    config::{
        EncoderSettings, FfmpegNvencSettings, ObsAmfSettings, ObsQsvSettings, ObsX264Settings,
        Preferences, RecordingBackend,
    },
    ui::{util, views::App},
};

use constants::{GH_ORG, GH_REPO, encoding::VideoEncoderType};
use egui::{
    Align, Align2, Button, CentralPanel, Checkbox, Color32, ComboBox, Context, Direction,
    FontFamily, FontId, Frame, InnerResponse, Label, Layout, Margin, Response, RichText,
    ScrollArea, Slider, TextEdit, TextFormat, Ui, Vec2, Widget, WidgetText, Window,
    text::LayoutJob, vec2,
};

mod upload_manager;

#[derive(Default)]
pub(crate) struct MainViewState {
    pub(crate) last_obs_check: Option<(std::time::Instant, bool)>,
    /// Recording pending deletion confirmation (stores folder path and name)
    pub(crate) pending_delete_recording: Option<(PathBuf, String)>,
    /// Pending recording location move (stores old path and new path)
    pub(crate) pending_move_location: Option<(PathBuf, PathBuf)>,

    /// Upload manager state
    pub(crate) upload_manager: upload_manager::UploadManager,
}

const SETTINGS_TEXT_WIDTH: f32 = 150.0;
const SETTINGS_TEXT_HEIGHT: f32 = 20.0;

impl App {
    pub fn main_view(&mut self, ctx: &Context) {
        if self.main_view_state.last_obs_check.is_none()
            || self
                .main_view_state
                .last_obs_check
                .is_some_and(|(last, _)| last.elapsed() > Duration::from_secs(1))
        {
            self.main_view_state.last_obs_check = Some((Instant::now(), is_obs_running()));
        }

        CentralPanel::default().show(ctx, |ui| {
            // Show new release warning if available
            if let Some(release) = &self.newer_release_available {
                newer_release_available(ui, release);
                ui.add_space(4.0);
            }

            // Show OBS warning if necessary
            if self.local_preferences.recording_backend == RecordingBackend::Embedded
                && self
                    .main_view_state
                    .last_obs_check
                    .is_some_and(|(_, is_obs_running)| is_obs_running)
            {
                obs_running_warning(ui);
                ui.add_space(4.0);
            }

            if self.is_recording {
                recording_notice(ui, &self.app_state);
                ui.add_space(4.0);
            }

            ScrollArea::vertical().show(ui, |ui| {
                // Account Section
                ui.group(|ui| {
                    account_section(ui, self);
                });
                ui.add_space(4.0);

                // Keyboard Shortcuts Section
                ui.group(|ui| {
                    keyboard_shortcuts_section(ui, &self.app_state, &mut self.local_preferences);
                });
                ui.add_space(4.0);

                // Overlay Settings Section
                ui.group(|ui| {
                    let foregrounded_game = self
                        .app_state
                        .last_foregrounded_game
                        .read()
                        .unwrap()
                        .clone();
                    overlay_settings_section(
                        ui,
                        &mut self.local_preferences,
                        &self.available_video_encoders,
                        &mut self.encoder_settings_window_open,
                        foregrounded_game.as_ref(),
                        &self.available_cues,
                    );
                });
                ui.add_space(4.0);

                // Upload Manager Section
                ui.group(|ui| {
                    upload_manager::view(
                        ui,
                        &mut self.main_view_state.upload_manager,
                        &mut self.local_preferences,
                        &self.app_state,
                        &mut self.main_view_state.pending_delete_recording,
                        self.newer_release_available.is_some(),
                    );
                });

                // Logo
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
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
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.hyperlink_to(
                            RichText::new("Wayfarer Labs")
                                .italics()
                                .color(Color32::LIGHT_BLUE),
                            "https://wayfarerlabs.ai/",
                        );
                    });
                });
            });
        });

        // Encoder Settings Window
        encoder_settings_window(
            ctx,
            &mut self.encoder_settings_window_open,
            &mut self.local_preferences.encoder,
        );

        // Delete Confirmation Window
        delete_recording_confirmation_window(
            ctx,
            &mut self.main_view_state.pending_delete_recording,
            &self.app_state,
        );

        // Move Location Confirmation Window
        move_location_confirmation_window(
            ctx,
            &mut self.main_view_state.pending_move_location,
            &mut self.local_preferences.recording_location,
            &self.app_state,
        );
    }
}

fn account_section(ui: &mut Ui, app: &mut App) {
    ui.label(RichText::new("Account").size(18.0).strong());
    ui.separator();

    ui.vertical(|ui| {
        ui.label("User ID:");
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_sized(vec2(0.0, SETTINGS_TEXT_HEIGHT), Button::new("Log out"))
                    .clicked()
                {
                    app.go_to_login();
                }

                let user_id = app
                    .authenticated_user_id
                    .clone()
                    .unwrap_or_else(|| Ok("Authenticating...".to_string()))
                    .unwrap_or_else(|e| format!("Error: {e}"));
                ui.add_sized(
                    vec2(ui.available_width(), SETTINGS_TEXT_HEIGHT),
                    TextEdit::singleline(&mut user_id.as_str()),
                );
            });
        });
    });
}

fn keyboard_shortcuts_section(
    ui: &mut Ui,
    app_state: &AppState,
    local_preferences: &mut Preferences,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Keyboard Shortcuts")
                .size(18.0)
                .strong(),
        );

        let tooltip = concat!(
            "Tip: You can set separate hotkeys for starting and stopping recording. ",
            "By default, the start key will toggle recording.",
            "\n\n",
            "Recordings will automatically stop every 10 minutes to split them into smaller files. ",
            "This is intentional behaviour to prevent data loss and make uploads more manageable. ",
            "The recording will resume automatically after stopping, so you don't need to do anything."
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            util::tooltip(ui, tooltip, None);
        });
    });
    ui.separator();

    ui.horizontal(|ui| {
        add_settings_text(
            ui,
            Label::new(if local_preferences.stop_hotkey_enabled {
                "Start Recording:"
            } else {
                "Toggle Recording:"
            }),
        );
        let button_text = if app_state
            .listening_for_new_hotkey
            .read()
            .unwrap()
            .listening_hotkey_target()
            == Some(HotkeyRebindTarget::Start)
        {
            "Press any key...".to_string()
        } else {
            local_preferences.start_recording_key.clone()
        };

        if add_settings_widget(ui, Button::new(button_text)).clicked() {
            *app_state.listening_for_new_hotkey.write().unwrap() =
                ListeningForNewHotkey::Listening {
                    target: HotkeyRebindTarget::Start,
                };
        }
    });

    let stop_hotkey_enabled = local_preferences.stop_hotkey_enabled;
    if stop_hotkey_enabled {
        ui.horizontal(|ui| {
            add_settings_text(ui, Label::new("Stop Recording:"));
            let listening_hotkey_target = app_state
                .listening_for_new_hotkey
                .read()
                .unwrap()
                .listening_hotkey_target();
            let button_text = if listening_hotkey_target == Some(HotkeyRebindTarget::Stop) {
                "Press any key...".to_string()
            } else {
                local_preferences.stop_recording_key.clone()
            };

            if add_settings_widget(ui, Button::new(button_text)).clicked() {
                *app_state.listening_for_new_hotkey.write().unwrap() =
                    ListeningForNewHotkey::Listening {
                        target: HotkeyRebindTarget::Stop,
                    };
            }
        });
    }

    ui.horizontal(|ui| {
        add_settings_text(ui, Label::new("Stop Hotkey:"));
        add_settings_widget(
            ui,
            Checkbox::new(
                &mut local_preferences.stop_hotkey_enabled,
                match stop_hotkey_enabled {
                    true => "Enabled",
                    false => "Disabled",
                },
            ),
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn overlay_settings_section(
    ui: &mut Ui,
    local_preferences: &mut Preferences,
    available_video_encoders: &[VideoEncoderType],
    encoder_settings_window_open: &mut bool,
    foregrounded_game: Option<&ForegroundedGame>,
    available_cues: &[String],
) {
    ui.label(RichText::new("Recorder Customization").size(18.0).strong());
    ui.separator();

    // Display foregrounded game indicator
    ui.horizontal(|ui| {
        add_settings_text(ui, Label::new("Foregrounded Window:"));

        add_settings_ui(ui, |ui| {
            let Some(fg) = foregrounded_game else {
                ui.label(
                    RichText::new("No window detected")
                        .color(Color32::from_rgb(150, 150, 150))
                        .monospace(),
                );
                return;
            };

            ui.horizontal(|ui| {
                let (color, icon) = if fg.is_recordable() {
                    (Color32::from_rgb(100, 255, 100), "✅")
                } else {
                    (Color32::from_rgb(255, 100, 100), "❌")
                };

                ui.label(RichText::new(icon).color(color).strong());
                ui.label(
                    RichText::new(fg.exe_name.clone().unwrap_or("Unknown".to_string()))
                        .color(color)
                        .monospace(),
                );

                if let Some(reason) = fg.unsupported_reason.as_ref() {
                    ui.label(
                        RichText::new("(unsupported)")
                            .color(Color32::from_rgb(200, 100, 100))
                            .italics(),
                    )
                    .on_hover_text(reason);
                }
            });
        });
    });

    ui.horizontal(|ui| {
        add_settings_text(ui, Label::new("Overlay Location:"));
        add_settings_ui(ui, |ui| {
            ComboBox::from_id_salt("overlay_location")
                .selected_text(local_preferences.overlay_location.to_string())
                .show_ui(ui, |ui| {
                    for location in crate::config::OverlayLocation::ALL {
                        ui.selectable_value(
                            &mut local_preferences.overlay_location,
                            location,
                            location.to_string(),
                        );
                    }
                });
        });
    });

    ui.horizontal(|ui| {
        add_settings_text(ui, Label::new("Overlay Opacity:"));
        ui.scope(|ui| {
            // one day egui will make sliders respect their width properly
            ui.spacing_mut().slider_width = ui.available_width() - 50.0;
            u8_percentage_slider(ui, &mut local_preferences.overlay_opacity);
        });
    });

    ui.horizontal(|ui| {
        add_settings_text(ui, Label::new("Recording Audio Cue:"));
        add_settings_ui(ui, |ui| {
            ui.horizontal(|ui| {
                // Honk toggle
                let honk = local_preferences.honk;
                let _ = ui.add(Checkbox::new(
                    &mut local_preferences.honk,
                    match honk {
                        true => "Honk.",
                        false => "Honk?",
                    },
                ));

                ui.add_space(4.0);

                // Inline volume slider (0-255 mapped to 0-100%)
                u8_percentage_slider(ui, &mut local_preferences.honk_volume);
            });
        });
    });

    if local_preferences.honk {
        ui.horizontal(|ui| {
            add_settings_text(ui, Label::new("Recording Audio Cues:"));
            add_settings_ui(ui, |ui| {
                ComboBox::from_id_salt("start_recording_cue")
                    .selected_text(&local_preferences.audio_cues.start_recording)
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for cue in available_cues {
                            ui.selectable_value(
                                &mut local_preferences.audio_cues.start_recording,
                                cue.clone(),
                                cue,
                            );
                        }
                    });

                ComboBox::from_id_salt("stop_recording_cue")
                    .selected_text(&local_preferences.audio_cues.stop_recording)
                    .width(150.0)
                    .show_ui(ui, |ui| {
                        for cue in available_cues {
                            ui.selectable_value(
                                &mut local_preferences.audio_cues.stop_recording,
                                cue.clone(),
                                cue,
                            );
                        }
                    });
            });
        });
    }

    ui.horizontal(|ui| {
        add_settings_text(ui, Label::new("Video Encoder:"));
        add_settings_ui(ui, |ui| {
            let encoder_name = local_preferences.encoder.encoder.to_string();
            ComboBox::from_id_salt("video_encoder")
                .selected_text(&encoder_name)
                .width(150.0)
                .show_ui(ui, |ui| {
                    for encoder in available_video_encoders {
                        ui.selectable_value(
                            &mut local_preferences.encoder.encoder,
                            *encoder,
                            encoder.to_string(),
                        );
                    }
                });

            ui.horizontal(|ui| {
                if ui.button("⚙ Settings").clicked() {
                    *encoder_settings_window_open = true;
                }

                let tooltip = concat!(
                    "Consider turning on VSync and/or switching encoders and/or using a different preset if your recordings suffer from dropped frames.\n\n",
                    "NVENC is known to drop frames when the GPU is under heavy load or does not have enough VRAM. ",
                    "Turning on the in-game frame limiter will help reduce dropped frames."
                );

                util::tooltip(ui, tooltip, None)
            });
        });
    });
}

fn add_settings_text(ui: &mut Ui, widget: impl Widget) -> Response {
    ui.allocate_ui_with_layout(
        vec2(SETTINGS_TEXT_WIDTH, SETTINGS_TEXT_HEIGHT),
        Layout {
            main_dir: Direction::LeftToRight,
            main_wrap: false,
            main_align: Align::RIGHT,
            main_justify: true,
            cross_align: Align::Center,
            cross_justify: true,
        },
        |ui| ui.add(widget),
    )
    .inner
}

fn add_settings_ui<R>(ui: &mut Ui, add_contents: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), SETTINGS_TEXT_HEIGHT),
        Layout {
            main_dir: Direction::LeftToRight,
            main_wrap: false,
            main_align: Align::LEFT,
            main_justify: true,
            cross_align: Align::Center,
            cross_justify: true,
        },
        add_contents,
    )
}

fn add_settings_widget(ui: &mut Ui, widget: impl Widget) -> Response {
    add_settings_ui(ui, |ui| ui.add(widget)).inner
}

/// Helper function to create a percentage slider for a u8 value (0-255 -> 0-100%)
/// Returns true if the value was changed
fn u8_percentage_slider(ui: &mut Ui, value: &mut u8) -> bool {
    let mut percentage = (*value as f32 / 255.0 * 100.0).round();
    let response = ui.add(
        Slider::new(&mut percentage, 0.0..=100.0)
            .suffix("%")
            .integer(),
    );
    if response.changed() {
        *value = (percentage / 100.0 * 255.0).round() as u8;
        true
    } else {
        false
    }
}

fn newer_release_available(ui: &mut Ui, release: &GitHubRelease) {
    Frame::default()
        .fill(Color32::DARK_GREEN)
        .inner_margin(Margin::same(15))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("New Release Available!").size(20.0).strong());

                // Release name
                ui.label(RichText::new(&release.name).size(18.0).strong());

                // Release date if available
                if let Some(date) = &release.release_date {
                    ui.label(
                        RichText::new(format!("Released: {}", date.format("%B %d, %Y"))).size(14.0),
                    );
                }

                ui.add_space(4.0);

                ui.label(
                    RichText::new("Recording and uploading will be blocked until you update.")
                        .size(14.0),
                );

                ui.add_space(4.0);

                let button_width = 200.0;
                let button_height = 35.0;

                // Release notes button
                if ui
                    .add_sized(
                        vec2(button_width, button_height),
                        Button::new(
                            RichText::new("Release Notes")
                                .size(14.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(0x1D, 0x6D, 0xA7)),
                    )
                    .clicked()
                {
                    #[allow(clippy::collapsible_if)]
                    if let Err(e) = opener::open_browser(&release.release_notes_url) {
                        tracing::error!("Failed to open release notes URL: {}", e);
                    }
                }

                ui.add_space(4.0);

                // Download button
                if ui
                    .add_sized(
                        vec2(button_width, button_height),
                        Button::new(
                            RichText::new("Download Now")
                                .size(14.0)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(0x28, 0xA7, 0x1D)), // Green button
                    )
                    .clicked()
                {
                    #[allow(clippy::collapsible_if)]
                    if let Err(e) = opener::open_browser(&release.download_url) {
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

fn obs_running_warning(ui: &mut Ui) {
    Frame::default()
        .fill(Color32::from_rgb(220, 53, 69))
        .inner_margin(Margin::same(15))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("OBS Studio Detected!")
                        .size(20.0)
                        .strong()
                        .color(Color32::WHITE),
                );

                ui.add_space(4.0);

                ui.label(
                    RichText::new(
                        "OBS Studio is currently running and may conflict with OWL Control. \
                         Please close OBS Studio before using OWL Control for the best experience.",
                    )
                    .size(14.0)
                    .color(Color32::WHITE),
                );
            });
        });
}

fn recording_notice(ui: &mut Ui, app_state: &AppState) {
    let recording_status = app_state.state.read().unwrap().clone();
    Frame::default()
        .fill(Color32::from_rgb(147, 51, 234))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                let font_id = FontId::new(16.0, FontFamily::Proportional);
                let color = Color32::WHITE;
                let recording_text: WidgetText = match recording_status {
                    RecordingStatus::Stopped => RichText::new("Stopped")
                        .font(font_id)
                        .strong()
                        .color(color)
                        .into(),
                    RecordingStatus::Recording {
                        start_time,
                        game_exe,
                    } => {
                        let mut job = LayoutJob::default();
                        job.append(
                            "Recording ",
                            0.0,
                            TextFormat {
                                font_id: font_id.clone(),
                                color,
                                ..Default::default()
                            },
                        );
                        job.append(
                            &game_exe,
                            0.0,
                            TextFormat {
                                font_id: font_id.clone(),
                                italics: true,
                                color,
                                ..Default::default()
                            },
                        );
                        job.append(
                            &format!(
                                " ({})",
                                util::format_seconds(start_time.elapsed().as_secs())
                            ),
                            0.0,
                            TextFormat {
                                font_id,
                                color,
                                ..Default::default()
                            },
                        );
                        job.into()
                    }
                    RecordingStatus::Paused => RichText::new("Paused")
                        .font(font_id)
                        .strong()
                        .color(color)
                        .into(),
                };
                ui.label(recording_text);
            });
        });
}

fn encoder_settings_window(
    ctx: &Context,
    encoder_settings_window_open: &mut bool,
    encoder_settings: &mut EncoderSettings,
) {
    Window::new(format!("{} Settings", encoder_settings.encoder))
        .open(encoder_settings_window_open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| match encoder_settings.encoder {
            VideoEncoderType::X264 => encoder_settings_x264(ui, &mut encoder_settings.x264),
            VideoEncoderType::NvEnc => encoder_settings_nvenc(ui, &mut encoder_settings.nvenc),
            VideoEncoderType::Amf => encoder_settings_amf(ui, &mut encoder_settings.amf),
            VideoEncoderType::Qsv => encoder_settings_qsv(ui, &mut encoder_settings.qsv),
        });
}

const PRESET_TOOLTIP: &str = "Please keep this as high as possible for best quality; only reduce it if you're experiencing performance issues.";

fn encoder_settings_x264(ui: &mut Ui, x264_settings: &mut ObsX264Settings) {
    util::dropdown_list(
        ui,
        "Preset:",
        constants::encoding::X264_PRESETS,
        &mut x264_settings.preset,
        |ui| {
            util::tooltip(ui, PRESET_TOOLTIP, None);
        },
    );
}

fn encoder_settings_nvenc(ui: &mut Ui, nvenc_settings: &mut FfmpegNvencSettings) {
    util::dropdown_list(
        ui,
        "Preset:",
        constants::encoding::NVENC_PRESETS,
        &mut nvenc_settings.preset2,
        |ui| {
            util::tooltip(ui, PRESET_TOOLTIP, None);
        },
    );

    ui.add_space(5.0);
    util::dropdown_list(
        ui,
        "Tune:",
        constants::encoding::NVENC_TUNE_OPTIONS,
        &mut nvenc_settings.tune,
        |_| {},
    );
}

fn encoder_settings_qsv(ui: &mut Ui, qsv_settings: &mut ObsQsvSettings) {
    util::dropdown_list(
        ui,
        "Target Usage:",
        constants::encoding::QSV_TARGET_USAGES,
        &mut qsv_settings.target_usage,
        |_| {},
    );
}

fn encoder_settings_amf(ui: &mut Ui, amf_settings: &mut ObsAmfSettings) {
    util::dropdown_list(
        ui,
        "Preset:",
        constants::encoding::AMF_PRESETS,
        &mut amf_settings.preset,
        |_| {},
    );
}

fn delete_recording_confirmation_window(
    ctx: &Context,
    pending_delete_recording: &mut Option<(PathBuf, String)>,
    app_state: &AppState,
) {
    if let Some((folder_path, folder_name)) = pending_delete_recording.clone() {
        let mut keep_open = true;
        Window::new("Confirm Deletion of Unuploaded Recording")
            .open(&mut keep_open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "The recording \"{folder_name}\" has not been uploaded yet."
                        ))
                        .size(13.0)
                        .color(Color32::from_rgb(255, 255, 100)),
                    );

                    ui.label(
                        RichText::new(
                            "You can still upload it by clicking the \"Upload Recordings\" button.",
                        )
                        .size(13.0),
                    );

                    ui.add_space(12.0);

                    // Open folder button
                    if ui
                        .add_sized(
                            vec2(ui.available_width(), 32.0),
                            Button::new(
                                RichText::new("📁 Open Folder for Investigation")
                                    .size(13.0)
                                    .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(100, 150, 255)),
                        )
                        .clicked()
                    {
                        app_state
                            .async_request_tx
                            .blocking_send(AsyncRequest::OpenFolder(folder_path.clone()))
                            .ok();
                    }

                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        // Cancel button
                        if ui
                            .add_sized(
                                vec2(ui.available_width() / 2.0, 32.0),
                                Button::new(RichText::new("Cancel").size(13.0)),
                            )
                            .clicked()
                        {
                            *pending_delete_recording = None;
                        }

                        // Really Delete button
                        if ui
                            .add_sized(
                                vec2(ui.available_width(), 32.0),
                                Button::new(
                                    RichText::new("Really Delete")
                                        .size(13.0)
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(180, 60, 60)),
                            )
                            .clicked()
                        {
                            app_state
                                .async_request_tx
                                .blocking_send(AsyncRequest::DeleteRecording(folder_path.clone()))
                                .ok();
                            *pending_delete_recording = None;
                        }
                    });
                });
            });

        if !keep_open {
            *pending_delete_recording = None;
        }
    }
}

fn move_location_confirmation_window(
    ctx: &Context,
    pending_move_location: &mut Option<(PathBuf, PathBuf)>,
    recording_location: &mut PathBuf,
    app_state: &AppState,
) {
    let Some((old_path, new_path)) = pending_move_location.clone() else {
        return;
    };

    let mut keep_open = true;
    Window::new("Move Recording Location")
        .open(&mut keep_open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(
                        "Would you like to move your existing recordings to the new location?",
                    )
                    .size(14.0),
                );

                ui.add_space(4.0);

                ui.label(
                    RichText::new("From:")
                        .size(12.0)
                        .color(Color32::from_rgb(200, 200, 200)),
                );
                ui.label(
                    RichText::new(old_path.to_string_lossy())
                        .size(12.0)
                        .color(Color32::WHITE),
                );

                ui.add_space(4.0);

                ui.label(
                    RichText::new("To:")
                        .size(12.0)
                        .color(Color32::from_rgb(200, 200, 200)),
                );
                ui.label(
                    RichText::new(new_path.to_string_lossy())
                        .size(12.0)
                        .color(Color32::WHITE),
                );

                ui.add_space(12.0);

                let intra_spacing = 4.0;

                ui.horizontal(|ui| {
                    // Don't Move button - just change the location without moving files
                    if ui
                        .add_sized(
                            vec2(ui.available_width() / 2.0, 32.0),
                            Button::new(RichText::new("Don't Move Files").size(13.0)),
                        )
                        .on_hover_text(
                            "Only update the recording location without moving existing files",
                        )
                        .clicked()
                    {
                        *recording_location = new_path.clone();
                        *pending_move_location = None;
                    }

                    // Move Files button
                    if ui
                        .add_sized(
                            vec2(ui.available_width(), 32.0),
                            Button::new(
                                RichText::new("Move Files").size(13.0).color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(100, 150, 255)),
                        )
                        .on_hover_text("Move all existing recordings to the new location")
                        .clicked()
                    {
                        *recording_location = new_path.clone();
                        app_state
                            .async_request_tx
                            .blocking_send(AsyncRequest::MoveRecordingsFolder {
                                from: old_path.clone(),
                                to: new_path.clone(),
                            })
                            .ok();
                        *pending_move_location = None;
                    }
                });

                ui.add_space(intra_spacing);

                // Cancel button
                if ui
                    .add_sized(
                        vec2(ui.available_width(), 28.0),
                        Button::new(RichText::new("Cancel").size(12.0)),
                    )
                    .clicked()
                {
                    *pending_move_location = None;
                }
            });
        });

    if !keep_open {
        *pending_move_location = None;
    }
}
