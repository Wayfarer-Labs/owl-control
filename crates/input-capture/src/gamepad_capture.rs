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

pub fn initialize_thread(input_tx: mpsc::Sender<Event>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut gilrs = gilrs::Gilrs::new().unwrap();
        let mut active_gamepad = None;

        // Examine new events
        while let Some(gilrs::Event { id, event, .. }) = gilrs.next_event_blocking(None) {
            if active_gamepad.is_none() {
                active_gamepad = Some(id);
            }

            let Some(event) = map_event(event) else {
                continue;
            };
            if input_tx.blocking_send(event).is_err() {
                tracing::warn!("Gamepad input tx closed, stopping gamepad capture");
                break;
            }
        }
    })
}

fn map_event(event: gilrs::EventType) -> Option<Event> {
    use gilrs::EventType;
    match event {
        EventType::ButtonPressed(button, _) => Some(Event::GamepadButtonPress {
            key: map_button(button),
            press_state: PressState::Pressed,
        }),
        EventType::ButtonReleased(button, _) => Some(Event::GamepadButtonPress {
            key: map_button(button),
            press_state: PressState::Released,
        }),
        EventType::ButtonChanged(button, value, _) => Some(Event::GamepadButtonChange {
            key: map_button(button),
            value,
        }),
        EventType::AxisChanged(axis, value, _) => Some(Event::GamepadAxisChange {
            axis: map_axis(axis),
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

fn map_button(button: gilrs::Button) -> u16 {
    use gilrs::Button;
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

fn map_axis(axis: gilrs::Axis) -> u16 {
    use gilrs::Axis;
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
