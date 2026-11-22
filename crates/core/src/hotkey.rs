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

#[derive(Debug)]
pub struct HotkeyDecoder {
    key: HotkeyKey,
    presses_seen: u64,
    held: Option<PressId>,
}

impl HotkeyDecoder {
    pub fn new(key: HotkeyKey) -> Self {
        Self {
            key,
            presses_seen: 0,
            held: None,
        }
    }

    /// `flags` is the modifier-flags word of a macOS flags-changed event.
    pub fn flags_changed(&mut self, flags: u64) -> Option<KeyEdge> {
        let is_down = flags & self.key.device_flag() != 0;
        match (is_down, self.held) {
            (true, None) => {
                self.presses_seen += 1;
                let press = PressId(self.presses_seen);
                self.held = Some(press);
                Some(KeyEdge::Pressed(press))
            }
            (false, Some(press)) => {
                self.held = None;
                Some(KeyEdge::Released(press))
            }
            _ => None,
        }
    }
}
