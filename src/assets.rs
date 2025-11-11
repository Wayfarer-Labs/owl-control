use std::{path::PathBuf, sync::OnceLock};

// if run in dev mode will be relative to project root. When run from .exe will be relative to install directory.
fn get_asset_path(filename: &str) -> PathBuf {
    let cwd = std::env::current_dir().expect("Failed to get executable path");
    cwd.join("assets").join(filename)
}

struct IconData(OnceLock<Vec<u8>>);
impl IconData {
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

    pub fn get_cue(&'static self, filename: &'static str) -> &'static [u8] {
        self.0.get_or_init(move || {
            let path = format!("cues/{filename}");
            std::fs::read(get_asset_path(&path)).unwrap_or_else(|e| {
                panic!("Failed to load {path}: {e}");
            })
        })
    }
}

// lazy static init of the bytes, will be initialized from png assets once at startup, then ref'd as bytes after
// helper fn cuz i don't want to call bytes.get().unwrap() everytime
pub fn get_owl_bytes() -> &'static [u8] {
    static DATA: IconData = IconData::new();
    DATA.get("owl.png")
}

pub fn get_logo_default_bytes() -> &'static [u8] {
    static DATA: IconData = IconData::new();
    DATA.get("owl-logo.png")
}

pub fn get_logo_recording_bytes() -> &'static [u8] {
    static DATA: IconData = IconData::new();
    DATA.get("owl-logo-recording.png")
}

pub fn get_honk_0_bytes() -> &'static [u8] {
    static DATA: IconData = IconData::new();
    DATA.get_cue("goose_honk0.mp3")
}

pub fn get_honk_1_bytes() -> &'static [u8] {
    static DATA: IconData = IconData::new();
    DATA.get_cue("goose_honk1.mp3")
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
