mod input_recorder;
mod local_recording;
mod obs_embedded_recorder;
mod obs_socket_recorder;
mod recorder;
mod recording;

pub use local_recording::{LocalRecording, LocalRecordingInfo};
pub use recorder::{Recorder, get_foregrounded_game};
pub use recording::get_recording_base_resolution;
