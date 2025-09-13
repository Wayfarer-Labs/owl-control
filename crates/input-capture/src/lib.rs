use color_eyre::Result;
use tokio::sync::mpsc;

mod raw_input;
use raw_input::RawInput;

#[derive(Debug, Clone, Copy)]
pub enum Event {
    MouseMove([i32; 2]),
    MousePress {
        key: u16,
        press_state: PressState,
    },
    /// Negative values indicate scrolling down, positive values indicate scrolling up.
    MouseScroll {
        scroll_amount: i16,
    },
    KeyPress {
        key: u16,
        press_state: PressState,
    },
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

        let _raw_input_thread = std::thread::spawn(move || {
            let mut raw_input =
                Some(RawInput::initialize().expect("failed to initialize raw input"));
            RawInput::run_queue(move |event| {
                if input_tx.blocking_send(event).is_err() {
                    // Force the raw input to be dropped, which will unregister the window
                    // class and destroy the window, stopping the message queue.
                    raw_input.take();
                }
            })
            .expect("failed to run windows message queue");
        });

        let _gilrs_thread = std::thread::spawn(move || {
            let mut gilrs = gilrs::Gilrs::new().unwrap();

            // Iterate over all connected gamepads
            for (_id, gamepad) in gilrs.gamepads() {
                println!("{} is {:?}", gamepad.name(), gamepad.power_info());
            }
            loop {
                // Examine new events
                while let Some(gilrs::Event {
                    id, event, time, ..
                }) = gilrs.next_event_blocking(None)
                {
                    println!("{:?} New event from {}: {:?}", time, id, event);
                }
            }
        });

        Ok((
            Self {
                _raw_input_thread,
                _gilrs_thread,
            },
            input_rx,
        ))
    }
}
