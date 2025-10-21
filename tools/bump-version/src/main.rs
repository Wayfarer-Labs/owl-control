use clap::Parser;
use color_eyre::eyre::{Context, ContextCompat as _, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Parser)]
#[command(name = "bump-version")]
#[command(about = "Bump semantic version in Cargo.toml and create git tag")]
struct Args {
    /// Version type to bump: major, minor, patch or a literal version string
    #[arg(default_value = "patch")]
    version_type: String,
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let args = Args::parse();

    // Get the root Cargo.toml path
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_dir = manifest_dir
        .parent()
        .context("Could not get parent of CARGO_MANIFEST_DIR")?
        .parent()
        .context("Could not get parent of project directory")?;

    let cargo_toml_path = project_dir.join("Cargo.toml");

    // Read the current Cargo.toml
    let content = fs::read_to_string(&cargo_toml_path)
        .with_context(|| format!("Failed to read {}", cargo_toml_path.display()))?;

    // Extract current version - find the version line
    let mut current_version = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("version") && trimmed.contains('=') && trimmed.contains('"') {
            // Extract version between quotes
            if let Some(start) = trimmed.find('"')
                && let Some(end) = trimmed.rfind('"')
                && start < end
            {
                current_version = Some(trimmed[start + 1..end].to_string());
                break;
            }
        }
    }

    let current_version = match current_version {
        Some(v) => v,
        None => bail!("Could not find version field in Cargo.toml [workspace.package]"),
    };

    // Determine the new version
    let new_version = if args.version_type == "major"
        || args.version_type == "minor"
        || args.version_type == "patch"
    {
        // Parse current version for semantic versioning
        let version_parts: Vec<u32> = current_version
            .split('.')
            .map(|s| s.parse::<u32>())
            .collect::<Result<Vec<_>, _>>()
            .context(format!(
                "Failed to parse version '{}' - must be in X.Y.Z format",
                current_version
            ))?;

        if version_parts.len() != 3 {
            bail!(
                "Version must be in format X.Y.Z, got {} parts",
                version_parts.len()
            );
        }

        let [major, minor, patch] = [version_parts[0], version_parts[1], version_parts[2]];

        // Calculate new version based on bump type
        match args.version_type.as_str() {
            "major" => format!("{}.0.0", major + 1),
            "minor" => format!("{}.{}.0", major, minor + 1),
            "patch" => format!("{}.{}.{}", major, minor, patch + 1),
            _ => unreachable!(),
        }
    } else {
        // Use the version_type as a literal version string
        args.version_type.clone()
    };

    // Update version in content using simple string replacement
    let updated_content = content.replace(
        &format!(r#"version = "{}""#, current_version),
        &format!(r#"version = "{}""#, new_version),
    );

    // Verify the replacement actually happened
    if updated_content == content {
        bail!("Failed to replace version string in Cargo.toml - pattern not found");
    }

    // Write back to Cargo.toml
    fs::write(&cargo_toml_path, &updated_content).context(format!(
        "Failed to write updated {}",
        cargo_toml_path.display()
    ))?;

    println!(
        "Updated version in {} to {}",
        cargo_toml_path.display(),
        new_version
    );
    println!("Version bumped from {} to {}", current_version, new_version);

    // Update lock files
    println!("Updating lock files...");

    run_command("cargo", &["check"], project_dir)?;

    // Commit the version change
    run_command("git", &["add", "Cargo.toml", "Cargo.lock"], project_dir)?;

    run_command(
        "git",
        &["commit", "-m", &format!("Bump version to {}", new_version)],
        project_dir,
    )?;

    // Create a tag
    run_command("git", &["tag", &format!("v{}", new_version)], project_dir)?;

    println!(
        r#"
Version bump complete!

To push the changes and trigger the build workflow, run:
  git push && git push --tags
"#
    );

    Ok(())
}

fn run_command(program: &str, args: &[&str], cwd: &Path) -> color_eyre::Result<()> {
    let status = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .status()
        .context(format!("Failed to execute {program} with args {args:?}"))?;

    if !status.success() {
        bail!(
            "{program} command failed with exit code {:?}",
            status.code()
        );
    }

    Ok(())
}
