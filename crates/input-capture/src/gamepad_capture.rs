use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, RwLock},
};

use tokio::sync::mpsc;

use crate::{Event, PressState};

// Copied from gilrs 0.11 constants; I want to ensure stable identifiers for these
// to ensure we don't have to change them if gilrs changes them.
pub const BTN_UNKNOWN: u16 = 0;

pub const BTN_SOUTH: u16 = 1;
pub const BTN_EAST: u16 = 2;
pub const BTN_C: u16 = 3;
pub const BTN_NORTH: u16 = 4;
pub const BTN_WEST: u16 = 5;
pub const BTN_Z: u16 = 6;
pub const BTN_LT: u16 = 7;
pub const BTN_RT: u16 = 8;
pub const BTN_LT2: u16 = 9;
pub const BTN_RT2: u16 = 10;
pub const BTN_SELECT: u16 = 11;
pub const BTN_START: u16 = 12;
pub const BTN_MODE: u16 = 13;
pub const BTN_LTHUMB: u16 = 14;
pub const BTN_RTHUMB: u16 = 15;

pub const BTN_DPAD_UP: u16 = 16;
pub const BTN_DPAD_DOWN: u16 = 17;
pub const BTN_DPAD_LEFT: u16 = 18;
pub const BTN_DPAD_RIGHT: u16 = 19;

pub const AXIS_UNKNOWN: u16 = 0;

pub const AXIS_LSTICKX: u16 = 1;
pub const AXIS_LSTICKY: u16 = 2;
pub const AXIS_LEFTZ: u16 = 3;
pub const AXIS_RSTICKX: u16 = 4;
pub const AXIS_RSTICKY: u16 = 5;
pub const AXIS_RIGHTZ: u16 = 6;
pub const AXIS_DPADX: u16 = 7;
pub const AXIS_DPADY: u16 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadId {
    XInput(usize),
    WGI(usize),
}
impl std::fmt::Display for GamepadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GamepadId::XInput(id) => write!(f, "XInput:{}", id),
            GamepadId::WGI(id) => write!(f, "WGI:{}", id),
        }
    }
}
impl std::str::FromStr for GamepadId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(id) = s.strip_prefix("XInput:") {
            return Ok(GamepadId::XInput(id.parse::<usize>().unwrap()));
        }
        if let Some(id) = s.strip_prefix("WGI:") {
            return Ok(GamepadId::WGI(id.parse::<usize>().unwrap()));
        }
        Err(format!("Invalid gamepad id: {s}"))
    }
}
impl serde::Serialize for GamepadId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}
impl<'de> serde::Deserialize<'de> for GamepadId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ActiveGamepad {
    pub digital: HashSet<u16>,
    pub analog: HashMap<u16, f32>,
    pub axis: HashMap<u16, f32>,
}

#[derive(Default)]
pub struct ActiveGamepads {
    pub devices: HashMap<GamepadId, ActiveGamepad>,
}
impl ActiveGamepads {
    pub fn get_or_insert(&mut self, id: GamepadId) -> &mut ActiveGamepad {
        self.devices.entry(id).or_insert_with(|| ActiveGamepad {
            digital: HashSet::new(),
            analog: HashMap::new(),
            axis: HashMap::new(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GamepadMetadata {
    pub name: String,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
}

pub struct GamepadThreads {
    _xinput_thread: std::thread::JoinHandle<()>,
    _wgi_thread: std::thread::JoinHandle<()>,
}

pub fn initialize_thread(
    input_tx: mpsc::Sender<Event>,
    active_gamepads: Arc<Mutex<ActiveGamepads>>,
    gamepads: Arc<RwLock<HashMap<GamepadId, GamepadMetadata>>>,
) -> GamepadThreads {
    let already_captured_by_xinput = Arc::new(RwLock::new(HashSet::new()));

    // We use both the `xinput` and `wgi` versions of gilrs so that we can capture Xbox controllers
    // (which only work with `xinput`) and PS controllers (which only work with `wgi`).
    //
    // We should ostensibly be able to use just `wgi`, but `wgi` only supports all controllers with a focused window
    // associated with the process, which we can't do for our delightful little input recorder.
    // However, it *does* work without a focused window for PS controllers.
    //
    // I love Windows.

    // xinput
    let _xinput_thread = std::thread::spawn({
        let already_captured_by_xinput = already_captured_by_xinput.clone();
        let gamepads = gamepads.clone();
        let input_tx = input_tx.clone();
        let active_gamepads = active_gamepads.clone();
        move || {
            let mut gilrs = gilrs_xinput::Gilrs::new().unwrap();

            // Examine new events
            while let Some(gilrs_xinput::Event { id, event, .. }) = gilrs.next_event_blocking(None)
            {
                let gamepad = gilrs.gamepad(id);
                gamepads.write().unwrap().insert(
                    GamepadId::XInput(id.into()),
                    GamepadMetadata {
                        name: gamepad.name().to_string(),
                        vendor_id: gamepad.vendor_id(),
                        product_id: gamepad.product_id(),
                    },
                );

                already_captured_by_xinput
                    .write()
                    .unwrap()
                    .insert(gamepad.name().to_string());

                let Some(event) = map_event_xinput(GamepadId::XInput(id.into()), event) else {
                    continue;
                };
                update_active_gamepad(&mut active_gamepads.lock().unwrap(), event);
                if input_tx.blocking_send(event).is_err() {
                    tracing::warn!("Gamepad input tx closed, stopping gamepad capture");
                    break;
                }
            }
        }
    });

    // wgi
    let _wgi_thread = std::thread::spawn({
        let already_captured_by_xinput = already_captured_by_xinput.clone();
        move || {
            let mut gilrs = gilrs_wgi::Gilrs::new().unwrap();

            // Examine new events
            while let Some(gilrs_wgi::Event { id, event, .. }) = gilrs.next_event_blocking(None) {
                let gamepad = gilrs.gamepad(id);
                gamepads.write().unwrap().insert(
                    GamepadId::WGI(id.into()),
                    GamepadMetadata {
                        name: gamepad.name().to_string(),
                        vendor_id: gamepad.vendor_id(),
                        product_id: gamepad.product_id(),
                    },
                );
                if already_captured_by_xinput
                    .read()
                    .unwrap()
                    .contains(&gamepad.name().to_string())
                {
                    continue;
                }

                let Some(event) = map_event_wgi(GamepadId::WGI(id.into()), event) else {
                    continue;
                };
                update_active_gamepad(&mut active_gamepads.lock().unwrap(), event);
                if input_tx.blocking_send(event).is_err() {
                    tracing::warn!("Gamepad input tx closed, stopping gamepad capture");
                    break;
                }
            }
        }
    });

    fn update_active_gamepad(active_gamepads: &mut ActiveGamepads, event: Event) {
        match event {
            Event::GamepadButtonPress {
                key,
                press_state,
                id,
            } => {
                let active_gamepad = active_gamepads.get_or_insert(id);
                if press_state == PressState::Pressed {
                    active_gamepad.digital.insert(key);
                } else {
                    active_gamepad.digital.remove(&key);
                    active_gamepad.analog.remove(&key);
                }
            }
            Event::GamepadButtonChange { key, value, id } => {
                active_gamepads.get_or_insert(id).analog.insert(key, value);
            }
            Event::GamepadAxisChange { axis, value, id } => {
                active_gamepads.get_or_insert(id).axis.insert(axis, value);
            }
            _ => {}
        }
    }

    GamepadThreads {
        _xinput_thread,
        _wgi_thread,
    }
}

macro_rules! generate_map_functions {
    ($gilrs:ident, $map_event:ident, $map_button:ident, $map_axis:ident) => {
        fn $map_event(gamepad_id: GamepadId, event: $gilrs::EventType) -> Option<Event> {
            use $gilrs::EventType;
            match event {
                EventType::ButtonPressed(button, _) => Some(Event::GamepadButtonPress {
                    id: gamepad_id,
                    key: $map_button(button),
                    press_state: PressState::Pressed,
                }),
                EventType::ButtonReleased(button, _) => Some(Event::GamepadButtonPress {
                    id: gamepad_id,
                    key: $map_button(button),
                    press_state: PressState::Released,
                }),
                EventType::ButtonChanged(button, value, _) => Some(Event::GamepadButtonChange {
                    id: gamepad_id,
                    key: $map_button(button),
                    value,
                }),
                EventType::AxisChanged(axis, value, _) => Some(Event::GamepadAxisChange {
                    id: gamepad_id,
                    axis: $map_axis(axis),
                    value,
                }),
                EventType::ButtonRepeated(..)
                | EventType::Connected
                | EventType::Disconnected
                | EventType::Dropped
                | EventType::ForceFeedbackEffectCompleted
                | _ => None,
            }
        }

        fn $map_button(button: $gilrs::Button) -> u16 {
            use $gilrs::Button;
            match button {
                Button::South => BTN_SOUTH,
                Button::East => BTN_EAST,
                Button::North => BTN_NORTH,
                Button::West => BTN_WEST,
                Button::C => BTN_C,
                Button::Z => BTN_Z,
                Button::LeftTrigger => BTN_LT,
                Button::LeftTrigger2 => BTN_LT2,
                Button::RightTrigger => BTN_RT,
                Button::RightTrigger2 => BTN_RT2,
                Button::Select => BTN_SELECT,
                Button::Start => BTN_START,
                Button::Mode => BTN_MODE,
                Button::LeftThumb => BTN_LTHUMB,
                Button::RightThumb => BTN_RTHUMB,
                Button::DPadUp => BTN_DPAD_UP,
                Button::DPadDown => BTN_DPAD_DOWN,
                Button::DPadLeft => BTN_DPAD_LEFT,
                Button::DPadRight => BTN_DPAD_RIGHT,
                Button::Unknown => BTN_UNKNOWN,
            }
        }

        fn $map_axis(axis: $gilrs::Axis) -> u16 {
            use $gilrs::Axis;
            match axis {
                Axis::LeftStickX => AXIS_LSTICKX,
                Axis::LeftStickY => AXIS_LSTICKY,
                Axis::LeftZ => AXIS_LEFTZ,
                Axis::RightStickX => AXIS_RSTICKX,
                Axis::RightStickY => AXIS_RSTICKY,
                Axis::RightZ => AXIS_RIGHTZ,
                Axis::DPadX => AXIS_DPADX,
                Axis::DPadY => AXIS_DPADY,
                Axis::Unknown => AXIS_UNKNOWN,
            }
        }
    };
}

generate_map_functions!(
    gilrs_xinput,
    map_event_xinput,
    map_button_xinput,
    map_axis_xinput
);
generate_map_functions!(gilrs_wgi, map_event_wgi, map_button_wgi, map_axis_wgi);
