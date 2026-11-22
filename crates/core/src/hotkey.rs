use crate::recording::PressId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyKey {
    RightOption,
}

impl HotkeyKey {
    /// The bit macOS sets for this physical key alone, unlike the shared
    /// "some Option key is down" bit that the key on the other side also sets.
    fn device_flag(self) -> u64 {
        match self {
            HotkeyKey::RightOption => 0x40,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEdge {
    Pressed(PressId),
    Released(PressId),
}

#[derive(Debug, Clone, Copy)]
enum KeyState {
    Up,
    Held(PressId),
    /// Down, but we never saw it go down, so it does not count as a press.
    HeldWithoutAPress,
}

#[derive(Debug)]
pub struct HotkeyDecoder {
    key: HotkeyKey,
    presses_seen: u64,
    state: KeyState,
}

impl HotkeyDecoder {
    pub fn new(key: HotkeyKey) -> Self {
        Self {
            key,
            presses_seen: 0,
            state: KeyState::Up,
        }
    }

    /// Brings the decoder back in line with `flags`, the modifier state read
    /// directly from macOS, after key events may have been missed.
    pub fn resync(&mut self, flags: u64) -> Option<KeyEdge> {
        match (self.is_down(flags), self.state) {
            (true, KeyState::Up) => {
                self.state = KeyState::HeldWithoutAPress;
                None
            }
            _ => self.flags_changed(flags),
        }
    }

    /// `flags` is the modifier-flags word of a macOS flags-changed event.
    pub fn flags_changed(&mut self, flags: u64) -> Option<KeyEdge> {
        match (self.is_down(flags), self.state) {
            (true, KeyState::Up) => {
                self.presses_seen += 1;
                let press = PressId(self.presses_seen);
                self.state = KeyState::Held(press);
                Some(KeyEdge::Pressed(press))
            }
            (false, KeyState::Held(press)) => {
                self.state = KeyState::Up;
                Some(KeyEdge::Released(press))
            }
            (false, KeyState::HeldWithoutAPress) => {
                self.state = KeyState::Up;
                None
            }
            _ => None,
        }
    }

    fn is_down(&self, flags: u64) -> bool {
        flags & self.key.device_flag() != 0
    }
}
