use egui::{
    Align, Button, Checkbox, CollapsingHeader, Color32, CursorIcon, Frame, Label, Layout, Margin,
    ProgressBar, Response, RichText, ScrollArea, Sense, TextEdit, TextWrapMode, Ui, Vec2, vec2,
    widgets::Spinner,
};

use crate::{
    api::{UserUpload, UserUploadStatistics},
    app_state::{AppState, AsyncRequest},
    config::Preferences,
    output_types::Metadata,
    record::{LocalRecording, LocalRecordingInfo, LocalRecordingPaused},
    ui::{util, views::main::FOOTER_HEIGHT},
    upload,
};

#[derive(Default)]
pub struct UploadManager {
    recordings: Recordings,
    virtual_list: egui_virtual_list::VirtualList,
    current_upload_progress: Option<upload::ProgressData>,
    last_upload_error: Option<String>,
    /// Number of recordings queued for auto-upload
    auto_upload_queue_count: usize,
    /// Previous state of auto_upload_on_completion preference (for detecting toggle-off)
    prev_auto_upload_enabled: bool,
}
impl UploadManager {
    pub fn update_user_upload_statistics(&mut self, statistics: UserUploadStatistics) {
        self.recordings.update_user_upload_statistics(statistics);
    }

    pub fn update_user_upload_list(&mut self, uploads: Vec<UserUpload>, limit: u32, offset: u32) {
        self.recordings
            .update_user_upload_list(uploads, limit, offset);
        self.virtual_list.reset();
    }

    pub fn update_local_recordings(&mut self, local_recordings: Vec<LocalRecording>) {
        self.recordings.update_local_recordings(local_recordings);
        self.virtual_list.reset();
    }

    pub fn update_current_upload_progress(&mut self, progress: Option<upload::ProgressData>) {
        self.current_upload_progress = progress;
    }

    pub fn update_last_upload_error(&mut self, last_upload_error: Option<String>) {
        self.last_upload_error = last_upload_error;
    }

    pub fn update_auto_upload_queue_count(&mut self, count: usize) {
        self.auto_upload_queue_count = count;
    }
}

#[derive(Default)]
pub struct Recordings {
    storage: RecordingStorage,

    /// Date filter for uploaded files (start date)
    filter_start_date: Option<chrono::NaiveDate>,
    /// Date filter for uploaded files (end date)
    filter_end_date: Option<chrono::NaiveDate>,

    // Updated on changes
    all: Vec<RecordingIndex>,
    /// All recordings that meet the date filter
    filtered: Vec<RecordingIndex>,
    /// Same as [`Self::filtered`], but excluding local uploaded recordings
    filtered_excluding_local_uploaded: Vec<RecordingIndex>,
    latest_upload_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    invalid_count_filtered: usize,

    statistics: Option<UserUploadStatistics>,
    limit: u32,
    offset: u32,
}
impl Recordings {
    pub fn update_user_upload_statistics(&mut self, statistics: UserUploadStatistics) {
        self.statistics = Some(statistics);
        self.update_calculated_state();
    }

    pub fn update_user_upload_list(&mut self, uploads: Vec<UserUpload>, limit: u32, offset: u32) {
        self.storage.uploaded = uploads;
        self.storage.uploads_available = true;
        self.limit = limit;
        self.offset = offset;
        self.update_calculated_state();
    }

    pub fn update_local_recordings(&mut self, local_recordings: Vec<LocalRecording>) {
        self.storage.local = local_recordings;
        self.storage.local_available = true;
        self.update_calculated_state();
    }

    pub fn iter_filtered(&self) -> impl Iterator<Item = Recording<'_>> {
        self.filtered.iter().filter_map(|ri| self.storage.get(*ri))
    }

    pub fn get(&self, index: RecordingIndex) -> Option<Recording<'_>> {
        self.storage.get(index)
    }

    pub fn get_by_index_filtered_excluding_local_uploaded(
        &self,
        index: usize,
    ) -> Option<Recording<'_>> {
        self.filtered_excluding_local_uploaded
            .get(index)
            .and_then(|ri| self.storage.get(*ri))
    }

    pub fn is_empty_filtered(&self) -> bool {
        self.filtered.is_empty()
    }

    pub fn invalid_count_filtered(&self) -> usize {
        self.invalid_count_filtered
    }

    pub fn len_filtered_excluding_local_uploaded(&self) -> usize {
        self.filtered_excluding_local_uploaded.len()
    }

    pub fn any_available(&self) -> bool {
        self.uploads_available() || self.local_available()
    }

    pub fn uploads_available(&self) -> bool {
        self.storage.uploads_available
    }

    pub fn local_available(&self) -> bool {
        self.storage.local_available
    }

    pub fn earliest_timestamp(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.all
            .last()
            .and_then(|ri| self.storage.get(*ri))
            .map(|r| r.timestamp())
    }

    pub fn latest_timestamp(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.all
            .first()
            .and_then(|ri| self.storage.get(*ri))
            .map(|r| r.timestamp())
    }

    pub fn latest_upload_timestamp(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.latest_upload_timestamp
    }

    pub fn filter_start_date(&self) -> Option<chrono::NaiveDate> {
        self.filter_start_date
    }

    pub fn filter_end_date(&self) -> Option<chrono::NaiveDate> {
        self.filter_end_date
    }

    pub fn set_filter_start_date(&mut self, date: Option<chrono::NaiveDate>) {
        self.set_filter_dates(date, self.filter_end_date);
    }

    pub fn set_filter_end_date(&mut self, date: Option<chrono::NaiveDate>) {
        self.set_filter_dates(self.filter_start_date, date);
    }

    pub fn set_filter_dates(
        &mut self,
        start: Option<chrono::NaiveDate>,
        end: Option<chrono::NaiveDate>,
    ) {
        self.filter_start_date = start;
        self.filter_end_date = end;
        self.update_filtered_state();
    }
}
impl Recordings {
    fn update_calculated_state(&mut self) {
        let user_upload_indices = self
            .storage
            .uploaded
            .iter()
            .enumerate()
            .map(|(i, _)| RecordingIndex::Uploaded(i));
        let local_recording_indices = self
            .storage
            .local
            .iter()
            .enumerate()
            .map(|(i, _)| RecordingIndex::Local(i));

        self.all = user_upload_indices
            .chain(local_recording_indices)
            .collect::<Vec<_>>();
        self.all
            .sort_by_key(|ri| std::cmp::Reverse(self.storage.get(*ri).map(|r| r.timestamp())));
        self.update_filtered_state();
    }

    fn update_filtered_state(&mut self) {
        self.filtered = self
            .all
            .iter()
            .copied()
            .filter(|entry| {
                let Some(date) = self.get(*entry).map(|r| r.timestamp().date_naive()) else {
                    return false;
                };
                let after_start = self.filter_start_date.is_none_or(|start| date >= start);
                let before_end = self.filter_end_date.is_none_or(|end| date <= end);
                after_start && before_end
            })
            .collect::<Vec<_>>();

        self.filtered_excluding_local_uploaded = self
            .filtered
            .iter()
            .copied()
            .filter(|entry| {
                let Some(recording) = self.get(*entry) else {
                    return false;
                };
                !matches!(recording, Recording::Local(LocalRecording::Uploaded { .. }))
            })
            .collect::<Vec<_>>();

        self.latest_upload_timestamp = self
            .iter_filtered()
            .filter(|r| matches!(r, Recording::Uploaded(_)))
            .map(|r| r.timestamp())
            .max();

        self.invalid_count_filtered = self
            .iter_filtered()
            .filter(|r| matches!(r, Recording::Local(LocalRecording::Invalid { .. })))
            .count();
    }
}

#[derive(Default)]
struct RecordingStorage {
    uploaded: Vec<UserUpload>,
    uploads_available: bool,

    local: Vec<LocalRecording>,
    local_available: bool,
}
impl RecordingStorage {
    fn get(&self, index: RecordingIndex) -> Option<Recording<'_>> {
        match index {
            RecordingIndex::Uploaded(index) => self.uploaded.get(index).map(Recording::Uploaded),
            RecordingIndex::Local(index) => self.local.get(index).map(Recording::Local),
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum RecordingIndex {
    Uploaded(usize),
    Local(usize),
}

#[derive(Debug, Copy, Clone)]
pub enum Recording<'a> {
    Uploaded(&'a UserUpload),
    Local(&'a LocalRecording),
}
impl Recording<'_> {
    pub fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
        match self {
            Recording::Uploaded(upload) => upload.created_at,
            Recording::Local(local) => local
                .info()
                .timestamp
                .map(chrono::DateTime::<chrono::Utc>::from)
                .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from(std::time::UNIX_EPOCH)),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn view(
    ui: &mut Ui,
    upload_manager: &mut UploadManager,
    local_preferences: &mut Preferences,
    app_state: &AppState,
    pending_delete_recording: &mut Option<(std::path::PathBuf, String)>,
    is_newer_release_available: bool,
) {
    let recordings = &mut upload_manager.recordings;

    // Compute the unified recordings list.
    let now = chrono::Utc::now();
    let start_date = recordings.earliest_timestamp().unwrap_or(now).date_naive();
    let end_date = recordings.latest_timestamp().unwrap_or(now).date_naive();

    // Display the full path below
    let full_rec_loc = dunce::canonicalize(&local_preferences.recording_location)
        .unwrap_or_else(|_| local_preferences.recording_location.clone());
    ui.horizontal(|ui| {
        ui.label(RichText::new("Upload Manager").size(18.0).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // Open the folder
            if ui.button(RichText::new("Open").size(12.0)).clicked() {
                app_state
                    .async_request_tx
                    .blocking_send(AsyncRequest::OpenDataDump)
                    .ok();
            }

            // Popups to select the new recording location
            if ui.button(RichText::new("Move").size(12.0)).clicked() {
                app_state
                    .async_request_tx
                    .blocking_send(AsyncRequest::PickRecordingFolder {
                        current_location: full_rec_loc.clone(),
                    })
                    .ok();
            }
        });
    });
    // Textedit that displays the recording location (textedit has nicer properties than a label for some reason, like stretching to fill the available width)
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ScrollArea::horizontal()
                .id_salt("recording_location_scroll")
                .show(ui, |ui| {
                    let hover_text = concat!(
                        "This is the folder where your recordings are stored. ",
                        "Use the 'Move' button to change the location."
                    );
                    ui.add_sized(
                        vec2(ui.available_width(), super::SETTINGS_TEXT_HEIGHT),
                        TextEdit::singleline(
                            // egui has custom behaviour for &mut &str, so we need to convert our
                            // Cow<str> to a &str, and then take a mutable reference to it.
                            &mut &*full_rec_loc.to_string_lossy(),
                        ),
                    )
                    .on_hover_text(hover_text);
                });
        });
    });
    ui.horizontal(|ui| {
        let filter_start = recordings.filter_start_date();
        let filter_end = recordings.filter_end_date();

        // From date picker
        ui.label("Viewing recordings from");
        if let Some(new_start) =
            optional_date_picker(ui, filter_start, start_date, "filter_start_date")
        {
            recordings.set_filter_start_date(Some(new_start));
        }

        // To date picker
        ui.label("to");
        if let Some(new_end) = optional_date_picker(ui, filter_end, end_date, "filter_end_date") {
            recordings.set_filter_end_date(Some(new_end));
        }

        // Clear filter button
        if (filter_start.is_some() || filter_end.is_some()) && ui.button("Reset").clicked() {
            recordings.set_filter_dates(None, None);
        }
    });
    ui.separator();
    ui.add_space(10.0);

    ui.horizontal(|ui| {
        upload_stats_view(ui, recordings);
    });
    ui.add_space(8.0);

    // Unified Recordings Section
    CollapsingHeader::new(
        RichText::new(&{
            let invalid_count = recordings.invalid_count_filtered();
            if invalid_count > 0 {
                format!("Upload Tracker ({invalid_count} invalid)")
            } else {
                "Upload Tracker".to_string()
            }
        })
        .size(16.0),
    )
    .default_open(true)
    .show(ui, |ui| {
        recordings_view(
            ui,
            recordings,
            &mut upload_manager.virtual_list,
            app_state,
            pending_delete_recording,
        );
    });

    // Progress Bar
    if let Some(progress) = &upload_manager.current_upload_progress {
        ui.add_space(10.0);

        // Display current file and files remaining
        ui.label(format!(
            "Uploading: {} ({} files remaining)",
            progress.file_progress.current_file, progress.file_progress.files_remaining
        ));

        ui.label(format!(
            "Current upload: {:.2}% ({}/{})",
            progress.percent,
            util::format_bytes(progress.bytes_uploaded),
            util::format_bytes(progress.total_bytes),
        ));
        ui.add(ProgressBar::new(progress.percent as f32 / 100.0));
        ui.label(format!(
            "Speed: {:.1} MB/s • ETA: {}",
            progress.speed_mbps,
            util::format_seconds(progress.eta_seconds as u64),
        ));

        // Show queued recordings count if any
        if upload_manager.auto_upload_queue_count > 0 {
            ui.label(
                RichText::new(format!(
                    "(+{} more recording{} queued for auto-upload)",
                    upload_manager.auto_upload_queue_count,
                    if upload_manager.auto_upload_queue_count == 1 {
                        ""
                    } else {
                        "s"
                    }
                ))
                .size(11.0)
                .color(Color32::from_rgb(150, 200, 255)),
            );
        }
    }

    // Unreliable Connection Setting
    ui.add_space(5.0);
    let offline_mode = app_state
        .offline
        .mode
        .load(std::sync::atomic::Ordering::SeqCst);
    ui.add_enabled_ui(!offline_mode, |ui| {
        ui.horizontal(|ui| {
            ui.add(Checkbox::new(
                &mut local_preferences.unreliable_connection,
                "Optimize for unreliable connections",
            ));
            util::tooltip(
                ui,
                concat!(
                    "Enable this if you have a slow or unstable internet connection. ",
                    "This will use smaller file chunks to improve upload success rates."
                ),
                None,
            );
        });
    });

    // Auto-upload on completion setting
    ui.add_enabled_ui(!offline_mode, |ui| {
        ui.horizontal(|ui| {
            ui.add(Checkbox::new(
                &mut local_preferences.auto_upload_on_completion,
                "Automatically upload recordings when complete",
            ));
            util::tooltip(
                ui,
                concat!(
                    "Automatically start uploading recordings when they finish. ",
                    "If an upload is already in progress, new recordings will be queued."
                ),
                None,
            );

            // Detect when auto-upload is turned off and clear the queue
            if upload_manager.prev_auto_upload_enabled
                && !local_preferences.auto_upload_on_completion
            {
                app_state
                    .async_request_tx
                    .blocking_send(AsyncRequest::ClearAutoUploadQueue)
                    .ok();
            }
            upload_manager.prev_auto_upload_enabled = local_preferences.auto_upload_on_completion;
        });
    });

    // Delete Uploaded Recordings Setting
    ui.add_enabled_ui(!offline_mode, |ui| {
        ui.horizontal(|ui| {
            ui.add(Checkbox::new(
                &mut local_preferences.delete_uploaded_files,
                "Delete recordings after successful upload",
            ));
            util::tooltip(ui, concat!(
                "Automatically delete local recordings after they have been successfully uploaded. ",
                "Invalid uploads, as well as existing uploads, will not be deleted."
            ), None);

            // Check how many uploaded local recordings there are
            let uploaded_local_count = upload_manager
                .recordings
                .iter_filtered()
                .filter(|r| matches!(r, Recording::Local(LocalRecording::Uploaded { .. })))
                .count();

            if uploaded_local_count > 0 {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button(RichText::new(format!("Clear {uploaded_local_count} Uploaded Recordings")).size(11.0))
                        .on_hover_text(concat!(
                            "Delete all local recordings that have been successfully uploaded. ",
                            "This will free up disk space while keeping your uploaded recordings in the cloud."
                        ))
                        .clicked()
                    {
                        app_state
                            .async_request_tx
                            .blocking_send(AsyncRequest::DeleteAllUploadedLocalRecordings)
                            .ok();
                    }
                });
            }
        });
    });

    // Upload Button
    ui.add_space(5.0);
    if upload_manager.current_upload_progress.is_some() {
        // Show Pause button when uploading
        ui.add_enabled_ui(
            !offline_mode
                && !app_state
                    .upload_pause_flag
                    .load(std::sync::atomic::Ordering::Relaxed),
            |ui| {
                let response = ui
                    .add_sized(
                        vec2(ui.available_width(), 32.0),
                        Button::new(
                            RichText::new("Pause Upload")
                                .size(12.0)
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(180, 60, 60)),
                    )
                    .on_hover_text(if offline_mode {
                        "Uploads are disabled in offline mode"
                    } else {
                        concat!(
                            "Pause the uploading process. ",
                            "The next upload will resume where the current one left off."
                        )
                    });
                if response.clicked() {
                    app_state
                        .async_request_tx
                        .blocking_send(AsyncRequest::PauseUpload)
                        .ok();
                }
            },
        );
    } else {
        // Show Upload button when not uploading
        // Count uploadable recordings (unuploaded + paused)
        let uploadable_count = upload_manager
            .recordings
            .iter_filtered()
            .filter(|r| {
                matches!(
                    r,
                    Recording::Local(
                        LocalRecording::Unuploaded { .. } | LocalRecording::Paused { .. }
                    )
                )
            })
            .count();

        let enabled = !offline_mode && !is_newer_release_available && uploadable_count > 0;
        ui.add_enabled_ui(enabled, |ui| {
            let mut response = ui.add_sized(
                vec2(ui.available_width(), 32.0),
                Button::new(
                    RichText::new(format!("Upload {uploadable_count} Recordings")).size(12.0),
                ),
            );

            // Add hover text explaining why uploads are disabled
            if !enabled {
                let hover_text = if offline_mode {
                    "Uploads are disabled in offline mode"
                } else if is_newer_release_available {
                    "Please update to the latest version before uploading."
                } else if uploadable_count == 0 {
                    "No recordings available to upload."
                } else {
                    "Unknown error."
                };
                response = response.on_disabled_hover_text(hover_text);
            }

            if response.clicked() {
                upload_manager.last_upload_error = None;
                app_state
                    .async_request_tx
                    .blocking_send(AsyncRequest::UploadData)
                    .ok();
            }

            if let Some(error) = &upload_manager.last_upload_error {
                ui.label(
                    RichText::new(error)
                        .size(12.0)
                        .color(Color32::from_rgb(255, 0, 0)),
                );
            }
        });
    }
}

fn upload_stats_view(ui: &mut Ui, recordings: &Recordings) {
    let cell_count = 5;
    let available_width = ui.available_width() - (cell_count as f32 * 10.0);
    let cell_width = available_width / cell_count as f32;

    // Calculate pending stats
    let mut unuploaded_duration: f64 = 0.0;
    let mut unuploaded_count: usize = 0;
    let mut unuploaded_size: u64 = 0;

    for recording in recordings.iter_filtered() {
        if let Recording::Local(
            LocalRecording::Unuploaded { info, metadata }
            | LocalRecording::Paused(LocalRecordingPaused { metadata, info, .. }),
        ) = recording
        {
            unuploaded_duration += metadata.as_ref().map(|m| m.duration).unwrap_or(0.0);
            unuploaded_count += 1;
            unuploaded_size += info.folder_size;
        }
    }

    fn create_upload_cell(ui: &mut Ui, icon: &str, title: &str, value: &str) {
        // Icon
        ui.label(RichText::new(icon).size(28.0));
        // Title
        ui.label(RichText::new(title).size(12.0).strong());
        // Value
        ui.add(
            Label::new(
                RichText::new(value)
                    .size(10.0)
                    .color(Color32::from_rgb(128, 128, 128)),
            )
            .wrap_mode(TextWrapMode::Extend),
        );
    }

    // Cell 1: Total Uploaded
    ui.allocate_ui_with_layout(
        vec2(cell_width, ui.available_height()),
        Layout::top_down(Align::Center),
        |ui| {
            let val = if let Some(stats) = &recordings.statistics {
                util::format_seconds(stats.total_video_time.seconds as u64)
            } else if recordings.uploads_available() {
                "0s".to_string()
            } else {
                "Loading...".to_string()
            };
            create_upload_cell(ui, "📊", "Total Uploaded", &val);
        },
    );

    // Cell 2: Files Uploaded
    ui.allocate_ui_with_layout(
        vec2(cell_width, ui.available_height()),
        Layout::top_down(Align::Center),
        |ui| {
            let val = if let Some(stats) = &recordings.statistics {
                stats.total_uploads.to_string()
            } else if recordings.uploads_available() {
                "0".to_string()
            } else {
                "Loading...".to_string()
            };
            create_upload_cell(ui, "📁", "Files Uploaded", &val);
        },
    );

    // Cell 3: Volume Uploaded
    ui.allocate_ui_with_layout(
        vec2(cell_width, ui.available_height()),
        Layout::top_down(Align::Center),
        |ui| {
            let val = if let Some(stats) = &recordings.statistics {
                util::format_bytes(stats.total_data.bytes)
            } else if recordings.uploads_available() {
                "0 B".to_string()
            } else {
                "Loading...".to_string()
            };
            create_upload_cell(ui, "💾", "Volume Uploaded", &val);
        },
    );

    // Cell 4: Pending Uploads
    ui.allocate_ui_with_layout(
        vec2(cell_width, ui.available_height()),
        Layout::top_down(Align::Center),
        |ui| {
            create_upload_cell(
                ui,
                "⏳", // Icon
                "Pending Uploads",
                &format!(
                    "{} / {} files / {}",
                    util::format_seconds(unuploaded_duration as u64),
                    unuploaded_count,
                    util::format_bytes(unuploaded_size)
                ),
            );
        },
    );

    // Cell 5: Last Upload
    ui.allocate_ui_with_layout(
        vec2(cell_width, ui.available_height()),
        Layout::top_down(Align::Center),
        |ui| {
            create_upload_cell(
                ui,
                "🕒", // Icon
                "Last Upload",
                &if recordings.uploads_available() {
                    recordings
                        .latest_upload_timestamp()
                        .map(|dt| dt.with_timezone(&chrono::Local))
                        .map(util::format_datetime)
                        .unwrap_or("Never".to_string())
                } else {
                    "Loading...".to_string()
                },
            );
        },
    );
    ui.add_space(10.0);
}

fn recordings_view(
    ui: &mut Ui,
    recordings: &mut Recordings,
    recordings_virtual_list: &mut egui_virtual_list::VirtualList,
    app_state: &AppState,
    pending_delete_recording: &mut Option<(std::path::PathBuf, String)>,
) {
    const FONTSIZE: f32 = 13.0;
    Frame::new()
        .inner_margin(Margin {
            left: 4,
            right: 12,
            top: 4,
            bottom: 4,
        })
        .show(ui, |ui| {
            let button_height = 28.0;
            let pagination_height = 30.0;
            let height = (ui.available_height() - FOOTER_HEIGHT).max(button_height);

            // Show spinner if still loading
            if !recordings.any_available() {
                ui.vertical_centered(|ui| {
                    ui.add(Spinner::new().size(height));
                });
                return;
            };

            // Delete All Invalid button (only show if there are invalid recordings)
            let any_invalid = recordings.invalid_count_filtered() > 0;
            if any_invalid
                && ui
                    .add_sized(
                        vec2(ui.available_width(), button_height),
                        Button::new(
                            RichText::new("Delete Invalid Recordings")
                                .size(FONTSIZE)
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(180, 60, 60)),
                    )
                    .clicked()
            {
                // Send async request to delete all invalid recordings
                app_state
                    .async_request_tx
                    .blocking_send(AsyncRequest::DeleteAllInvalidRecordings)
                    .ok();
            }

            // Pagination Controls
            if let Some(stats) = &recordings.statistics
                && stats.total_uploads > 0
            {
                ui.horizontal(|ui| {
                    let total_pages =
                        (stats.total_uploads as f32 / recordings.limit as f32).ceil() as u32;
                    let current_page = (recordings.offset / recordings.limit) + 1;

                    ui.add_enabled_ui(recordings.offset > 0, |ui| {
                        if ui.button("Previous").clicked() {
                            let new_offset = recordings.offset.saturating_sub(recordings.limit);
                            app_state
                                .async_request_tx
                                .blocking_send(AsyncRequest::LoadUploadList {
                                    limit: recordings.limit,
                                    offset: new_offset,
                                })
                                .ok();
                        }
                    });

                    ui.label(format!("Page {current_page} of {total_pages}"));

                    ui.add_enabled_ui(current_page < total_pages, |ui| {
                        if ui.button("Next").clicked() {
                            let new_offset = recordings.offset + recordings.limit;
                            app_state
                                .async_request_tx
                                .blocking_send(AsyncRequest::LoadUploadList {
                                    limit: recordings.limit,
                                    offset: new_offset,
                                })
                                .ok();
                        }
                    });

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(format!("Total records: {}", stats.total_uploads));
                    });
                });
                ui.separator();
            }

            ScrollArea::vertical()
                .max_height(
                    height
                        - (if any_invalid { button_height } else { 0.0 })
                        - (if recordings.statistics.is_some() {
                            pagination_height
                        } else {
                            0.0
                        }),
                )
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if recordings.is_empty_filtered() {
                        ui.label("No recordings available in this time period.");
                        return;
                    }

                    recordings_virtual_list.ui_custom_layout(
                        ui,
                        recordings.len_filtered_excluding_local_uploaded(),
                        |ui, index| {
                            let Some(recording) =
                                recordings.get_by_index_filtered_excluding_local_uploaded(index)
                            else {
                                return 0;
                            };

                            render_recording_entry(
                                ui,
                                recording,
                                app_state,
                                FONTSIZE,
                                pending_delete_recording,
                            );
                            1
                        },
                    );
                });
        });
}

fn render_recording_entry(
    ui: &mut Ui,
    entry: Recording,
    app_state: &AppState,
    font_size: f32,
    pending_delete_recording: &mut Option<(std::path::PathBuf, String)>,
) {
    fn datetime<Tz: chrono::TimeZone>(ui: &mut Ui, datetime: chrono::DateTime<Tz>, font_size: f32) {
        let local_time = datetime.with_timezone(&chrono::Local);
        ui.label(RichText::new(local_time.format("%Y-%m-%d %H:%M:%S").to_string()).size(font_size));
    }

    fn filesize(ui: &mut Ui, filesize_mb: f64, font_size: f32) {
        ui.label(RichText::new(format!("{filesize_mb:.2} MB")).size(font_size));
    }

    fn duration(ui: &mut Ui, duration: f64, font_size: f32) {
        ui.label(RichText::new(util::format_seconds(duration as u64)).size(font_size));
    }

    fn local_recording_link(
        ui: &mut Ui,
        info: &LocalRecordingInfo,
        metadata: Option<&Metadata>,
        async_request_tx: &tokio::sync::mpsc::Sender<AsyncRequest>,
        font_size: f32,
        color: Color32,
    ) {
        ui.vertical(|ui| {
            if ui
                .add(
                    Label::new(
                        RichText::new(info.folder_name.as_str())
                            .size(font_size)
                            .color(color)
                            .underline(),
                    )
                    .sense(Sense::click()),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                async_request_tx
                    .blocking_send(AsyncRequest::OpenFolder(info.folder_path.clone()))
                    .ok();
            }

            if let Some(metadata) = metadata {
                ui.label(
                    RichText::new(&metadata.game_exe)
                        .size((font_size * 0.8).floor())
                        .color(color.gamma_multiply(0.8)),
                );
            }
        });
    }

    fn delete_button(ui: &mut Ui, font_size: f32) -> Response {
        ui.add_sized(
            vec2(60.0, 20.0),
            Button::new(
                RichText::new("Delete")
                    .size(font_size)
                    .color(Color32::WHITE),
            )
            .fill(Color32::from_rgb(180, 60, 60)),
        )
    }

    fn frame(ui: &mut Ui, color: Color32, add_contents: impl FnOnce(&mut Ui)) {
        Frame::new()
            .fill(color)
            .inner_margin(4.0)
            .corner_radius(4.0)
            .show(ui, add_contents);
    }

    match entry {
        Recording::Uploaded(upload) => {
            // Successful upload entry
            frame(ui, ui.visuals().faint_bg_color, |ui| {
                ui.horizontal(|ui| {
                    // Success indicator
                    ui.label(
                        RichText::new("✔")
                            .size(font_size)
                            .color(Color32::from_rgb(100, 255, 100)),
                    );

                    // Filename
                    ui.label(RichText::new(upload.id.as_str()).size(font_size));

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        datetime(ui, upload.created_at, font_size);
                        filesize(ui, upload.file_size_mb, font_size);

                        // Duration if available
                        if let Some(dur) = upload.video_duration_seconds {
                            duration(ui, dur, font_size);
                        }
                    });
                });
            });
        }
        Recording::Local(recording) => match recording {
            LocalRecording::Invalid {
                info,
                metadata,
                error_reasons,
                by_server,
            } => {
                // Invalid upload entry
                frame(ui, Color32::from_rgb(80, 40, 40), |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            // Failure indicator
                            ui.label(
                                RichText::new("❌")
                                    .size(font_size)
                                    .color(Color32::from_rgb(255, 100, 100)),
                            );

                            // Folder name (clickable to open folder)
                            local_recording_link(
                                ui,
                                info,
                                metadata.as_deref(),
                                &app_state.async_request_tx,
                                font_size,
                                Color32::from_rgb(255, 200, 200),
                            );

                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                // Timestamp if available
                                if let Some(ts) = info.timestamp {
                                    datetime(
                                        ui,
                                        chrono::DateTime::<chrono::Utc>::from(ts),
                                        font_size,
                                    );
                                }

                                if delete_button(ui, font_size).clicked() {
                                    app_state
                                        .async_request_tx
                                        .blocking_send(AsyncRequest::DeleteRecording(
                                            info.folder_path.clone(),
                                        ))
                                        .ok();
                                }

                                filesize(ui, info.folder_size as f64 / 1024.0 / 1024.0, font_size);

                                if let Some(md) = metadata.as_deref() {
                                    duration(ui, md.duration, font_size);
                                }
                            });
                        });

                        // Collapsible error reasons section
                        CollapsingHeader::new(
                            RichText::new(format!(
                                "⚠ {} error{}{}",
                                error_reasons.len(),
                                if error_reasons.len() == 1 { "" } else { "s" },
                                if *by_server {
                                    " (server invalidated)"
                                } else {
                                    ""
                                }
                            ))
                            .size(font_size - 1.0)
                            .color(Color32::from_rgb(255, 150, 150)),
                        )
                        .id_salt(format!("error_reasons_{}", info.folder_name))
                        .default_open(false)
                        .show(ui, |ui| {
                            Frame::new()
                                .inner_margin(Margin::symmetric(8, 4))
                                .outer_margin(Margin::symmetric(0, 0))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    for reason in error_reasons {
                                        ui.label(
                                            RichText::new(format!("• {reason}"))
                                                .size(font_size - 1.0)
                                                .color(Color32::from_rgb(255, 200, 200)),
                                        );
                                    }
                                });
                        });
                    });
                });
            }
            LocalRecording::Unuploaded { info, metadata } => {
                // Unuploaded entry
                frame(ui, Color32::from_rgb(90, 80, 40), |ui| {
                    ui.horizontal(|ui| {
                        // Pending indicator
                        ui.label(
                            RichText::new("⏳")
                                .size(font_size)
                                .color(Color32::from_rgb(255, 255, 100)),
                        );

                        // Folder name (clickable to open folder)
                        local_recording_link(
                            ui,
                            info,
                            metadata.as_deref(),
                            &app_state.async_request_tx,
                            font_size,
                            Color32::from_rgb(255, 255, 150),
                        );

                        // "Pending upload" label
                        ui.label(
                            RichText::new("(pending upload)")
                                .size(font_size - 1.0)
                                .color(Color32::from_rgb(200, 180, 100))
                                .italics(),
                        );

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            // Timestamp if available
                            if let Some(timestamp) = info.timestamp {
                                datetime(
                                    ui,
                                    chrono::DateTime::<chrono::Utc>::from(timestamp),
                                    font_size,
                                );
                            }

                            if delete_button(ui, font_size).clicked() {
                                // Show confirmation dialog
                                *pending_delete_recording =
                                    Some((info.folder_path.clone(), info.folder_name.clone()));
                            }

                            filesize(ui, info.folder_size as f64 / 1024.0 / 1024.0, font_size);

                            if let Some(md) = metadata.as_deref() {
                                duration(ui, md.duration, font_size);
                            }
                        });
                    });
                });
            }
            LocalRecording::Paused(paused @ LocalRecordingPaused { metadata, info, .. }) => {
                // Paused upload entry
                frame(ui, Color32::from_rgb(70, 60, 90), |ui| {
                    ui.horizontal(|ui| {
                        // Paused indicator
                        ui.label(
                            RichText::new("⏸")
                                .size(font_size)
                                .color(Color32::from_rgb(150, 150, 255)),
                        );

                        // Folder name (clickable to open folder)
                        local_recording_link(
                            ui,
                            info,
                            metadata.as_deref(),
                            &app_state.async_request_tx,
                            font_size,
                            Color32::from_rgb(200, 200, 255),
                        );

                        // "Upload paused" label
                        ui.label(
                            RichText::new("(upload paused)")
                                .size(font_size - 1.0)
                                .color(Color32::from_rgb(170, 170, 220))
                                .italics(),
                        );

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            // Timestamp if available
                            if let Some(timestamp) = info.timestamp {
                                datetime(
                                    ui,
                                    chrono::DateTime::<chrono::Utc>::from(timestamp),
                                    font_size,
                                );
                            }

                            if delete_button(ui, font_size).clicked() {
                                // Show confirmation dialog
                                *pending_delete_recording =
                                    Some((info.folder_path.clone(), info.folder_name.clone()));
                            }

                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing = Vec2::ZERO;
                                filesize(ui, info.folder_size as f64 / 1024.0 / 1024.0, font_size);
                                ui.add(Label::new(
                                    RichText::new(format!(
                                        "{:.2}/",
                                        paused.upload_progress().uploaded_bytes() as f64
                                            / 1024.0
                                            / 1024.0
                                    ))
                                    .size(font_size),
                                ));
                            });

                            if let Some(md) = metadata.as_deref() {
                                duration(ui, md.duration, font_size);
                            }
                        });
                    });
                });
            }
            LocalRecording::Uploaded { .. } => {
                // Uploaded recordings are not shown in the local recordings UI
                // They're already displayed in the successful uploads section as we pull
                // them from the api endpoint.
                unreachable!();
            }
        },
    }
}

/// Wrapper for DatePickerButton that handles Option<NaiveDate>
fn optional_date_picker(
    ui: &mut Ui,
    date: Option<chrono::NaiveDate>,
    default: chrono::NaiveDate,
    id: &str,
) -> Option<chrono::NaiveDate> {
    // Initialize with today's date if None
    let mut temp_date = date.unwrap_or(default);
    let response = ui.add(egui_extras::DatePickerButton::new(&mut temp_date).id_salt(id));
    if response.changed() {
        Some(temp_date)
    } else {
        None
    }
}
