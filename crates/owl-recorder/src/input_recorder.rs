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
/// KB: [keycode : int, key_down : bool] (key down = true, key up = false)
/// MB: [button_idx : int, key_down : bool]
/// MM: [dx : int, dy : int]
/// SCROLL: [amt : int] (positive = up)
impl InputRecorder {
    pub(crate) async fn start(path: &Path) -> Result<Self> {
        let file = File::create_new(path)
            .await
            .wrap_err_with(|| eyre!("failed to create and open {path:?}"))?;
        let mut recorder = Self { file };

        recorder.write_header().await?;
        recorder.write_event(EventType::Start).await?;

        Ok(recorder)
    }

    pub(crate) async fn seen_input(&mut self, e: input_capture::Event) -> Result<()> {
        self.write_event(EventType::Input(e)).await
    }

    pub(crate) async fn stop(mut self) -> Result<()> {
        self.write_event(EventType::End).await
    }

    async fn write_header(&mut self) -> Result<()> {
        const HEADER: &str = "timestamp,event_type,event_args\n";
        self.file.write_all(HEADER.as_bytes()).await?;
        Ok(())
    }

    async fn write_event(&mut self, event_type: EventType) -> Result<()> {
        let entry = Entry {
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64(),
            event_type,
        };
        self.write_entry(entry).await
    }

    async fn write_entry(
        &mut self,
        Entry {
            timestamp,
            event_type,
        }: Entry,
    ) -> Result<()> {
        let (event_name, event_data): (&'static str, serde_json::Value) = match event_type {
            EventType::Start => ("START", json!([])),
            EventType::End => ("END", json!([])),
            EventType::Input(event) => match event {
                input_capture::Event::MouseMove([x, y]) => ("MOUSE_MOVE", json!([x, y])),
                input_capture::Event::MousePress { key, press_state } => (
                    "MOUSE_BUTTON",
                    json!([key, press_state == PressState::Pressed]),
                ),
                input_capture::Event::MouseScroll { scroll_amount } => {
                    ("SCROLL", json!([scroll_amount]))
                }
                input_capture::Event::KeyPress { key, press_state } => {
                    ("KEYBOARD", json!([key, press_state == PressState::Pressed]))
                }
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

struct Entry {
    timestamp: f64,
    event_type: EventType,
}

enum EventType {
    Start,
    End,
    Input(input_capture::Event),
}
