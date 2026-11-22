use std::cell::Cell;

use buttervoice_core::api_key::choose;

fn some(key: &str) -> Option<String> {
    Some(key.to_string())
}

#[test]
fn a_key_in_the_environment_is_used_ahead_of_the_saved_one() {
    assert_eq!(choose(some("from-env"), || some("saved")), some("from-env"));
}

#[test]
fn the_saved_key_is_used_when_the_environment_has_none() {
    assert_eq!(choose(None, || some("saved")), some("saved"));
}

#[test]
fn a_blank_key_counts_as_no_key() {
    assert_eq!(choose(some("   "), || some("saved")), some("saved"));
    assert_eq!(choose(None, || some("")), None);
}

#[test]
fn space_around_a_pasted_key_is_dropped() {
    assert_eq!(choose(None, || some(" saved\n")), some("saved"));
}

#[test]
fn the_saved_key_is_not_looked_up_when_the_environment_has_one() {
    let looked_up = Cell::new(false);

    choose(some("from-env"), || {
        looked_up.set(true);
        None
    });

    assert!(
        !looked_up.get(),
        "looking it up can raise a Keychain prompt"
    );
}
