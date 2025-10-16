use std::path::Path;

use color_eyre::{
    Result,
    eyre::{WrapErr as _, eyre},
};
use tokio::{fs::File, io::AsyncWriteExt as _};

use crate::output_types::{InputEvent, InputEventType};

pub(crate) struct InputRecorder {
    file: File,
}

impl InputRecorder {
    pub(crate) async fn start(path: &Path) -> Result<Self> {
        let file = File::create_new(path)
            .await
            .wrap_err_with(|| eyre!("failed to create and open {path:?}"))?;
        let mut recorder = Self { file };

        recorder.write_header().await?;
        recorder
            .write_entry(InputEvent::new_at_now(InputEventType::Start))
            .await?;

        Ok(recorder)
    }

    pub(crate) async fn seen_input(&mut self, e: input_capture::Event) -> Result<()> {
        self.write_entry(InputEvent::new_at_now(InputEventType::from_input_event(e)?))
            .await
    }

    pub(crate) async fn stop(mut self) -> Result<()> {
        self.write_entry(InputEvent::new_at_now(InputEventType::End))
            .await
    }

    pub(crate) async fn write_focus(&mut self, focused: bool) -> Result<()> {
        // write alt tab status to the input tracker
        match focused {
            true => {
                self.write_entry(InputEvent::new_at_now(InputEventType::Refocus))
                    .await
            }
            false => {
                self.write_entry(InputEvent::new_at_now(InputEventType::Unfocus))
                    .await
            }
        }
    }

    async fn write_header(&mut self) -> Result<()> {
        const HEADER: &str = "timestamp,event_type,event_args\n";
        self.file.write_all(HEADER.as_bytes()).await?;
        Ok(())
    }

    async fn write_entry(&mut self, event: InputEvent) -> Result<()> {
        let line = format!("{}\n", event);
        self.file
            .write_all(line.as_bytes())
            .await
            .wrap_err("failed to save entry to inputs file")
    }
}
