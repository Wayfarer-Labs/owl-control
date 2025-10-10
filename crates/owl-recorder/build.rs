use embed_manifest::{embed_manifest, new_manifest};
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_manifest(new_manifest("WayfarerLabs.OwlControl"))
            .expect("unable to embed manifest file");
    }

    // Copy assets to target directory
    copy_assets_to_target();

    println!("cargo:rerun-if-changed=build.rs");
}

fn copy_assets_to_target() {
    // Get the target directory
    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();

    // Source assets directory (relative to project root)
    let source_assets = Path::new("assets");

    // Destination assets directory (in target directory)
    let dest_assets = target_dir.join("assets");

    // Only copy if assets directory exists in source
    if source_assets.exists() && source_assets.is_dir() {
        // Create destination assets directory if it doesn't exist
        if !dest_assets.exists() {
            fs::create_dir_all(&dest_assets).unwrap();
        }

        // Copy all files from source assets to destination
        copy_dir_contents(source_assets, &dest_assets);

        println!("cargo:rerun-if-changed=assets");
    } else {
        println!("cargo:warning=assets directory not found at project root");
    }
}

fn copy_dir_contents(src: &Path, dst: &Path) {
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if path.is_file() {
            // Only copy if file doesn't exist or source is newer
            let should_copy = if dst_path.exists() {
                let src_modified = fs::metadata(&path).unwrap().modified().unwrap();
                let dst_modified = fs::metadata(&dst_path).unwrap().modified().unwrap();
                src_modified > dst_modified
            } else {
                true
            };

            if should_copy {
                fs::copy(&path, &dst_path).unwrap();
                println!("cargo:rerun-if-changed={}", path.display());
            }
        } else if path.is_dir() {
            // Recursively copy subdirectories
            if !dst_path.exists() {
                fs::create_dir_all(&dst_path).unwrap();
            }
            copy_dir_contents(&path, &dst_path);
        }
    }
}
