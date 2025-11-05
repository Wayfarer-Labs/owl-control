use std::collections::{HashMap, HashSet};

use input_capture::GamepadId;
use serde::{Deserialize, Serialize};

use crate::{system::hardware_specs, validation::InputStats};

#[derive(Serialize, Deserialize, Clone, Debug)]
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub gamepads: HashMap<GamepadId, GamepadMetadata>,
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
    /// START: very beginning of recording
    Start { inputs: input_capture::ActiveInput },
    /// END: very end of recording
    End { inputs: input_capture::ActiveInput },
    /// VIDEO_START: beginning of video recording (e.g. if the video were to be played at this point, it would line up with the event stream)
    VideoStart,
    /// VIDEO_END: end of video recording
    VideoEnd,
    /// HOOK_START: when the application was successfully hooked (e.g. when the recording is non-black)
    HookStart,
    /// UNFOCUS
    #[deprecated(since = "1.1.0", note = "Removed; don't use")]
    Unfocus,
    /// FOCUS
    #[deprecated(since = "1.1.0", note = "Removed; don't use")]
    Focus,
    /// MOUSE_MOVE: [dx : int, dy : int]
    MouseMove { dx: i32, dy: i32 },
    /// MOUSE_BUTTON: [button_idx : int, key_down : bool]
    MouseButton { button: u16, pressed: bool },
    /// SCROLL: [amt : int] (positive = up)
    Scroll { amount: i16 },
    /// KEYBOARD: [keycode : int, key_down : bool] (key down = true, key up = false)
    Keyboard { key: u16, pressed: bool },
    /// GAMEPAD_BUTTON: [button_idx : int, key_down : bool]
    GamepadButton {
        button: u16,
        pressed: bool,
        id: Option<GamepadId>,
    },
    /// GAMEPAD_BUTTON_VALUE: [button_idx : int, value : float]
    GamepadButtonValue {
        button: u16,
        value: f32,
        id: Option<GamepadId>,
    },
    /// GAMEPAD_AXIS: [axis_idx : int, value : float]
    GamepadAxis {
        axis: u16,
        value: f32,
        id: Option<GamepadId>,
    },
    /// UNKNOWN: [unknown : any] - used for backwards compatibility, never outputted
    Unknown,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SerializedStart {
    pub inputs: Inputs,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SerializedEnd {
    pub inputs: Inputs,
}
impl InputEventType {
    pub fn id(&self) -> &'static str {
        #[allow(deprecated)]
        match self {
            InputEventType::Start { .. } => "START",
            InputEventType::End { .. } => "END",
            InputEventType::VideoStart => "VIDEO_START",
            InputEventType::VideoEnd => "VIDEO_END",
            InputEventType::HookStart => "HOOK_START",
            InputEventType::Unfocus => "UNFOCUS",
            InputEventType::Focus => "FOCUS",
            InputEventType::MouseMove { .. } => "MOUSE_MOVE",
            InputEventType::MouseButton { .. } => "MOUSE_BUTTON",
            InputEventType::Scroll { .. } => "SCROLL",
            InputEventType::Keyboard { .. } => "KEYBOARD",
            InputEventType::GamepadButton { .. } => "GAMEPAD_BUTTON",
            InputEventType::GamepadButtonValue { .. } => "GAMEPAD_BUTTON_VALUE",
            InputEventType::GamepadAxis { .. } => "GAMEPAD_AXIS",
            InputEventType::Unknown => "UNKNOWN",
        }
    }

    pub fn json_args(&self) -> serde_json::Value {
        use serde_json::json;
        #[allow(deprecated)]
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
            InputEventType::HookStart => json!([]),
            InputEventType::Unfocus => json!([]),
            InputEventType::Focus => json!([]),
            InputEventType::MouseMove { dx, dy } => json!([dx, dy]),
            InputEventType::MouseButton { button, pressed } => json!([button, pressed]),
            InputEventType::Scroll { amount } => json!([amount]),
            InputEventType::Keyboard { key, pressed } => json!([key, pressed]),
            InputEventType::GamepadButton {
                button,
                pressed,
                id,
            } => {
                if let Some(id) = *id {
                    json!([button, pressed, id])
                } else {
                    json!([button, pressed])
                }
            }
            InputEventType::GamepadButtonValue { button, value, id } => {
                if let Some(id) = *id {
                    json!([button, value, id])
                } else {
                    json!([button, value])
                }
            }
            InputEventType::GamepadAxis { axis, value, id } => {
                if let Some(id) = *id {
                    json!([axis, value, id])
                } else {
                    json!([axis, value])
                }
            }
            InputEventType::Unknown => json!([]),
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
            Event::GamepadButtonPress {
                key,
                press_state,
                id,
            } => Ok(InputEventType::GamepadButton {
                button: key,
                pressed: press_state == PressState::Pressed,
                id: Some(id),
            }),
            Event::GamepadButtonChange { key, value, id } => {
                Ok(InputEventType::GamepadButtonValue {
                    button: key,
                    value,
                    id: Some(id),
                })
            }
            Event::GamepadAxisChange { axis, value, id } => Ok(InputEventType::GamepadAxis {
                axis,
                value,
                id: Some(id),
            }),
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

        fn parse_pair_with_optional_third<
            T1: serde::de::DeserializeOwned,
            T2: serde::de::DeserializeOwned,
            T3: serde::de::DeserializeOwned,
        >(
            id: &str,
            json_args: serde_json::Value,
        ) -> Result<(T1, T2, Option<T3>), InputEventReadError> {
            parse_args::<(T1, T2, T3)>(id, json_args.clone())
                .map(|t| (t.0, t.1, Some(t.2)))
                .or_else(|_| parse_args::<(T1, T2)>(id, json_args).map(|t| (t.0, t.1, None)))
        }

        #[allow(deprecated)]
        match id {
            // If we can't parse the args, just return a default start or end event
            // Compatibility with older recordings
            "START" => Ok(InputEventType::Start {
                inputs: parse_args::<SerializedStart>(id, json_args)
                    .map(|s| s.inputs.into())
                    .unwrap_or_default(),
            }),
            "END" => Ok(InputEventType::End {
                inputs: parse_args::<SerializedEnd>(id, json_args)
                    .map(|s| s.inputs.into())
                    .unwrap_or_default(),
            }),
            "VIDEO_START" => Ok(InputEventType::VideoStart),
            "VIDEO_END" => Ok(InputEventType::VideoEnd),
            "HOOK_START" => Ok(InputEventType::HookStart),
            "UNFOCUS" => Ok(InputEventType::Unfocus),
            "FOCUS" => Ok(InputEventType::Focus),
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
                let args: (u16, bool, Option<GamepadId>) =
                    parse_pair_with_optional_third(id, json_args)?;
                Ok(InputEventType::GamepadButton {
                    button: args.0,
                    pressed: args.1,
                    id: args.2,
                })
            }
            "GAMEPAD_BUTTON_VALUE" => {
                let args: (u16, f32, Option<GamepadId>) =
                    parse_pair_with_optional_third(id, json_args)?;
                Ok(InputEventType::GamepadButtonValue {
                    button: args.0,
                    value: args.1,
                    id: args.2,
                })
            }
            "GAMEPAD_AXIS" => {
                let args: (u16, f32, Option<GamepadId>) =
                    parse_pair_with_optional_third(id, json_args)?;
                Ok(InputEventType::GamepadAxis {
                    axis: args.0,
                    value: args.1,
                    id: args.2,
                })
            }
            other => {
                tracing::warn!("Unknown event type: {other}, remapping to UNKNOWN");
                Ok(InputEventType::Unknown)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GamepadInputs {
    pub digital: HashSet<u16>,
    pub analog: HashMap<u16, f32>,
    pub axis: HashMap<u16, f32>,
}
impl From<input_capture::ActiveGamepad> for GamepadInputs {
    fn from(gamepad: input_capture::ActiveGamepad) -> Self {
        Self {
            digital: gamepad.digital,
            analog: gamepad.analog,
            axis: gamepad.axis,
        }
    }
}
impl From<GamepadInputs> for input_capture::ActiveGamepad {
    fn from(inputs: GamepadInputs) -> Self {
        Self {
            digital: inputs.digital,
            analog: inputs.analog,
            axis: inputs.axis,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct GamepadMetadata {
    pub name: String,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
}
impl From<input_capture::GamepadMetadata> for GamepadMetadata {
    fn from(metadata: input_capture::GamepadMetadata) -> Self {
        Self {
            name: metadata.name,
            vendor_id: metadata.vendor_id,
            product_id: metadata.product_id,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Inputs {
    pub keyboard: HashSet<u16>,
    pub mouse: HashSet<u16>,
    #[serde(default, skip_serializing)]
    #[deprecated]
    gamepad_digital: HashSet<u16>,
    #[serde(default, skip_serializing)]
    #[deprecated]
    gamepad_analog: HashMap<u16, f32>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub gamepads: HashMap<GamepadId, GamepadInputs>,
}
impl From<input_capture::ActiveInput> for Inputs {
    fn from(inputs: input_capture::ActiveInput) -> Self {
        #[allow(deprecated)]
        Self {
            keyboard: inputs.keyboard,
            mouse: inputs.mouse,
            gamepad_digital: inputs
                .gamepads
                .values()
                .flat_map(|gamepad| gamepad.digital.iter())
                .copied()
                .collect(),
            gamepad_analog: inputs
                .gamepads
                .values()
                .flat_map(|gamepad| gamepad.analog.iter())
                .map(|(key, value)| (*key, *value))
                .collect(),
            gamepads: inputs
                .gamepads
                .into_iter()
                .map(|(id, gamepad)| (id, gamepad.into()))
                .collect(),
        }
    }
}
impl From<Inputs> for input_capture::ActiveInput {
    fn from(event: Inputs) -> Self {
        Self {
            keyboard: event.keyboard,
            mouse: event.mouse,
            gamepads: event
                .gamepads
                .into_iter()
                .map(|(id, gamepad)| (id, gamepad.into()))
                .collect(),
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
