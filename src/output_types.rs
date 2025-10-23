use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{system::hardware_specs, upload::validation::InputStats};

#[derive(Serialize, Deserialize, Clone)]
pub struct Metadata {
    pub game_exe: String,
    // Whenever adding new fields to this, ensure you use an `Option` to ensure
    // that the uploader will not fail to upload older recordings.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub window_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub game_resolution: Option<(u32, u32)>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub owl_control_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub owl_control_commit: Option<String>,
    pub session_id: String,
    pub hardware_id: String,
    pub hardware_specs: Option<hardware_specs::HardwareSpecs>,
    pub start_timestamp: f64,
    pub end_timestamp: f64,
    pub duration: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub input_stats: Option<InputStats>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recorder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recorder_extra: Option<serde_json::Value>,
}

#[derive(Debug)]
pub enum InputEventReadError {
    /// The event type ID is not valid.
    InvalidEvent { id: String },
    /// The event args for this event type are not valid.
    InvalidArgs {
        id: String,
        args: serde_json::Value,
        error: serde_json::Error,
    },
    /// This event is missing fields.
    MissingFields { event: String },
    /// The timestamp is not a valid float.
    InvalidTimestamp { event: String },
    /// This event's args are not valid JSON.
    InvalidArgsJson {
        event: String,
        error: serde_json::Error,
    },
}
impl std::error::Error for InputEventReadError {}
impl std::fmt::Display for InputEventReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InputEventReadError::InvalidEvent { id } => {
                write!(f, "Invalid event type ID: {id}")
            }
            InputEventReadError::InvalidArgs { id, args, error } => {
                write!(f, "Invalid event args for {id} with args {args}: {error}")
            }
            InputEventReadError::MissingFields { event } => {
                write!(f, "Missing fields for event: {event}")
            }
            InputEventReadError::InvalidTimestamp { event } => {
                write!(f, "Invalid timestamp for event: {event}")
            }
            InputEventReadError::InvalidArgsJson { event, error } => {
                write!(f, "Invalid args JSON for event: {event}: {error}")
            }
        }
    }
}

/// Quick Rundown on Event Datasets:
///
/// When stored as CSVs, each row has:
/// - timestamp [unix time]
/// - event type (see events.py) [str]
/// - event_args (see callback args) [list[any]]
#[derive(Debug, Clone, PartialEq)]
pub enum InputEventType {
    /// Start
    Start { inputs: input_capture::ActiveInput },
    /// End
    End { inputs: input_capture::ActiveInput },
    /// VIDEO_START
    VideoStart,
    /// VIDEO_END
    VideoEnd,
    /// MOUSE_MOVE: [dx : int, dy : int]
    MouseMove { dx: i32, dy: i32 },
    /// MOUSE_BUTTON: [button_idx : int, key_down : bool]
    MouseButton { button: u16, pressed: bool },
    /// SCROLL: [amt : int] (positive = up)
    Scroll { amount: i16 },
    /// KEYBOARD: [keycode : int, key_down : bool] (key down = true, key up = false)
    Keyboard { key: u16, pressed: bool },
    /// GAMEPAD_BUTTON: [button_idx : int, key_down : bool]
    GamepadButton { button: u16, pressed: bool },
    /// GAMEPAD_BUTTON_VALUE: [button_idx : int, value : float]
    GamepadButtonValue { button: u16, value: f32 },
    /// GAMEPAD_AXIS: [axis_idx : int, value : float]
    GamepadAxis { axis: u16, value: f32 },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SerializedStart {
    pub inputs: Inputs,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SerializedEnd {
    pub inputs: Inputs,
}
impl InputEventType {
    pub fn id(&self) -> &'static str {
        match self {
            InputEventType::Start { .. } => "START",
            InputEventType::End { .. } => "END",
            InputEventType::VideoStart => "VIDEO_START",
            InputEventType::VideoEnd => "VIDEO_END",
            InputEventType::MouseMove { .. } => "MOUSE_MOVE",
            InputEventType::MouseButton { .. } => "MOUSE_BUTTON",
            InputEventType::Scroll { .. } => "SCROLL",
            InputEventType::Keyboard { .. } => "KEYBOARD",
            InputEventType::GamepadButton { .. } => "GAMEPAD_BUTTON",
            InputEventType::GamepadButtonValue { .. } => "GAMEPAD_BUTTON_VALUE",
            InputEventType::GamepadAxis { .. } => "GAMEPAD_AXIS",
        }
    }

    pub fn json_args(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            InputEventType::Start { inputs } => serde_json::to_value(SerializedStart {
                inputs: Inputs::from(inputs.clone()),
            })
            .unwrap(),
            InputEventType::End { inputs } => serde_json::to_value(SerializedEnd {
                inputs: Inputs::from(inputs.clone()),
            })
            .unwrap(),
            InputEventType::VideoStart => json!([]),
            InputEventType::VideoEnd => json!([]),
            InputEventType::MouseMove { dx, dy } => json!([dx, dy]),
            InputEventType::MouseButton { button, pressed } => json!([button, pressed]),
            InputEventType::Scroll { amount } => json!([amount]),
            InputEventType::Keyboard { key, pressed } => json!([key, pressed]),
            InputEventType::GamepadButton { button, pressed } => json!([button, pressed]),
            InputEventType::GamepadButtonValue { button, value } => json!([button, value]),
            InputEventType::GamepadAxis { axis, value } => json!([axis, value]),
        }
    }

    pub fn from_input_event(event: input_capture::Event) -> Result<Self, InputEventReadError> {
        use input_capture::{Event, PressState};
        match event {
            Event::MouseMove([x, y]) => Ok(InputEventType::MouseMove { dx: x, dy: y }),
            Event::MousePress { key, press_state } => Ok(InputEventType::MouseButton {
                button: key,
                pressed: press_state == PressState::Pressed,
            }),
            Event::MouseScroll { scroll_amount } => Ok(InputEventType::Scroll {
                amount: scroll_amount,
            }),
            Event::KeyPress { key, press_state } => Ok(InputEventType::Keyboard {
                key,
                pressed: press_state == PressState::Pressed,
            }),
            Event::GamepadButtonPress { key, press_state } => Ok(InputEventType::GamepadButton {
                button: key,
                pressed: press_state == PressState::Pressed,
            }),
            Event::GamepadButtonChange { key, value } => {
                Ok(InputEventType::GamepadButtonValue { button: key, value })
            }
            Event::GamepadAxisChange { axis, value } => {
                Ok(InputEventType::GamepadAxis { axis, value })
            }
        }
    }

    pub fn from_id_and_json_args(
        id: &str,
        json_args: serde_json::Value,
    ) -> Result<Self, InputEventReadError> {
        fn parse_args<T: serde::de::DeserializeOwned>(
            id: &str,
            json_args: serde_json::Value,
        ) -> Result<T, InputEventReadError> {
            serde_json::from_value(json_args.clone()).map_err(|e| {
                InputEventReadError::InvalidArgs {
                    id: id.to_string(),
                    args: json_args,
                    error: e,
                }
            })
        }

        match id {
            "START" => Ok(InputEventType::Start {
                inputs: parse_args::<SerializedStart>(id, json_args)?.inputs.into(),
            }),
            "END" => Ok(InputEventType::End {
                inputs: parse_args::<SerializedEnd>(id, json_args)?.inputs.into(),
            }),
            "VIDEO_START" => Ok(InputEventType::VideoStart),
            "VIDEO_END" => Ok(InputEventType::VideoEnd),
            "MOUSE_MOVE" => {
                let args: (i32, i32) = parse_args(id, json_args)?;
                Ok(InputEventType::MouseMove {
                    dx: args.0,
                    dy: args.1,
                })
            }
            "MOUSE_BUTTON" => {
                let args: (u16, bool) = parse_args(id, json_args)?;
                Ok(InputEventType::MouseButton {
                    button: args.0,
                    pressed: args.1,
                })
            }
            "SCROLL" => {
                let args: (i16,) = parse_args(id, json_args)?;
                Ok(InputEventType::Scroll { amount: args.0 })
            }
            "KEYBOARD" => {
                let args: (u16, bool) = parse_args(id, json_args)?;
                Ok(InputEventType::Keyboard {
                    key: args.0,
                    pressed: args.1,
                })
            }
            "GAMEPAD_BUTTON" => {
                let args: (u16, bool) = parse_args(id, json_args)?;
                Ok(InputEventType::GamepadButton {
                    button: args.0,
                    pressed: args.1,
                })
            }
            "GAMEPAD_BUTTON_VALUE" => {
                let args: (u16, f32) = parse_args(id, json_args)?;
                Ok(InputEventType::GamepadButtonValue {
                    button: args.0,
                    value: args.1,
                })
            }
            "GAMEPAD_AXIS" => {
                let args: (u16, f32) = parse_args(id, json_args)?;
                Ok(InputEventType::GamepadAxis {
                    axis: args.0,
                    value: args.1,
                })
            }
            _ => Err(InputEventReadError::InvalidEvent { id: id.to_string() }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Inputs {
    pub keyboard: HashSet<u16>,
    pub mouse: HashSet<u16>,
    pub gamepad_digital: HashSet<u16>,
    pub gamepad_analog: HashMap<u16, f32>,
}
impl From<input_capture::ActiveInput> for Inputs {
    fn from(inputs: input_capture::ActiveInput) -> Self {
        Self {
            keyboard: inputs.keyboard,
            mouse: inputs.mouse,
            gamepad_digital: inputs.gamepad_digital,
            gamepad_analog: inputs.gamepad_analog,
        }
    }
}
impl From<Inputs> for input_capture::ActiveInput {
    fn from(event: Inputs) -> Self {
        Self {
            keyboard: event.keyboard,
            mouse: event.mouse,
            gamepad_digital: event.gamepad_digital,
            gamepad_analog: event.gamepad_analog,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InputEvent {
    pub timestamp: f64,
    pub event: InputEventType,
}
impl InputEvent {
    pub fn new(timestamp: f64, event: InputEventType) -> Self {
        Self { timestamp, event }
    }

    pub fn new_at_now(event: InputEventType) -> Self {
        Self::new(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64(),
            event,
        )
    }
}
impl std::fmt::Display for InputEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{},{},\"{}\"",
            self.timestamp,
            self.event.id(),
            self.event.json_args()
        )
    }
}
impl std::str::FromStr for InputEvent {
    type Err = InputEventReadError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        // Find the first comma
        let first_comma = s
            .find(',')
            .ok_or_else(|| InputEventReadError::MissingFields {
                event: s.to_string(),
            })?;

        // Find the second comma after the first one
        let second_comma = s[first_comma + 1..]
            .find(',')
            .map(|pos| first_comma + 1 + pos)
            .ok_or_else(|| InputEventReadError::MissingFields {
                event: s.to_string(),
            })?;

        // Extract the three fields
        let timestamp_str = &s[..first_comma];
        let event_type = &s[first_comma + 1..second_comma];
        let mut event_args = &s[second_comma + 1..];

        // Parse timestamp
        let timestamp =
            timestamp_str
                .parse::<f64>()
                .map_err(|_| InputEventReadError::InvalidTimestamp {
                    event: s.to_string(),
                })?;

        // Remove quotes from event_args if present
        if event_args.starts_with('"') && event_args.ends_with('"') {
            event_args = &event_args[1..event_args.len() - 1];
        }

        // Parse event_args as JSON
        let event_args =
            serde_json::from_str(event_args).map_err(|e| InputEventReadError::InvalidArgsJson {
                event: s.to_string(),
                error: e,
            })?;

        let event_type = InputEventType::from_id_and_json_args(event_type, event_args)?;
        Ok(InputEvent::new(timestamp, event_type))
    }
}
