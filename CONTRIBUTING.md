# Contributing to OWL Control

Thanks for your interest in contributing to OWL Control! 🦉

## Building from Source

**Note for Linux developers**: See [LINUX_DEV_SETUP.md](./LINUX_DEV_SETUP.md) for instructions on setting up a Windows VM for development and testing.

Using PowerShell or Command Prompt:

1. Install [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html).

2. Clone the repo:

```powershell
git clone https://github.com/Wayfarer-Labs/owl-control.git
cd owl-control
```

3. Build the application to create the target directory (this only needs to be done once; `cargo run` will rebuild afterwards):

```powershell
cargo build
```

4. Install `cargo-obs-build`:

```powershell
cargo install cargo-obs-build
```

5. Install the OBS binaries (this only needs to be done when the OBS version is updated):

```powershell
cargo obs-build build --out-dir target\x86_64-pc-windows-msvc\debug
```

6. Run OWL Control with:

```powershell
cargo run
```

To build a production-ready release with an installer:

- Install [NSIS](https://sourceforge.net/projects/nsis/) to default location
- Run the build script

```powershell
build-resources\scripts\build.ps1
```

## Tools

### owlc-test-app

A lightweight wgpu-based test application for testing OWL Control's recording functionality.

**Usage:**

```powershell
cargo run -p test-app
```

Press `Escape` to close the window.

## Code Quality

This project uses automated code formatting tools to maintain consistent code style:

```bash
cargo fmt
```

and automated linting tools to ensure code quality:

```bash
cargo clippy
```

## Updating the Games List

Before updating the games list, please check with the data team to ensure that the changes are valid. In this phase, we are primarily pruning games and not adding new ones.

Please update `crates/constants/src/supported_games.json` and run `cargo run --p update-games --release` to update `GAMES.md`. To find the executable names for each game, you can use:

- SteamDB: You can look at the depots for a game and find the `exe` files. This requires some discernment, and is hard to automate. It is the most reliable approach, however.
- Rely on Discord's hard work, and use their list instead: <https://discord.com/api/v10/applications/detectable>

## Data Structure Changes

### Modifying Output Formats

If you need to change the structure of recorded data outputs (especially `inputs.csv` or other data files), **please check with the data team first** to ensure they can properly ingest the new format.

### Backwards Compatibility

When making changes to data structures:

- **Maintain backwards compatibility** - the code must be able to load both old and new CSV formats
- This ensures that users can still upload recordings made with older versions of OWL Control
- Test your changes with both old and new data files to verify upload functionality works correctly

### Event Types

When modifying event types in the codebase:

- **Never remove event types** - even if they're no longer used
- Instead, mark deprecated event types with appropriate deprecation markers
- This preserves backwards compatibility with old recordings that may still contain those event types

## Releasing a New Version

### Version Bumping

We use an automated tool to bump versions. Run one of the following commands:

```bash
# For semantic versioning
cargo run -p bump-version -- major    # 1.0.0 -> 2.0.0
cargo run -p bump-version -- minor    # 1.0.0 -> 1.1.0
cargo run -p bump-version -- patch    # 1.0.0 -> 1.0.1

# Or specify a custom version
cargo run -p bump-version -- 1.2.3
```

This command will:

- Update version numbers in relevant files
- Create a git commit
- Create a git tag

### Automated Releases

Once you push the tag to the repository, GitHub Actions will automatically build and publish a release.

### Pre-Release Checklist

Before releasing a new version, ensure you've completed the following:

- [ ] **Test recording** with your supported encoders (NVENC, AMD, etc.) in multiple games
- [ ] **Test uploading** to verify the upload functionality works correctly
- [ ] **Update documentation** if there are any user-facing changes or new features
- [ ] **For major changes**: Create and test a release candidate first

### Release Candidates

For significant changes, create a release candidate and test it with the community before the final release:

```bash
# Example: Creating a release candidate for version 1.1.1
cargo run -p bump-version -- 1.1.1-rc1
```

After creating a release candidate:

1. Push the RC tag to trigger the automated build
2. Share the RC with testers in the Discord server
3. Gather feedback and fix any issues
4. Once validated, create the final release

## Questions?

If you have any questions or need help, feel free to:

- Open an issue on [GitHub Issues](https://github.com/Wayfarer-Labs/owl-control/issues)
- Join the discussion in the Discord server
