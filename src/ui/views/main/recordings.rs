use crate::{
    api::UserUpload,
    app_state::{AppState, AsyncRequest},
    output_types::Metadata,
    record::{LocalRecording, LocalRecordingInfo},
    ui::util,
};

#[derive(Default)]
pub struct Recordings {
    storage: RecordingStorage,

    /// Date filter for uploaded files (start date)
    filter_start_date: Option<chrono::NaiveDate>,
    /// Date filter for uploaded files (end date)
    filter_end_date: Option<chrono::NaiveDate>,

    // Updated on changes
    all: Vec<RecordingIndex>,
    filtered: Vec<RecordingIndex>,
    latest_upload_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    invalid_count_filtered: usize,
}
impl Recordings {
    pub fn update_user_uploads(&mut self, user_uploads: Vec<UserUpload>) {
        self.storage.uploaded = user_uploads;
        self.storage.uploads_available = true;
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

    pub fn get_by_index_filtered(&self, index: usize) -> Option<Recording<'_>> {
        self.filtered
            .get(index)
            .and_then(|ri| self.storage.get(*ri))
    }

    pub fn is_empty_filtered(&self) -> bool {
        self.filtered.is_empty()
    }

    pub fn len_filtered(&self) -> usize {
        self.filtered.len()
    }

    pub fn invalid_count_filtered(&self) -> usize {
        self.invalid_count_filtered
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

pub fn upload_stats_view(ui: &mut egui::Ui, recordings: &Recordings) {
    let cell_count = 5;
    let available_width = ui.available_width() - (cell_count as f32 * 10.0);
    let cell_width = available_width / cell_count as f32;

    // Calculate stats for each of our categories. The endpoint that we use to get
    // the uploaded recordings tells us this, but over the entire range: we'd like to
    // cover just the user-filtered range.
    let mut total_duration: f64 = 0.0;
    let mut total_count: usize = 0;
    let mut total_size: u64 = 0;
    let mut last_upload: Option<chrono::DateTime<chrono::Utc>> = None;

    let mut unuploaded_duration: f64 = 0.0;
    let mut unuploaded_count: usize = 0;
    let mut unuploaded_size: u64 = 0;

    for recording in recordings.iter_filtered() {
        match recording {
            Recording::Uploaded(recording) => {
                total_duration += recording.video_duration_seconds.unwrap_or(0.0);
                total_count += 1;
                total_size += recording.file_size_bytes;
                if last_upload.is_none() || recording.created_at > last_upload.unwrap() {
                    last_upload = Some(recording.created_at);
                }
            }
            Recording::Local(LocalRecording::Unuploaded { info, metadata }) => {
                unuploaded_duration += metadata.as_ref().map(|m| m.duration).unwrap_or(0.0);
                unuploaded_count += 1;
                unuploaded_size += info.folder_size;
            }
            Recording::Local(LocalRecording::Invalid { .. } | LocalRecording::Uploaded { .. }) => {
                // We don't count these in our stats
            }
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
        ui.add(
            egui::Label::new(
                egui::RichText::new(value)
                    .size(10.0)
                    .color(egui::Color32::from_rgb(128, 128, 128)),
            )
            .wrap_mode(egui::TextWrapMode::Extend),
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
                &if recordings.uploads_available() {
                    util::format_seconds(total_duration as u64)
                } else {
                    "Loading...".to_string()
                },
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
                &if recordings.uploads_available() {
                    total_count.to_string()
                } else {
                    "Loading...".to_string()
                },
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
                &if recordings.uploads_available() {
                    util::format_bytes(total_size)
                } else {
                    "Loading...".to_string()
                },
            );
        },
    );

    // Cell 4: Pending Uploads
    ui.allocate_ui_with_layout(
        egui::vec2(cell_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Center),
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
        egui::vec2(cell_width, ui.available_height()),
        egui::Layout::top_down(egui::Align::Center),
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

pub fn recordings_view(
    ui: &mut egui::Ui,
    recordings: &mut Recordings,
    recordings_virtual_list: &mut egui_virtual_list::VirtualList,
    app_state: &AppState,
    pending_delete_recording: &mut Option<(std::path::PathBuf, String)>,
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
            let button_height = 28.0;
            let height = 120.0;

            // Show spinner if still loading
            if !recordings.any_available() {
                ui.vertical_centered(|ui| {
                    ui.add(egui::widgets::Spinner::new().size(height));
                });
                return;
            };

            // Delete All Invalid button (only show if there are invalid recordings)
            let any_invalid = recordings.invalid_count_filtered() > 0;
            if any_invalid
                && ui
                    .add_sized(
                        egui::vec2(ui.available_width(), button_height),
                        egui::Button::new(
                            egui::RichText::new("Delete Invalid Recordings")
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
                    .blocking_send(AsyncRequest::DeleteAllInvalidRecordings)
                    .ok();
            }

            egui::ScrollArea::vertical()
                .max_height(height - if any_invalid { button_height } else { 0.0 })
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if recordings.is_empty_filtered() {
                        ui.label("No recordings available in this time period.");
                        return;
                    }

                    recordings_virtual_list.ui_custom_layout(
                        ui,
                        recordings.len_filtered(),
                        |ui, index| {
                            let Some(recording) = recordings.get_by_index_filtered(index) else {
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
    ui: &mut egui::Ui,
    entry: Recording,
    app_state: &AppState,
    font_size: f32,
    pending_delete_recording: &mut Option<(std::path::PathBuf, String)>,
) {
    fn datetime<Tz: chrono::TimeZone>(
        ui: &mut egui::Ui,
        datetime: chrono::DateTime<Tz>,
        font_size: f32,
    ) {
        let local_time = datetime.with_timezone(&chrono::Local);
        ui.label(
            egui::RichText::new(local_time.format("%Y-%m-%d %H:%M:%S").to_string()).size(font_size),
        );
    }

    fn filesize(ui: &mut egui::Ui, filesize_mb: f64, font_size: f32) {
        ui.label(egui::RichText::new(format!("{filesize_mb:.2} MB")).size(font_size));
    }

    fn duration(ui: &mut egui::Ui, duration: f64, font_size: f32) {
        ui.label(egui::RichText::new(util::format_seconds(duration as u64)).size(font_size));
    }

    fn local_recording_link(
        ui: &mut egui::Ui,
        info: &LocalRecordingInfo,
        metadata: Option<&Metadata>,
        async_request_tx: &tokio::sync::mpsc::Sender<AsyncRequest>,
        font_size: f32,
        color: egui::Color32,
    ) {
        ui.vertical(|ui| {
            if ui
                .add(
                    egui::Label::new(
                        egui::RichText::new(info.folder_name.as_str())
                            .size(font_size)
                            .color(color)
                            .underline(),
                    )
                    .sense(egui::Sense::click()),
                )
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                async_request_tx
                    .blocking_send(AsyncRequest::OpenFolder(info.folder_path.clone()))
                    .ok();
            }

            if let Some(metadata) = metadata {
                ui.label(
                    egui::RichText::new(&metadata.game_exe)
                        .size((font_size * 0.8).floor())
                        .color(color.gamma_multiply(0.8)),
                );
            }
        });
    }

    fn delete_button(ui: &mut egui::Ui, font_size: f32) -> egui::Response {
        ui.add_sized(
            egui::vec2(60.0, 20.0),
            egui::Button::new(
                egui::RichText::new("Delete")
                    .size(font_size)
                    .color(egui::Color32::WHITE),
            )
            .fill(egui::Color32::from_rgb(180, 60, 60)),
        )
    }

    fn frame(ui: &mut egui::Ui, color: egui::Color32, add_contents: impl FnOnce(&mut egui::Ui)) {
        egui::Frame::new()
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
                        egui::RichText::new("✔")
                            .size(font_size)
                            .color(egui::Color32::from_rgb(100, 255, 100)),
                    );

                    // Filename
                    ui.label(egui::RichText::new(upload.id.as_str()).size(font_size));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
            } => {
                // Invalid upload entry
                frame(ui, egui::Color32::from_rgb(80, 40, 40), |ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            // Failure indicator
                            ui.label(
                                egui::RichText::new("❌")
                                    .size(font_size)
                                    .color(egui::Color32::from_rgb(255, 100, 100)),
                            );

                            // Folder name (clickable to open folder)
                            local_recording_link(
                                ui,
                                info,
                                metadata.as_deref(),
                                &app_state.async_request_tx,
                                font_size,
                                egui::Color32::from_rgb(255, 200, 200),
                            );

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
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

                                    filesize(
                                        ui,
                                        info.folder_size as f64 / 1024.0 / 1024.0,
                                        font_size,
                                    );

                                    if let Some(md) = metadata.as_deref() {
                                        duration(ui, md.duration, font_size);
                                    }
                                },
                            );
                        });

                        // Collapsible error reasons section
                        egui::CollapsingHeader::new(
                            egui::RichText::new(format!(
                                "⚠ {} error{}",
                                error_reasons.len(),
                                if error_reasons.len() == 1 { "" } else { "s" }
                            ))
                            .size(font_size - 1.0)
                            .color(egui::Color32::from_rgb(255, 150, 150)),
                        )
                        .id_salt(format!("error_reasons_{}", info.folder_name))
                        .default_open(false)
                        .show(ui, |ui| {
                            egui::Frame::new()
                                .inner_margin(egui::Margin::symmetric(8, 4))
                                .outer_margin(egui::Margin::symmetric(0, 0))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    for reason in error_reasons {
                                        ui.label(
                                            egui::RichText::new(format!("• {}", reason))
                                                .size(font_size - 1.0)
                                                .color(egui::Color32::from_rgb(255, 200, 200)),
                                        );
                                    }
                                });
                        });
                    });
                });
            }
            LocalRecording::Unuploaded { info, metadata } => {
                // Unuploaded entry
                frame(ui, egui::Color32::from_rgb(90, 80, 40), |ui| {
                    ui.horizontal(|ui| {
                        // Pending indicator
                        ui.label(
                            egui::RichText::new("⏳")
                                .size(font_size)
                                .color(egui::Color32::from_rgb(255, 255, 100)),
                        );

                        // Folder name (clickable to open folder)
                        local_recording_link(
                            ui,
                            info,
                            metadata.as_deref(),
                            &app_state.async_request_tx,
                            font_size,
                            egui::Color32::from_rgb(255, 255, 150),
                        );

                        // "Pending upload" label
                        ui.label(
                            egui::RichText::new("(pending upload)")
                                .size(font_size - 1.0)
                                .color(egui::Color32::from_rgb(200, 180, 100))
                                .italics(),
                        );

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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
            LocalRecording::Uploaded { .. } => {
                // Uploaded recordings are not shown in the local recordings UI
                // They're already displayed in the successful uploads section as we pull
                // them from the api endpoint.
            }
        },
    }
}
