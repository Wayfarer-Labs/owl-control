use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use tokio::sync::mpsc;

use crate::{
    api::ApiClient,
    app_state::{self, AppState, AsyncRequest, UiUpdate, UiUpdateUnreliable},
    record::LocalRecording,
    upload::upload_tar::UploadTarOutput,
};

mod progress_sender;
pub use progress_sender::{FileProgress, ProgressData, ProgressSender};

mod upload_folder;

mod create_tar;

mod upload_tar;

pub async fn start(
    app_state: Arc<AppState>,
    api_client: Arc<ApiClient>,
    recording_location: PathBuf,
) {
    let reliable_tx = app_state.ui_update_tx.clone();
    let unreliable_tx = app_state.ui_update_unreliable_tx.clone();
    let pause_flag = app_state.upload_pause_flag.clone();

    // Reset cancel flag at start of upload
    pause_flag.store(false, std::sync::atomic::Ordering::SeqCst);

    let (api_token, unreliable_connection, delete_uploaded) = {
        let config = app_state.config.read().unwrap();
        (
            config.credentials.api_key.clone(),
            config.preferences.unreliable_connection,
            config.preferences.delete_uploaded_files,
        )
    };

    app_state
        .ui_update_unreliable_tx
        .send(UiUpdateUnreliable::UpdateUploadProgress(Some(
            ProgressData::default(),
        )))
        .ok();

    if let Err(e) = run(
        &recording_location,
        api_client,
        api_token,
        unreliable_connection,
        delete_uploaded,
        reliable_tx.clone(),
        unreliable_tx.clone(),
        app_state.async_request_tx.clone(),
        pause_flag,
    )
    .await
    {
        tracing::error!(e=?e, "Error uploading recordings");
    }

    for req in [
        AsyncRequest::LoadUploadStats,
        AsyncRequest::LoadLocalRecordings,
    ] {
        app_state.async_request_tx.send(req).await.ok();
    }
    unreliable_tx
        .send(UiUpdateUnreliable::UpdateUploadProgress(None))
        .ok();
}

/// Separate function to allow for fallibility
#[allow(clippy::too_many_arguments)]
async fn run(
    recording_location: &Path,
    api_client: Arc<ApiClient>,
    api_token: String,
    unreliable_connection: bool,
    delete_uploaded: bool,
    reliable_tx: app_state::UiUpdateSender,
    unreliable_tx: app_state::UiUpdateUnreliableSender,
    async_req_tx: mpsc::Sender<AsyncRequest>,
    pause_flag: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), upload_folder::UploadFolderError> {
    // Scan all local recordings and filter to only Paused and Unuploaded
    let recordings_to_upload: Vec<_> = LocalRecording::scan_directory(recording_location)
        .into_iter()
        .filter(|rec| {
            matches!(
                rec,
                LocalRecording::Paused(_) | LocalRecording::Unuploaded { .. }
            )
        })
        .collect();

    let total_files_to_upload = recordings_to_upload.len() as u64;
    let mut files_uploaded = 0u64;

    let mut last_upload_time = std::time::Instant::now();
    let reload_every_n_files = 5;
    let reload_if_at_least_has_passed = std::time::Duration::from_secs(2 * 60);
    for recording in recordings_to_upload {
        // Check if upload has been paused
        if pause_flag.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }

        let info = recording.info().clone();
        let path = info.folder_path.clone();

        let file_progress = FileProgress {
            current_file: info.folder_name.clone(),
            files_remaining: total_files_to_upload.saturating_sub(files_uploaded),
        };

        let result = upload_folder::upload_folder(
            recording,
            api_client.clone(),
            &api_token,
            unreliable_connection,
            unreliable_tx.clone(),
            pause_flag.clone(),
            file_progress,
        )
        .await;

        let recording_to_delete = match result {
            Ok(UploadTarOutput::Success(recording)) => Some(recording),
            Ok(UploadTarOutput::ServerInvalid(_recording)) => {
                // We intentionally choose not to delete server invalid recordings, so that the user can learn why it was invalidated
                None
            }
            Ok(UploadTarOutput::Paused(_recording)) => {
                // We intentionally choose not to delete paused recordings, as they are still valid and can be resumed
                None
            }
            Err(e) => {
                tracing::error!("Error uploading folder {}: {:?}", path.display(), e);
                reliable_tx.send(UiUpdate::UploadFailed(e.to_string())).ok();
                continue;
            }
        };

        files_uploaded += 1;

        // delete the uploaded recording directory if the preference is enabled
        if delete_uploaded && let Some(uploaded_recording) = recording_to_delete {
            let path = path.display();
            match uploaded_recording.delete(&api_client, &api_token).await {
                Ok(_) => {
                    tracing::info!("Deleted uploaded directory: {path}");
                }
                Err(e) => {
                    tracing::error!("Failed to delete uploaded directory {path}: {e:?}");
                }
            }
        }

        let should_reload = if files_uploaded.is_multiple_of(reload_every_n_files) {
            tracing::info!(
                "{} files uploaded, reloading upload stats and local recordings",
                files_uploaded
            );
            true
        } else if last_upload_time.elapsed() > reload_if_at_least_has_passed {
            tracing::info!(
                "{} seconds since last upload, reloading upload stats and local recordings",
                last_upload_time.elapsed().as_secs()
            );
            true
        } else {
            false
        };

        if should_reload {
            for req in [
                AsyncRequest::LoadUploadStats,
                AsyncRequest::LoadLocalRecordings,
            ] {
                async_req_tx.send(req).await.ok();
            }
        }
        last_upload_time = std::time::Instant::now();
    }

    Ok(())
}
