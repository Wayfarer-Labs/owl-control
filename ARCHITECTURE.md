# Architecture

This document describes the high-level architecture of OWL Control. For build instructions and contribution guidelines, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Overview

OWL Control is a Windows application that captures control data (keyboard, mouse, gamepad inputs) from video games alongside synchronized video recordings. This data is used to train world models and AI agents.

The application is written in Rust and organized as a Cargo workspace with multiple crates. It uses a multi-threaded architecture with a native GUI built on egui/winit and video recording via embedded OBS.

## Workspace Structure

```
owl-control/
├── src/                    # Main application
├── crates/
│   ├── constants/          # Shared constants and game data
│   ├── game-process/       # Windows process management
│   └── input-capture/      # Keyboard, mouse, gamepad capture
├── tools/
│   ├── bump-version/       # Semantic version updater
│   └── update-games/       # GAMES.md generator
└── build-resources/        # Build scripts, installer config
```

## Crates

### `owl-control` (main binary)

The core application containing the UI, recording logic, and API integration. This is where everything comes together.

### `crates/constants`

Shared constants used across the codebase:
- Recording parameters (FPS, resolution, duration limits)
- Video encoder configurations (X264, NVENC, AMF, QSV)
- Supported games list with Steam app IDs
- File naming conventions and paths

### `crates/game-process`

Windows API wrapper for process management:
- Getting the foreground window and its process
- Enumerating process modules
- Process name detection

### `crates/input-capture`

Input capture for keyboard, mouse, and gamepads:
- Keyboard/mouse via Windows raw input message queue
- Gamepads via both XInput and Windows.Gaming.Input (WGI)
- Spawns dedicated capture threads that send events through channels

## Main Application Modules

### `src/main.rs` - Entry Point

Sets up the multi-threaded architecture:
1. Initializes logging, config, and shared state
2. Spawns the tokio async thread
3. Runs the UI event loop on the main thread

### `src/app_state.rs` - Shared State

The central state container, wrapped in `Arc` and passed to all major systems. Holds the current recording status, user configuration, channel senders for inter-thread communication, and upload state flags.

### `src/config.rs` - Configuration

User preferences persisted to `config.json`:
- Hotkeys for start/stop recording
- Overlay position and opacity
- Recording backend selection
- Encoder settings
- Per-game configurations

### `src/tokio_thread.rs` - Async Runtime

The heart of the application. Runs on a dedicated thread with a tokio runtime and handles:
- Recording lifecycle (start, monitor, stop)
- Game focus monitoring (pause when game loses focus)
- Idle time tracking (stop after max idle duration)
- Recording validation
- API communication
- Upload orchestration

Communicates with the UI thread via channels.

### `src/record/` - Recording

- **`recorder.rs`** - Orchestrates recording sessions. Defines the `VideoRecorder` trait that abstracts video backends.
- **`recording.rs`** - Manages a single recording session, coordinating video and input capture.
- **`input_recorder.rs`** - Writes timestamped input events to CSV files.
- **`obs_embedded_recorder.rs`** - Embeds OBS directly. Supports multiple hardware encoders.
- **`obs_socket_recorder.rs`** - Connects to an external OBS instance via WebSocket.
- **`local_recording.rs`** - Manages recording folder structure and state markers.

### `src/output_types.rs` - Event Types

Defines the event types for all recorded inputs (mouse, keyboard, gamepad) and session markers. These are written to `inputs.csv` in each recording.

### `src/api/` - API Integration

HTTP client for backend communication, API key validation, and chunked multipart uploads with resume capability.

### `src/upload/` - Upload Manager

Handles uploading completed recordings: scans for ready recordings, creates TAR archives, uploads in chunks with progress tracking, and marks uploaded files.

### `src/ui/` - User Interface

- **`mod.rs`** - WGPU rendering setup and winit event loop integration.
- **`views/`** - Application views (login, consent, main).
- **`overlay.rs`** - In-game overlay showing recording status.
- **`tray_icon.rs`** - System tray integration.
- **`components/`** - Reusable UI widgets.

### `src/validation/` - Recording Validation

Validates recordings before upload: duration, average FPS, input variety and count.

### `src/system/` - System Integration

Platform utilities: single instance enforcement, hardware identification, system specs collection, keycode mapping.

### `src/play_time.rs` - Play Time Tracking

Monitors accumulated play time and shows health reminders at configurable thresholds.

## Threading Model

| Thread | Purpose | Communication |
|--------|---------|---------------|
| Main (UI) | Winit event loop, egui rendering | Channels to/from tokio |
| Tokio | Recording, API, uploads | Channels to/from UI |
| Raw Input | Keyboard/mouse capture | Channel to tokio |
| Gamepad (2x) | XInput + WGI polling | Channels to tokio |
| Overlay | In-game overlay rendering | Shared state |

Inter-thread communication uses tokio channels:
- `mpsc` for request/response patterns
- `broadcast` for one-to-many updates

## Data Flow

### Recording Session

```
User presses start hotkey
    │
    ├─► Detect game process
    ├─► Initialize input capture
    └─► Start video recording (OBS backend)
           │
           ▼
    ┌─────────────────────────────────┐
    │  Recording Loop                 │
    │  • Write input events to CSV    │
    │  • Monitor game focus           │
    │  • Track idle time              │
    │  • Check duration limits        │
    └─────────────────────────────────┘
           │
           ▼
User presses stop hotkey (or timeout)
    │
    ├─► Stop video recording
    ├─► Validate recording
    ├─► Create metadata.json
    └─► Mark ready for upload
           │
           ▼
    Upload triggered
    │
    ├─► Create TAR archive
    ├─► Chunked upload with progress
    └─► Mark as uploaded
```

### Recording Output

Each recording creates a folder containing:
- `recording.mp4` - Video file
- `inputs.csv` - Timestamped input events
- `metadata.json` - Hardware specs, settings, session info

## Extending the System

### Adding a New Video Recording Backend

Video recording is abstracted behind the `VideoRecorder` trait in [src/record/recorder.rs](src/record/recorder.rs). To add a new backend (e.g., for VR recording):

1. **Implement the `VideoRecorder` trait:**

```rust
#[async_trait::async_trait(?Send)]
pub trait VideoRecorder {
    /// Unique identifier for this backend (e.g., "obs-embedded", "vr-recorder")
    fn id(&self) -> &'static str;

    /// List of video encoders this backend supports
    fn available_encoders(&self) -> &[VideoEncoderType];

    /// Start recording. Receives the target window, process info, encoder settings,
    /// and an InputEventStream to emit video-related events (VIDEO_START, HOOK_START, etc.)
    async fn start_recording(
        &mut self,
        dummy_video_path: &Path,
        pid: u32,
        hwnd: HWND,
        game_exe: &str,
        video_settings: EncoderSettings,
        game_config: GameConfig,
        game_resolution: (u32, u32),
        event_stream: InputEventStream,
    ) -> Result<()>;

    /// Stop recording and return any backend-specific metadata as JSON
    async fn stop_recording(&mut self) -> Result<serde_json::Value>;

    /// Called periodically during recording. Return current FPS if available.
    async fn poll(&mut self) -> PollUpdate;

    /// Check if a window can be captured by this backend
    fn is_window_capturable(&self, hwnd: HWND) -> bool;

    /// Returns true if the recording hook has timed out (failed to attach)
    async fn check_hook_timeout(&mut self) -> bool;
}
```

2. **Key integration points:**
   - The backend receives an `InputEventStream` to emit timing events (`VIDEO_START`, `HOOK_START`, `VIDEO_END`) that synchronize with the input CSV
   - `poll()` is called regularly during recording to update FPS in the UI
   - Return backend-specific metadata from `stop_recording()` which gets stored in `metadata.json` under `recorder_extra`

3. **Wire it up in `Recorder::new()`** based on user config (see `RecordingBackend` enum in `src/config.rs`)

4. **Add any new encoder types** to `crates/constants/src/encoding.rs` if your backend supports encoders not already listed

See [src/record/obs_embedded_recorder.rs](src/record/obs_embedded_recorder.rs) and [src/record/obs_socket_recorder.rs](src/record/obs_socket_recorder.rs) for reference implementations.

### Adding a New Input Source

Input capture is handled by the `input-capture` crate, which sends events through a channel to the main application. To add a new input source (e.g., VR controllers, eye tracking):

1. **Define new event variants** in `input-capture/src/lib.rs`:

```rust
pub enum Event {
    // Existing events...
    MouseMove([i32; 2]),
    KeyPress { key: u16, press_state: PressState },
    // Add new variants for your input type
    VrController { hand: Hand, position: [f32; 3], rotation: [f32; 4], ... },
}
```

2. **Create a capture module** (similar to `kbm_capture.rs` or `gamepad_capture.rs`):
   - Spawn a dedicated thread for polling/listening to the input device
   - Send events through the `mpsc::Sender<Event>` channel
   - Track active state in a shared struct (e.g., which buttons are currently held)

3. **Initialize your capture** in `InputCapture::new()` and store the thread handle

4. **Map to output events** by adding corresponding variants to `InputEventType` in [src/output_types.rs](src/output_types.rs):
   - Add a new variant with a unique string ID (e.g., `"VR_CONTROLLER"`)
   - Implement `from_input_event()` to convert from `input_capture::Event`
   - Implement `json_args()` to serialize the event data
   - Implement `from_id_and_json_args()` for backwards compatibility when reading CSVs

5. **Update validation** in `src/validation/` if the new input type should contribute to recording validation metrics

**Important:** Never remove event type variants—mark them `#[deprecated]` instead. This ensures older recordings remain parseable.

### Input/Output Format

Events are stored in `inputs.csv` with the format:
```
timestamp,EVENT_TYPE,"[json_args]"
```

For example:
```
1699900000.123,KEYBOARD,"[65,true]"
1699900000.456,MOUSE_MOVE,"[10,-5]"
1699900000.789,GAMEPAD_AXIS,"[0,0.75,1]"
```

The timestamp is Unix time (seconds since epoch with fractional milliseconds). The JSON args format varies by event type—see `InputEventType::json_args()` for the exact format of each.

## Constraints

1. **Windows-only** - Relies on Win32 APIs for input capture and process management
2. **Single instance** - Enforced via named mutex to prevent conflicts
3. **Fixed recording parameters** - 60 FPS, 640x360 resolution
4. **Backwards compatibility** - Event types are never removed, only deprecated
