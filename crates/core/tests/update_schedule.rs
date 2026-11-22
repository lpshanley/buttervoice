use buttervoice_core::update_schedule::check_due;

const HOUR: u64 = 60 * 60;
const NOW: u64 = 1_800_000_000;

#[test]
fn with_the_switch_off_no_check_is_ever_due() {
    assert!(!check_due(false, None, NOW));
    assert!(!check_due(false, Some(NOW - 30 * 24 * HOUR), NOW));
}

#[test]
fn a_check_is_due_when_there_has_never_been_one() {
    assert!(check_due(true, None, NOW));
}

#[test]
fn a_check_is_due_a_day_after_the_last_one_and_not_before() {
    assert!(!check_due(true, Some(NOW - 23 * HOUR), NOW));
    assert!(check_due(true, Some(NOW - 24 * HOUR), NOW));
    assert!(check_due(true, Some(NOW - 30 * HOUR), NOW));
}

#[test]
fn a_last_check_dated_in_the_future_does_not_stop_checks_for_good() {
    assert!(check_due(true, Some(NOW + 48 * HOUR), NOW));
}
