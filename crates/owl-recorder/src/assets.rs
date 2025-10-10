use std::{path::PathBuf, sync::OnceLock};

// lazy static init of the bytes, will be initialized from png assets once at startup, then ref'd as bytes after
static OWL: OnceLock<Vec<u8>> = OnceLock::new();
static LOGO_DEFAULT_BYTES: OnceLock<Vec<u8>> = OnceLock::new();
static LOGO_RECORDING_BYTES: OnceLock<Vec<u8>> = OnceLock::new();
static HONK_0_BYTES: OnceLock<Vec<u8>> = OnceLock::new();
static HONK_1_BYTES: OnceLock<Vec<u8>> = OnceLock::new();

fn get_asset_path(filename: &str) -> PathBuf {
    let exe_path = std::env::current_exe().expect("Failed to get executable path");
    let exe_dir = exe_path
        .parent()
        .expect("Failed to get executable directory");
    exe_dir.join("assets").join(filename)
}

// helper fn cuz i don't want to call bytes.get().unwrap() everytime
pub fn get_owl_bytes() -> &'static [u8] {
    OWL.get_or_init(|| {
        std::fs::read(get_asset_path("owl.png")).unwrap_or_else(|e| {
            panic!("Failed to load owl icon: {}", e);
        })
    })
}

pub fn get_logo_default_bytes() -> &'static [u8] {
    LOGO_DEFAULT_BYTES.get_or_init(|| {
        std::fs::read(get_asset_path("owl-logo.png")).unwrap_or_else(|e| {
            panic!("Failed to load owl-logo icon: {}", e);
        })
    })
}

pub fn get_logo_recording_bytes() -> &'static [u8] {
    LOGO_RECORDING_BYTES.get_or_init(|| {
        std::fs::read(get_asset_path("owl-logo-recording.png")).unwrap_or_else(|e| {
            panic!("Failed to load owl-logo icon: {}", e);
        })
    })
}

pub fn get_honk_0_bytes() -> &'static [u8] {
    HONK_0_BYTES.get_or_init(|| {
        std::fs::read(get_asset_path("goose_honk0.mp3")).unwrap_or_else(|e| {
            panic!("Failed to load goose_honk0.mp3: {}", e);
        })
    })
}

pub fn get_honk_1_bytes() -> &'static [u8] {
    HONK_1_BYTES.get_or_init(|| {
        std::fs::read(get_asset_path("goose_honk1.mp3")).unwrap_or_else(|e| {
            panic!("Failed to load goose_honk1.mp3: {}", e);
        })
    })
}
