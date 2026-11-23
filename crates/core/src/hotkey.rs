use crate::recording::PressId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyKey {
    RightOption,
    LeftOption,
    RightCommand,
    RightControl,
    LeftControl,
    Fn,
}

impl HotkeyKey {
    pub const ALL: [HotkeyKey; 6] = [
        HotkeyKey::RightOption,
        HotkeyKey::LeftOption,
        HotkeyKey::RightCommand,
        HotkeyKey::RightControl,
        HotkeyKey::LeftControl,
        HotkeyKey::Fn,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HotkeyKey::RightOption => "right_option",
            HotkeyKey::LeftOption => "left_option",
            HotkeyKey::RightCommand => "right_command",
            HotkeyKey::RightControl => "right_control",
            HotkeyKey::LeftControl => "left_control",
            HotkeyKey::Fn => "fn",
        }
    }

    /// The key a saved choice stands for. With no choice, or one this
    /// version does not know, it is Right Option.
    pub fn from_setting(saved: Option<&str>) -> HotkeyKey {
        Self::ALL
            .into_iter()
            .find(|key| Some(key.name()) == saved)
            .unwrap_or(HotkeyKey::RightOption)
    }

    /// The bit macOS sets for this physical key alone, unlike the shared
    /// "some Option key is down" bit that the key on the other side also sets.
    fn device_flag(self) -> u64 {
        match self {
            HotkeyKey::RightOption => 0x40,
            HotkeyKey::LeftOption => 0x20,
            HotkeyKey::RightCommand => 0x10,
            HotkeyKey::RightControl => 0x2000,
            HotkeyKey::LeftControl => 0x1,
            // There is only one Fn key, so its shared bit is its own.
            HotkeyKey::Fn => 0x80_0000,
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

    /// Listens for `key` from now on. Press ids keep counting, so a press of
    /// the new key is never mistaken for one of the old.
    pub fn set_key(&mut self, key: HotkeyKey) {
        self.key = key;
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
