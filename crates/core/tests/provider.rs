use buttervoice_core::transcription::Provider;

#[test]
fn in_local_mode_nothing_of_the_cloud_service_is_set_up() {
    let chosen = Provider::Local.set_up(
        || "the local engine",
        || -> &str { panic!("the cloud service was set up in local mode") },
    );

    assert_eq!(chosen, "the local engine");
}

#[test]
fn in_grok_mode_the_local_engine_is_not_loaded() {
    let chosen = Provider::Grok.set_up(
        || -> &str { panic!("the local engine was loaded in Grok mode") },
        || "the Grok client",
    );

    assert_eq!(chosen, "the Grok client");
}

#[test]
fn each_provider_is_saved_under_its_own_name() {
    assert_eq!(Provider::Local.name(), "local");
    assert_eq!(Provider::Grok.name(), "grok");
}

#[test]
fn a_saved_choice_is_read_back_as_that_provider() {
    assert_eq!(Provider::from_setting(Some("local")), Provider::Local);
    assert_eq!(Provider::from_setting(Some("grok")), Provider::Grok);
}

#[test]
fn with_no_saved_choice_dictation_stays_on_this_machine() {
    assert_eq!(Provider::from_setting(None), Provider::Local);
}

#[test]
fn an_unrecognised_saved_choice_stays_on_this_machine() {
    assert_eq!(
        Provider::from_setting(Some("somewhere-else")),
        Provider::Local
    );
}
