use color_eyre::Result;
use input_capture::InputCapture;

pub fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let (_input_capture, mut input_rx) = InputCapture::new()?;
    while let Some(event) = input_rx.blocking_recv() {
        tracing::info!(?event, "Received raw input event");
    }

    Ok(())
}
