use serde::Deserialize;

use crate::app_state::{UiUpdateUnreliable, UiUpdateUnreliableSender};

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
pub struct ProgressData {
    pub bytes_uploaded: u64,
    pub total_bytes: u64,
    pub speed_mbps: f64,
    pub eta_seconds: f64,
    pub percent: f64,
    pub file_progress: FileProgress,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
pub struct FileProgress {
    pub current_file: String,
    pub files_remaining: u64,
}

pub struct ProgressSender {
    tx: UiUpdateUnreliableSender,
    bytes_uploaded: u64,
    last_update_time: std::time::Instant,
    file_size: u64,
    start_time: std::time::Instant,
    file_progress: FileProgress,
}
impl ProgressSender {
    pub fn new(tx: UiUpdateUnreliableSender, file_size: u64, file_progress: FileProgress) -> Self {
        Self {
            tx,
            bytes_uploaded: 0,
            last_update_time: std::time::Instant::now(),
            file_size,
            start_time: std::time::Instant::now(),
            file_progress,
        }
    }

    pub fn increment_bytes_uploaded(&mut self, bytes: u64) {
        self.set_bytes_uploaded(self.bytes_uploaded + bytes);
    }

    pub fn bytes_uploaded(&self) -> u64 {
        self.bytes_uploaded
    }

    pub fn set_bytes_uploaded(&mut self, bytes: u64) {
        self.bytes_uploaded = bytes;
        self.send();
    }

    pub fn send(&mut self) {
        if self.last_update_time.elapsed().as_millis() > 100 {
            self.send_impl();
            self.last_update_time = std::time::Instant::now();
        }
    }

    fn send_impl(&self) {
        let bps = self.bytes_uploaded as f64 / self.start_time.elapsed().as_secs_f64();
        self.tx
            .send(UiUpdateUnreliable::UpdateUploadProgress(Some(
                ProgressData {
                    bytes_uploaded: self.bytes_uploaded,
                    total_bytes: self.file_size,
                    speed_mbps: bps / (1024.0 * 1024.0),
                    eta_seconds: if bps > 0.0 {
                        (self.file_size - self.bytes_uploaded) as f64 / bps
                    } else {
                        0.0
                    },
                    percent: if self.file_size > 0 {
                        ((self.bytes_uploaded as f64 / self.file_size as f64) * 100.0).min(100.0)
                    } else {
                        0.0
                    },
                    file_progress: self.file_progress.clone(),
                },
            )))
            .ok();
    }
}
