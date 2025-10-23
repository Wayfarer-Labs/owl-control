use std::{path::Path, time::Duration};

use constants::{MAX_FOOTAGE, MIN_FOOTAGE};

use crate::output_types::Metadata;

pub fn validate(video_path: &Path, metadata: &Metadata) -> Vec<String> {
    let mut invalid_reasons = vec![];

    let duration = Duration::from_secs_f64(metadata.duration);
    if duration < MIN_FOOTAGE {
        invalid_reasons.push(format!("Video length {} too short.", metadata.duration));
    }
    if duration > MAX_FOOTAGE + Duration::from_secs(10) {
        invalid_reasons.push(format!("Video length {} too long.", metadata.duration));
    }

    let size_bytes = match std::fs::metadata(video_path).map(|m| m.len()) {
        Ok(size_bytes) => size_bytes,
        Err(e) => {
            invalid_reasons.push(format!("Video size unknown: {e}"));
            return invalid_reasons;
        }
    };

    let size_mbytes = size_bytes as f64 / (1024.0 * 1024.0);
    let size_mbits = size_mbytes * 8.0;

    let bitrate = 2.0;
    let expected_mbits = bitrate * metadata.duration;

    if size_mbits < 0.25 * expected_mbits {
        invalid_reasons.push(format!(
            "Video size {size_mbits:.2}Mb too small compared to expected {expected_mbits:.2}Mb",
        ));
    }

    invalid_reasons
}
