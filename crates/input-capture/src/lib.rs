use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, RwLock},
};

use color_eyre::Result;
use tokio::sync::mpsc;

mod kbm_capture;
use kbm_capture::KbmCapture;

mod gamepad_capture;
pub use gamepad_capture::{ActiveGamepad, GamepadId, GamepadMetadata};

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
    GamepadButtonPress {
        key: u16,
        press_state: PressState,
        id: GamepadId,
    },
    /// Gamepad button value change (e.g. analogue buttons like triggers)
    GamepadButtonChange { key: u16, value: f32, id: GamepadId },
    /// Gamepad axis value change
    GamepadAxisChange {
        axis: u16,
        value: f32,
        id: GamepadId,
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActiveInput {
    pub keyboard: HashSet<u16>,
    pub mouse: HashSet<u16>,
    pub gamepads: HashMap<GamepadId, ActiveGamepad>,
}

pub struct InputCapture {
    _raw_input_thread: std::thread::JoinHandle<()>,
    _gamepad_threads: gamepad_capture::GamepadThreads,
    active_keys: Arc<Mutex<kbm_capture::ActiveKeys>>,
    active_gamepad: Arc<Mutex<gamepad_capture::ActiveGamepads>>,
    gamepads: Arc<RwLock<HashMap<GamepadId, GamepadMetadata>>>,
}
impl InputCapture {
    pub fn new() -> Result<(Self, mpsc::Receiver<Event>)> {
        let (input_tx, input_rx) = mpsc::channel(10);

        let active_keys = Arc::new(Mutex::new(kbm_capture::ActiveKeys::default()));
        let _raw_input_thread = std::thread::spawn({
            let input_tx = input_tx.clone();
            let active_keys = active_keys.clone();
            move || {
                KbmCapture::initialize(active_keys)
                    .expect("failed to initialize raw input")
                    .run_queue(move |event| {
                        if input_tx.blocking_send(event).is_err() {
                            tracing::warn!("Keyboard input tx closed, stopping keyboard capture");
                            return false;
                        }
                        true
                    })
                    .expect("failed to run windows message queue");
            }
        });

        let active_gamepad = Arc::new(Mutex::new(gamepad_capture::ActiveGamepads::default()));
        let gamepads = Arc::new(RwLock::new(HashMap::new()));
        let _gamepad_threads =
            gamepad_capture::initialize_thread(input_tx, active_gamepad.clone(), gamepads.clone());

        Ok((
            Self {
                _raw_input_thread,
                _gamepad_threads,
                active_keys,
                active_gamepad,
                gamepads,
            },
            input_rx,
        ))
    }

    pub fn active_input(&self) -> ActiveInput {
        let active_keys = self.active_keys.lock().unwrap();
        let active_gamepad = self.active_gamepad.lock().unwrap();
        ActiveInput {
            keyboard: active_keys.keyboard.clone(),
            mouse: active_keys.mouse.clone(),
            gamepads: active_gamepad.devices.clone(),
        }
    }

    pub fn gamepads(&self) -> HashMap<GamepadId, GamepadMetadata> {
        self.gamepads.read().unwrap().clone()
    }
}
