// build.rs
use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3) // Go up from OUT_DIR to target/{profile}/
        .unwrap();

    println!("cargo:rerun-if-changed={:?}", target_dir.to_str());
    // Mainly for packaging the obs dummy dll for bootstrapper
    let dll_paths = vec!["libs/obs.dll"];

    // Copy DLLs to target directory
    for dll_path in dll_paths {
        let src = Path::new(dll_path);
        if src.exists() {
            let filename = src.file_name().unwrap();
            let dest = target_dir.join(filename);

            // originally we only track rerun if changes occur to lib/ but that ended up being too inconsistent, so instead
            // we just rerun the copy process every time build is called if the .dll doesn't already exist in target directory
            if !dest.exists() {
                if let Err(e) = fs::copy(&src, &dest) {
                    eprintln!("Warning: Failed to copy {}: {}", dll_path, e);
                } else {
                    println!("cargo:rerun-if-changed={}", dll_path);
                    println!("Copied {} to {}", dll_path, dest.display());
                }
            }
        } else {
            eprintln!("Warning: DLL not found: {}", dll_path);
        }
    }
}
