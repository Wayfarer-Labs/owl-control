use std::path::Path;

use color_eyre::{
    Result,
    eyre::{WrapErr as _, eyre},
};
use tokio::{fs::File, io::AsyncWriteExt as _, sync::mpsc};

use crate::output_types::{InputEvent, InputEventType};

/// Stream for sending timestamped input events to the writer
#[derive(Clone)]
pub(crate) struct InputEventStream {
    tx: mpsc::UnboundedSender<InputEvent>,
}

impl InputEventStream {
    /// Send a timestamped input event at current time. This is the only supported send
    /// since now that we rely on the rx queue to flush outputs to file, we also want this
    /// queue to be populated in chronological order, so arbitrary timestamp writing
    /// shouldn't be supported anyway.
    pub(crate) fn send_input(&self, event: input_capture::Event) -> Result<()> {
        // maybe we can make this faster by deferring lookups until flush event, but don't think
        // such level of optimisation is necessary yet.
        let event_type = InputEventType::from_input_event(event)?;
        let input_event = InputEvent::new_at_now(event_type);
        self.tx
            .send(input_event)
            .map_err(|_| eyre!("input event stream receiver was closed"))?;
        Ok(())
    }

    /// Send a timestamped video_start or video_end event.
    pub(crate) fn send_video_state(&self, starting: bool) -> Result<()> {
        let event = InputEvent::new_at_now(match starting {
            true => InputEventType::VideoStart,
            false => InputEventType::VideoEnd,
        });
        self.tx
            .send(event)
            .map_err(|_| eyre!("input event stream receiver was closed"))?;
        Ok(())
    }
}

pub(crate) struct InputEventWriter {
    file: File,
    rx: mpsc::UnboundedReceiver<InputEvent>,
}

impl InputEventWriter {
    pub(crate) async fn start(path: &Path) -> Result<(Self, InputEventStream)> {
        let file = File::create_new(path)
            .await
            .wrap_err_with(|| eyre!("failed to create and open {path:?}"))?;

        let (tx, rx) = mpsc::unbounded_channel();
        let stream = InputEventStream { tx };
        let mut writer = Self { file, rx };

        writer.write_header().await?;
        writer
            .write_entry(InputEvent::new_at_now(InputEventType::Start))
            .await?;

        Ok((writer, stream))
    }

    /// Flush all pending events from the channel and write them to file
    pub(crate) async fn flush(&mut self) -> Result<()> {
        while let Ok(event) = self.rx.try_recv() {
            self.write_entry(event).await?;
        }
        Ok(())
    }

    pub(crate) async fn stop(mut self) -> Result<()> {
        // Most accurate possible timestamp of exactly when the stop input recording was called
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        // Flush any remaining events
        self.flush().await?;

        // Write the end marker
        self.write_entry(InputEvent::new(timestamp, InputEventType::End))
            .await
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
