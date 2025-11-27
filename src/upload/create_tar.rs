use std::path::{Path, PathBuf};

use tokio::task::JoinError;

use crate::validation::ValidationResult;

#[derive(Debug)]
pub enum CreateTarError {
    Join(JoinError),
    InvalidFilename(PathBuf),
    Io(std::io::Error),
}
impl std::fmt::Display for CreateTarError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CreateTarError::Join(e) => write!(f, "Join error: {e}"),
            CreateTarError::InvalidFilename(path) => write!(f, "Invalid filename: {path:?}"),
            CreateTarError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}
impl std::error::Error for CreateTarError {}
impl From<JoinError> for CreateTarError {
    fn from(e: JoinError) -> Self {
        CreateTarError::Join(e)
    }
}
impl From<std::io::Error> for CreateTarError {
    fn from(e: std::io::Error) -> Self {
        CreateTarError::Io(e)
    }
}
pub async fn create_tar_file(
    recording_path: &Path,
    validation: &ValidationResult,
) -> Result<PathBuf, CreateTarError> {
    tokio::task::spawn_blocking({
        let recording_path = recording_path.to_path_buf();
        let validation = validation.clone();
        move || {
            // Create tar file inside the recording folder
            let tar_path = recording_path.join(format!(
                "{}.tar",
                &uuid::Uuid::new_v4().simple().to_string()[0..16]
            ));
            let mut tar = tar::Builder::new(std::fs::File::create(&tar_path)?);
            for path in [
                &validation.mp4_path,
                &validation.csv_path,
                &validation.meta_path,
            ] {
                tar.append_file(
                    path.file_name()
                        .ok_or_else(|| CreateTarError::InvalidFilename(path.to_owned()))?,
                    &mut std::fs::File::open(path)?,
                )?;
            }

            Ok(tar_path)
        }
    })
    .await?
}
