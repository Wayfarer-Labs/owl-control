use std::collections::HashSet;

use raw_input::{Event, PressState};

#[derive(Default)]
pub(crate) struct EventDebouncer {
    keyboard: KeyDebouncer,
    mouse_key: KeyDebouncer,
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
