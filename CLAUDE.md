# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**emulaunch** is a standalone CLI with an interactive ratatui-based TUI for listing and opening Android emulators and iOS simulators.

### Commands

- `emulaunch` (no args) - Launch interactive TUI picker
- `emulaunch list` - Print plain text list to stdout
- `emulaunch open <name>` - Directly open an emulator by name

### Configuration

Android and iOS command paths are configurable. Priority: config file > environment variables > defaults.

**Config file** (TOML format):
- First checks `~/.config/emulators/config.toml`
- Falls back to `~/.emulators/config.toml`

```toml
android_emulator_cmd = "emulator"
adb_cmd = "adb"
xcrun_cmd = "xcrun"  # macOS only
```

**Environment variables** (fallback if no config file):
```bash
export ANDROID_EMULATOR_CMD="/path/to/emulator"  # default: "emulator"
export ADB_CMD="/path/to/adb"                   # default: "adb"
export XCRUN_CMD="/path/to/xcrun"               # default: "xcrun" (macOS only)
```

## Development Commands

```bash
# Check compilation without building
cargo check

# Build the binary
cargo build

# Run directly
cargo run

# Run with subcommand
cargo run -- list
cargo run -- open <name>

# Run linter
cargo clippy
```

## Architecture

```
src/
  main.rs        — CLI entry point (clap) + TUI app loop (ratatui/crossterm)
  emulators.rs   — Core logic: listing, opening, types
  config.rs      — Configuration loading (TOML file, env vars, platform defaults)
```

### Configuration (`src/config.rs`)

Loads binary paths with priority: config file > environment variables > platform defaults.

When commands aren't found, returns `CommandNotFoundError` with helpful messages including:
- Config file locations for manual configuration
- Platform-specific installation paths
- Install instructions

Platform-specific default paths checked:
- **macOS**: `~/Library/Android/sdk/emulator/emulator`, `~/Library/Android/sdk/platform-tools/adb`
- **Linux**: `~/Android/Sdk/emulator/emulator`, `~/Android/Sdk/platform-tools/adb`
- **Windows**: `%LOCALAPPDATA%\Android\Sdk\emulator\emulator.exe`, `%LOCALAPPDATA%\Android\Sdk\platform-tools\adb.exe`

### Code Style

- Use `cargo fmt` to format (configured for 2-space indentation in `rustfmt.toml`)

### Core Types (`src/emulators.rs`)

- **`AndroidEmulator`** - `{name, id, device_type, state}`
- **`IOSSimulator`** - `{name, udid, state, runtime}`
- **`EmulatorType`** - Enum for `Android(String)` or `IOS(String)` identification
- **`EmulatorEntry`** - Unified TUI display entry (SectionHeader, Android, or IOS)

### Listing Logic

**Android** (`list_android_emulators`):
1. Primary: `emulator -list-avds` - lists all AVDs
2. Secondary: Scan `~/.android/avd/` directory for `.ini` files
3. Tertiary: `adb devices -l` - lists running devices when primary fails

Running AVDs are detected via `adb devices` + `adb -s serial emu avd name` to get state. AVD display names are parsed from `~/.android/avd/<id>.avd/config.ini` (`avd.ini.displayname`).

**iOS** (`list_ios_simulators`): macOS only
- Uses `xcrun simctl list devices available --json`
- Parses JSON response to extract simulator info

### Opening Logic

- **Android**: `emulator -avd <name>` spawns the emulator process (uses `id` field)
- **iOS**: `xcrun simctl boot <udid>` boots, then `open -a Simulator` opens GUI

The `open` command matches against both `name` (display name) and `id`/`udid` values, so either identifier can be used.

### TUI (`src/main.rs`)

- ratatui + crossterm for terminal UI
- clap for CLI argument parsing
- Scrollable list with section headers
- Real-time fuzzy filtering by typing
- `j/k`/arrows to navigate, `Enter` to launch, `q/Esc` to quit

### Platform Guards

iOS-specific code uses `#[cfg(target_os = "macos")]`. Non-macOS builds get stub functions that return `"iOS simulators are only available on macOS"`.

## Release Workflow

Releases are automated using GitHub Actions with `cargo-dist`.

### Creating a Release

1. Update version in `Cargo.toml`
2. Commit the version bump
3. Create and push a tag:
   ```bash
   git tag v0.1.0
   git push --tags
   ```
4. The GitHub Actions workflow will automatically build and publish release artifacts

### Release Artifacts

Each release includes:
- Prebuilt binaries for:
  - macOS (Intel x86_64 and Apple Silicon aarch64)
  - Linux (x86_64 and ARM64)
  - Windows (x86_64)
- Shell script installer
- Checksums for verification

### CI/CD Configuration

- `.github/workflows/release.yml` - GitHub Actions workflow for building releases
- `cargo-dist.toml` - cargo-dist configuration for target platforms
- `install.sh` - Shell installer script for end users
