use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use color_eyre::{
    Result,
    eyre::{WrapErr as _, eyre},
};
use input_capture::PressState;
use serde_json::json;
use tokio::{fs::File, io::AsyncWriteExt as _};

pub(crate) struct InputRecorder {
    file: File,
}

/// Quick Rundown on Event Datasets:
///
/// When stored as CSVs, each row has:
/// - timestamp [unix time]
/// - event type (see events.py) [str]
/// - event_args (see callback args) [list[any]]
///
/// Event Args on different kinds of events:
/// KEYBOARD: [keycode : int, key_down : bool] (key down = true, key up = false)
/// MOUSE_BUTTON: [button_idx : int, key_down : bool]
/// MOUSE_MOVE: [dx : int, dy : int]
/// SCROLL: [amt : int] (positive = up)
/// GAMEPAD_BUTTON: [button_idx : int, key_down : bool]
/// GAMEPAD_BUTTON_VALUE: [button_idx : int, value : float]
/// GAMEPAD_AXIS: [axis_idx : int, value : float]
enum EventType {
    Start,
    End,
    Input(input_capture::Event),
}

impl InputRecorder {
    pub(crate) async fn start(path: &Path) -> Result<Self> {
        let file = File::create_new(path)
            .await
            .wrap_err_with(|| eyre!("failed to create and open {path:?}"))?;
        let mut recorder = Self { file };

        recorder.write_header().await?;
        recorder.write_entry(EventType::Start).await?;

        Ok(recorder)
    }

    pub(crate) async fn seen_input(&mut self, e: input_capture::Event) -> Result<()> {
        self.write_entry(EventType::Input(e)).await
    }

    pub(crate) async fn stop(mut self) -> Result<()> {
        self.write_entry(EventType::End).await
    }

    async fn write_header(&mut self) -> Result<()> {
        const HEADER: &str = "timestamp,event_type,event_args\n";
        self.file.write_all(HEADER.as_bytes()).await?;
        Ok(())
    }

    async fn write_entry(&mut self, event_type: EventType) -> Result<()> {
        use input_capture::Event;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        let (event_name, event_data): (&'static str, serde_json::Value) = match event_type {
            EventType::Start => ("START", json!([])),
            EventType::End => ("END", json!([])),
            EventType::Input(event) => match event {
                Event::MouseMove([x, y]) => ("MOUSE_MOVE", json!([x, y])),
                Event::MousePress { key, press_state } => (
                    "MOUSE_BUTTON",
                    json!([key, press_state == PressState::Pressed]),
                ),
                Event::MouseScroll { scroll_amount } => ("SCROLL", json!([scroll_amount])),
                Event::KeyPress { key, press_state } => {
                    ("KEYBOARD", json!([key, press_state == PressState::Pressed]))
                }
                Event::GamepadButtonPress { key, press_state } => (
                    "GAMEPAD_BUTTON",
                    json!([key, press_state == PressState::Pressed]),
                ),
                Event::GamepadButtonChange { key, value } => {
                    ("GAMEPAD_BUTTON_VALUE", json!([key, value]))
                }
                Event::GamepadAxisChange { axis, value } => ("GAMEPAD_AXIS", json!([axis, value])),
            },
        };
        let event_data = event_data.to_string();
        let line = format!("{timestamp},{event_name},\"{event_data}\"\n");
        self.file
            .write_all(line.as_bytes())
            .await
            .wrap_err("failed to save entry to inputs file")
    }
}
