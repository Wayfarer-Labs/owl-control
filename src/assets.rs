use std::{path::PathBuf, sync::OnceLock};

// if run in dev mode will be relative to project root. When run from .exe will be relative to install directory.
fn get_asset_path(filename: &str) -> PathBuf {
    let cwd = std::env::current_dir().expect("Failed to get executable path");
    cwd.join("assets").join(filename)
}

// lazy static init of the bytes, will be initialized from png assets once at startup, then ref'd as bytes after
// helper fn cuz i don't want to call bytes.get().unwrap() everytime
pub fn get_owl_bytes() -> &'static [u8] {
    static OWL: OnceLock<Vec<u8>> = OnceLock::new();
    OWL.get_or_init(|| {
        std::fs::read(get_asset_path("owl.png")).unwrap_or_else(|e| {
            panic!("Failed to load owl icon: {}", e);
        })
    })
}

pub fn get_logo_default_bytes() -> &'static [u8] {
    static LOGO_DEFAULT_BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    LOGO_DEFAULT_BYTES.get_or_init(|| {
        std::fs::read(get_asset_path("owl-logo.png")).unwrap_or_else(|e| {
            panic!("Failed to load owl-logo icon: {}", e);
        })
    })
}

pub fn get_logo_recording_bytes() -> &'static [u8] {
    static LOGO_RECORDING_BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    LOGO_RECORDING_BYTES.get_or_init(|| {
        std::fs::read(get_asset_path("owl-logo-recording.png")).unwrap_or_else(|e| {
            panic!("Failed to load owl-logo icon: {}", e);
        })
    })
}

pub fn get_honk_0_bytes() -> &'static [u8] {
    static HONK_0_BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    HONK_0_BYTES.get_or_init(|| {
        std::fs::read(get_asset_path("goose_honk0.mp3")).unwrap_or_else(|e| {
            panic!("Failed to load goose_honk0.mp3: {}", e);
        })
    })
}

pub fn get_honk_1_bytes() -> &'static [u8] {
    static HONK_1_BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    HONK_1_BYTES.get_or_init(|| {
        std::fs::read(get_asset_path("goose_honk1.mp3")).unwrap_or_else(|e| {
            panic!("Failed to load goose_honk1.mp3: {}", e);
        })
    })
}
