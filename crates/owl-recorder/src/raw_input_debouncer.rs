use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use constants::FPS;
use input_capture::{Event, PressState};

#[derive(Default)]
pub(crate) struct EventDebouncer {
    keyboard: KeyDebouncer,
    mouse_key: KeyDebouncer,
    gamepad_button: KeyDebouncer,
    gamepad_button_value: AnalogDebouncer,
    gamepad_axis: AnalogDebouncer,
}

impl EventDebouncer {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns true if the event should be processed, or false if it should be ignored.
    pub(crate) fn debounce(&mut self, e: Event) -> bool {
        match e {
            Event::MousePress { key, press_state } => self.mouse_key.debounce(key, press_state),
            Event::KeyPress { key, press_state } => self.keyboard.debounce(key, press_state),
            Event::GamepadButtonPress { key, press_state } => {
                self.gamepad_button.debounce(key, press_state)
            }
            Event::GamepadButtonChange { key, .. } => self.gamepad_button_value.debounce(key),
            Event::GamepadAxisChange { axis: key, .. } => self.gamepad_axis.debounce(key),
            Event::MouseMove(_) | Event::MouseScroll { .. } => true,
        }
    }
}

#[derive(Default)]
struct KeyDebouncer {
    pressed_keys: HashSet<u16>,
}
impl KeyDebouncer {
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns true if the key event should be processed, or false if it should be ignored.
    pub(crate) fn debounce(&mut self, key: u16, press_state: PressState) -> bool {
        match press_state {
            PressState::Pressed => self.pressed_keys.insert(key),
            PressState::Released => {
                self.pressed_keys.remove(&key);
                true
            }
        }
    }
}

#[derive(Default)]
struct AnalogDebouncer {
    last_change: HashMap<u16, std::time::Instant>,
}
impl AnalogDebouncer {
    /// Returns whether or not a sufficient amount of time has passed since the last change.
    pub(crate) fn debounce(&mut self, key: u16) -> bool {
        const MAX_ANALOGUE_SAMPLING_MICROSECONDS: u64 = (1_000_000.0 / (FPS as f32 * 2.0)) as u64;

        let now = std::time::Instant::now();
        let Some(last_change) = self.last_change.get(&key) else {
            self.last_change.insert(key, now);
            return true;
        };

        if now - *last_change > Duration::from_micros(MAX_ANALOGUE_SAMPLING_MICROSECONDS) {
            self.last_change.insert(key, now);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_debouncer() {
        use PressState::*;

        let mut debouncer = KeyDebouncer::new();

        assert!(debouncer.debounce(65, Pressed));
        assert!(!debouncer.debounce(65, Pressed));
        assert!(!debouncer.debounce(65, Pressed));
        assert!(debouncer.debounce(65, Released));
        assert!(debouncer.debounce(65, Pressed));
        assert!(!debouncer.debounce(65, Pressed));
    }
}
