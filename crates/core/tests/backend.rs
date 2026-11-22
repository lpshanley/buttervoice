use std::collections::HashSet;

use buttervoice_core::backend::{
    Backend, CATALOG, Model, Place, best_local_fit, find, local_backend_for,
};
use buttervoice_core::language::Language;

const A_MODEL: Model = Model {
    file: "made-up.gguf",
    source: "https://example.com/made-up.gguf",
    sha256: "0000000000000000000000000000000000000000000000000000000000000000",
    bytes: 1,
    licence: "MIT",
};

fn on_this_mac(id: &'static str, vetted_for: &'static [Language]) -> Backend {
    Backend {
        id,
        place: Place::ThisMac,
        model: Some(A_MODEL),
        speaks: &Language::ALL,
        vetted_for,
    }
}

fn in_the_cloud(id: &'static str, vetted_for: &'static [Language]) -> Backend {
    Backend {
        id,
        place: Place::Cloud,
        model: None,
        speaks: &Language::ALL,
        vetted_for,
    }
}

#[test]
fn the_best_local_fit_is_the_first_one_on_this_mac_vetted_for_the_language() {
    let catalog = [
        on_this_mac("english-only", &[Language::English]),
        on_this_mac("first-for-portuguese", &[Language::BrazilianPortuguese]),
        on_this_mac("second-for-portuguese", &[Language::BrazilianPortuguese]),
    ];

    let fit = best_local_fit(&catalog, Language::BrazilianPortuguese);

    assert_eq!(fit.map(|backend| backend.id), Some("first-for-portuguese"));
}

#[test]
fn a_cloud_service_is_never_the_local_fit_however_well_vetted() {
    let catalog = [
        in_the_cloud("cloud", &[Language::English]),
        on_this_mac("local", &[Language::English]),
    ];

    let fit = best_local_fit(&catalog, Language::English);

    assert_eq!(fit.map(|backend| backend.id), Some("local"));
}

#[test]
fn speaking_a_language_is_not_enough_to_be_recommended_for_it() {
    let catalog = [on_this_mac("speaks-both-vetted-for-neither", &[])];

    assert_eq!(best_local_fit(&catalog, Language::English), None);
}

#[test]
fn a_saved_choice_finds_its_backend_and_an_unknown_one_finds_none() {
    assert_eq!(
        find("grok").map(|backend| backend.place),
        Some(Place::Cloud)
    );
    assert_eq!(find("no-such-backend"), None);
}

#[test]
fn english_is_recommended_the_model_that_did_best_on_real_samples() {
    let fit = best_local_fit(&CATALOG, Language::English);

    assert_eq!(fit.map(|backend| backend.id), Some("parakeet-ultra"));
}

#[test]
fn the_english_only_model_stays_available_as_an_alternative() {
    let english_only = find("parakeet-tdt-0.6b-v2").expect("in the catalog");

    assert_eq!(english_only.place, Place::ThisMac);
    assert_eq!(english_only.speaks, &[Language::English]);
}

#[test]
fn nothing_local_is_recommended_for_portuguese_until_something_is_vetted() {
    assert_eq!(
        best_local_fit(&CATALOG, Language::BrazilianPortuguese),
        None
    );
}

#[test]
fn every_backend_in_the_catalog_has_its_own_id() {
    let ids: HashSet<&str> = CATALOG.iter().map(|backend| backend.id).collect();

    assert_eq!(ids.len(), CATALOG.len());
    assert!(!CATALOG.is_empty());
}

#[test]
fn a_backend_is_only_vetted_for_languages_it_speaks() {
    for backend in &CATALOG {
        for language in backend.vetted_for {
            assert!(backend.speaks.contains(language), "{}", backend.id);
        }
    }
}

#[test]
fn what_runs_on_this_mac_names_a_checkable_model_file_and_a_cloud_service_names_none() {
    for backend in &CATALOG {
        match (backend.place, backend.model) {
            (Place::ThisMac, Some(model)) => {
                assert!(model.source.starts_with("https://"), "{}", backend.id);
                assert!(model.source.ends_with(model.file), "{}", backend.id);
                assert_eq!(model.sha256.len(), 64, "{}", backend.id);
                assert!(
                    model.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                    "{}",
                    backend.id
                );
                assert!(model.bytes > 0, "{}", backend.id);
                assert!(!model.licence.is_empty(), "{}", backend.id);
            }
            (Place::Cloud, None) => {}
            _ => panic!("{} does not fit where it runs", backend.id),
        }
    }
}

#[test]
fn the_model_to_load_for_a_language_is_the_one_vetted_for_it() {
    let chosen = local_backend_for(&CATALOG, Language::English);

    assert_eq!(chosen.map(|backend| backend.id), Some("parakeet-ultra"));
}

#[test]
fn without_a_model_vetted_for_the_language_one_that_speaks_it_is_loaded() {
    let catalog = [
        Backend {
            speaks: &[Language::English],
            ..on_this_mac("english-only", &[Language::English])
        },
        on_this_mac("speaks-portuguese-untested", &[Language::English]),
    ];

    let chosen = local_backend_for(&catalog, Language::BrazilianPortuguese);

    assert_eq!(
        chosen.map(|backend| backend.id),
        Some("speaks-portuguese-untested")
    );
}

#[test]
fn when_nothing_on_this_mac_speaks_the_language_there_is_no_model_to_load() {
    let catalog = [
        Backend {
            speaks: &[Language::English],
            ..on_this_mac("english-only", &[Language::English])
        },
        in_the_cloud("cloud", &[Language::BrazilianPortuguese]),
    ];

    assert_eq!(
        local_backend_for(&catalog, Language::BrazilianPortuguese),
        None
    );
}
