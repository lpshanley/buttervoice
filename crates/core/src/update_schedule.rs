//! When the app checks for a newer release by itself.

const DAY: u64 = 24 * 60 * 60;

/// Whether a check is due now. Times are seconds since 1970.
///
/// A last check dated after `now` means the clock was moved back; waiting
/// for the clock to catch up could mean never checking, so one is due.
pub fn check_due(enabled: bool, last_checked: Option<u64>, now: u64) -> bool {
    enabled
        && match last_checked {
            None => true,
            Some(last) if last > now => true,
            Some(last) => now - last >= DAY,
        }
}
