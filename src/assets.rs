use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// if run in dev mode will be relative to project root. When run from .exe will be relative to install directory.
fn get_asset_path(filename: &str) -> PathBuf {
    let cwd = std::env::current_dir().expect("Failed to get executable path");
    cwd.join("assets").join(filename)
}

struct AssetData(OnceLock<Vec<u8>>);
impl AssetData {
    const fn new() -> Self {
        Self(OnceLock::new())
    }

    pub fn get(&'static self, filename: &'static str) -> &'static [u8] {
        self.0.get_or_init(move || {
            std::fs::read(get_asset_path(filename)).unwrap_or_else(|e| {
                panic!("Failed to load {filename}: {e}");
            })
        })
    }
}

// lazy static init of the bytes, will be initialized from png assets once at startup, then ref'd as bytes after
// helper fn cuz i don't want to call bytes.get().unwrap() everytime
pub fn get_owl_bytes() -> &'static [u8] {
    static DATA: AssetData = AssetData::new();
    DATA.get("owl.png")
}

pub fn get_logo_default_bytes() -> &'static [u8] {
    static DATA: AssetData = AssetData::new();
    DATA.get("owl-logo.png")
}

pub fn get_logo_recording_bytes() -> &'static [u8] {
    static DATA: AssetData = AssetData::new();
    DATA.get("owl-logo-recording.png")
}

/// Loads an arbitrary audio cue from the assets/cues/ directory, storing it all as lazily init
/// static refs in a hashmap that will last for the entire program lifetime (hence the Box::leak)
/// Falls back to default_start.mp3 if the requested cue fails to load
pub fn get_cue(filename: &str) -> &'static [u8] {
    static CUES: OnceLock<Mutex<HashMap<String, &'static [u8]>>> = OnceLock::new();

    let cues = CUES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = cues.lock().unwrap();

    *map.entry(filename.to_string()).or_insert_with(|| {
        let path = format!("cues/{filename}");
        let data = std::fs::read(get_asset_path(&path)).unwrap_or_else(|e| {
            // Try to fallback to default_start.mp3
            if filename != "default_start.mp3" {
                tracing::warn!("Failed to load {path}: {e}, falling back to default_start.mp3");
                let default_path = "cues/default_start.mp3";
                std::fs::read(get_asset_path(default_path)).unwrap_or_else(|e| {
                    panic!("Failed to load fallback {default_path}: {e}");
                })
            } else {
                panic!("Failed to load {path}: {e}");
            }
        });
        Box::leak(data.into_boxed_slice())
    })
}

/// Scans the cues folder and returns a list of available MP3 files
pub fn get_available_cues() -> Vec<String> {
    let cues_path = get_asset_path("cues");

    let mut cues = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&cues_path) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_file() {
                    if let Some(filename) = entry.file_name().to_str() {
                        if filename.ends_with(".mp3") {
                            cues.push(filename.to_string());
                        }
                    }
                }
            }
        }
    }

    // Sort alphabetically for consistent ordering
    cues.sort();
    cues
}

/// Loads icon data from bytes and returns the rgba data and dimensions
pub fn load_icon_data_from_bytes(bytes: &[u8]) -> (Vec<u8>, (u32, u32)) {
    let image = image::load_from_memory(bytes)
        .expect("Failed to load embedded icon")
        .into_rgba8();
    let dimensions = image.dimensions();
    let rgba = image.into_raw();
    (rgba, dimensions)
}
