use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    time::Duration,
};

use constants::FPS;
use input_capture::{Event, GamepadId, PressState};

#[derive(Default)]
pub(crate) struct EventDebouncer {
    keyboard: KeyDebouncer<u16>,
    mouse_key: KeyDebouncer<u16>,
    gamepad_button: KeyDebouncer<(GamepadId, u16)>,
    gamepad_button_value: AnalogDebouncer<(GamepadId, u16)>,
    gamepad_axis: AnalogDebouncer<(GamepadId, u16)>,
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
            Event::GamepadButtonPress {
                key,
                press_state,
                id,
            } => self.gamepad_button.debounce((id, key), press_state),
            Event::GamepadButtonChange { key, id, .. } => {
                self.gamepad_button_value.debounce((id, key))
            }
            Event::GamepadAxisChange { axis: key, id, .. } => self.gamepad_axis.debounce((id, key)),
            Event::MouseMove(_) | Event::MouseScroll { .. } => true,
        }
    }
}

struct KeyDebouncer<K: Eq + Hash> {
    pressed_keys: HashSet<K>,
}

impl<K: Eq + Hash> Default for KeyDebouncer<K> {
    fn default() -> Self {
        Self {
            pressed_keys: Default::default(),
        }
    }
}
impl<K: Eq + Hash> KeyDebouncer<K> {
    #[allow(dead_code)]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns true if the key event should be processed, or false if it should be ignored.
    pub(crate) fn debounce(&mut self, key: K, press_state: PressState) -> bool {
        match press_state {
            PressState::Pressed => self.pressed_keys.insert(key),
            PressState::Released => {
                self.pressed_keys.remove(&key);
                true
            }
        }
    }
}

struct AnalogDebouncer<K: Eq + Hash> {
    last_change: HashMap<K, std::time::Instant>,
}
impl<K: Eq + Hash> Default for AnalogDebouncer<K> {
    fn default() -> Self {
        Self {
            last_change: Default::default(),
        }
    }
}
impl<K: Eq + Hash> AnalogDebouncer<K> {
    /// Returns whether or not a sufficient amount of time has passed since the last change.
    pub(crate) fn debounce(&mut self, key: K) -> bool {
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
