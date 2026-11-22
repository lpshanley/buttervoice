use std::cell::Cell;

use buttervoice_core::api_key::{ApiKey, KeySource, choose};

fn some(key: &str) -> Option<String> {
    Some(key.to_string())
}

fn api_key(key: &str, source: KeySource) -> Option<ApiKey> {
    Some(ApiKey {
        key: key.to_string(),
        source,
    })
}

#[test]
fn a_key_in_the_environment_is_used_ahead_of_the_saved_one() {
    assert_eq!(
        choose(some("from-env"), || some("saved")),
        api_key("from-env", KeySource::Environment)
    );
}

#[test]
fn the_saved_key_is_used_when_the_environment_has_none() {
    assert_eq!(
        choose(None, || some("saved")),
        api_key("saved", KeySource::Saved)
    );
}

#[test]
fn a_blank_key_counts_as_no_key() {
    assert_eq!(
        choose(some("   "), || some("saved")),
        api_key("saved", KeySource::Saved)
    );
    assert_eq!(choose(None, || some("")), None);
}

#[test]
fn space_around_a_pasted_key_is_dropped() {
    assert_eq!(
        choose(None, || some(" saved\n")),
        api_key("saved", KeySource::Saved)
    );
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
