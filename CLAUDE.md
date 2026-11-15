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
- First checks `~/.config/emulaunch/config.toml`
- Falls back to `~/.emulaunch/config.toml`

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

# Run linter (CI runs with -D warnings, so fix all warnings)
cargo clippy --all-targets -- -D warnings

# Format code (2-space indentation via rustfmt.toml)
cargo fmt

# Run the unit tests (CI runs them on Linux, macOS and Windows)
cargo test
```

**Tests** are unit tests in `#[cfg(test)]` modules next to the code. They need no emulators, `adb` or Xcode: parsing of tool output (`adb`, `simctl`, `config.ini`) and the TUI state machine (`App::handle_key`) are separated from process execution so they can be tested with plain strings. Keep it that way: put new parsing in a function that takes text, and test that. Tests that need files use a self-cleaning temp dir; avoid tests that change environment variables (they run in parallel).

## Architecture

```
src/
  main.rs        — CLI entry point (clap) + TUI app loop (ratatui)
  emulators.rs   — Core logic: listing, opening, types
  error.rs       — Typed `Error` enum (thiserror) and `Result` alias
  config.rs      — Configuration loading (TOML file, env vars, platform defaults)
  theme.rs       — TUI colour themes
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

### Core Types (`src/emulators.rs`)

- **`Device`** - `{platform, name, id, state, meta}` for both Android emulators and iOS simulators. `id` is the AVD id or the simulator UDID; `meta` is the AVD kind (Android) or the runtime (iOS)
- **`Platform`** - `Android` or `Ios`; owns per-platform wording (section title, detail labels)
- **`State`** - `Booted`, `Shutdown` or `Other(String)`
- **`Error`** (`src/error.rs`) - every fallible function returns `error::Result`

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
- **iOS**: `xcrun simctl boot <udid>` boots, then the GUI is opened by bundle id: Simulator.app (`com.apple.iphonesimulator`, with `-CurrentDeviceUDID`) on older Xcode, falling back to Device Hub (`com.apple.dt.Devices`) on Xcode 27+, which no longer ships Simulator.app

The `open` command matches against both `name` (display name) and `id` values, so either identifier can be used. If a name matches several devices (same name on both platforms, or on two iOS runtimes) it fails and lists them; pass the id to pick one.

### TUI (`src/main.rs`)

- ratatui + crossterm for terminal UI
- clap for CLI argument parsing
- Scrollable list; the app keeps a flat `Vec<Device>` and inserts platform headings while building the rows
- Devices are listed on a background thread (`App::start_loading` / `poll_loading`), so the UI opens at once with a spinner in the list title and `Ctrl+R` never blocks. The old list stays visible while a refresh runs; a second refresh is ignored until it finishes. If the first listing finds nothing, the loop ends and the reasons are printed after the terminal is restored. Android and iOS are listed in parallel (`list_all_platforms`)
- Real-time substring filtering by typing (every printable key goes into the filter)
- Arrows or `Ctrl+N`/`Ctrl+P` to navigate, `Enter` to launch, `Ctrl+R`/`F5` to refresh, `?`/`F1` for help, `Esc`/`Ctrl+C` to quit

### Platform Guards

iOS-specific code uses `#[cfg(target_os = "macos")]`. Non-macOS builds get stub functions that return `Error::IosUnsupported`.

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

- `.github/workflows/ci.yml` - Runs `cargo check`, `cargo clippy -- -D warnings` on Ubuntu; builds on Ubuntu, macOS, and Windows
- `.github/workflows/release.yml` - GitHub Actions workflow for building releases via `cargo-dist`
- `dist-workspace.toml` - dist configuration for target platforms

