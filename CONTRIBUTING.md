# Contributing to OWL Control

Thanks for your interest in contributing to OWL Control! 🦉

## General Information

For most development information, including building from source, system requirements, and code formatting, please see the [README.md](README.md).

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
