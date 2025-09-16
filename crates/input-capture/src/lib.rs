use color_eyre::Result;
use tokio::sync::mpsc;

mod kbm_capture;
use kbm_capture::KbmCapture;

mod gamepad_capture;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    /// Relative mouse movement (x, y)
    MouseMove([i32; 2]),
    /// Mouse button press or release
    MousePress { key: u16, press_state: PressState },
    /// Mouse scroll wheel movement
    /// Negative values indicate scrolling down, positive values indicate scrolling up.
    MouseScroll { scroll_amount: i16 },
    /// Keyboard key press or release
    KeyPress { key: u16, press_state: PressState },
    /// Gamepad button press or release
    GamepadButtonPress { key: u16, press_state: PressState },
    /// Gamepad button value change (e.g. analogue buttons like triggers)
    GamepadButtonChange { key: u16, value: f32 },
    /// Gamepad axis value change
    GamepadAxisChange { axis: u16, value: f32 },
}
impl Event {
    pub fn key_press_keycode(&self) -> Option<u16> {
        match self {
            Event::KeyPress {
                key,
                press_state: PressState::Pressed,
            } => Some(*key),
            _ => None,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressState {
    Pressed,
    Released,
}

pub struct InputCapture {
    _raw_input_thread: std::thread::JoinHandle<()>,
    _gilrs_thread: std::thread::JoinHandle<()>,
}
impl InputCapture {
    pub fn new() -> Result<(Self, mpsc::Receiver<Event>)> {
        let (input_tx, input_rx) = mpsc::channel(10);

        let _raw_input_thread = std::thread::spawn({
            let input_tx = input_tx.clone();
            move || {
                let mut raw_input =
                    Some(KbmCapture::initialize().expect("failed to initialize raw input"));
                KbmCapture::run_queue(move |event| {
                    if input_tx.blocking_send(event).is_err() {
                        tracing::warn!("Keyboard input tx closed, stopping keyboard capture");
                        // Force the raw input to be dropped, which will unregister the window
                        // class and destroy the window, stopping the message queue.
                        raw_input.take();
                    }
                })
                .expect("failed to run windows message queue");
            }
        });

        let _gilrs_thread = gamepad_capture::initialize_thread(input_tx);

        Ok((
            Self {
                _raw_input_thread,
                _gilrs_thread,
            },
            input_rx,
        ))
    }
}
