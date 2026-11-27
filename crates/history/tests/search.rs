use buttervoice_history::{App, Entry, History};

fn dictated(text: &str, at: u64) -> Entry {
    Entry {
        text: text.to_string(),
        at,
        language: Some("en".to_string()),
        backend: Some("parakeet-ultra".to_string()),
        app: Some(App {
            name: "Slack".to_string(),
            bundle_id: Some("com.tinyspeck.slackmacgap".to_string()),
        }),
        heard: None,
    }
}

/// Searches a freshly opened history, so what is found is what reached the
/// disk.
fn search_after(entries: &[Entry], query: &str) -> Vec<Entry> {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("history.jsonl");
    let history = History::open(file.clone());
    for entry in entries {
        history.add(entry).unwrap();
    }
    History::open(file).search(query, 50)
}

#[test]
fn a_dictation_is_found_by_a_word_in_it() {
    let notes = dictated(
        "Can you send me the notes from this morning?",
        1_700_000_000,
    );
    let found = search_after(&[notes.clone(), dictated("Yes", 1_700_000_060)], "notes");
    assert_eq!(found, [notes]);
}

#[test]
fn accents_and_capitals_do_not_have_to_be_typed() {
    let launch = dictated("Vamos lançar a versão amanhã.", 1_700_000_000);
    assert_eq!(
        search_after(std::slice::from_ref(&launch), "lancar versao"),
        std::slice::from_ref(&launch)
    );
    assert_eq!(
        search_after(std::slice::from_ref(&launch), "VAMOS"),
        [launch]
    );
}

#[test]
fn accents_typed_in_the_search_still_find_the_dictation() {
    let launch = dictated("Vamos lançar a versão amanhã.", 1_700_000_000);
    assert_eq!(
        search_after(std::slice::from_ref(&launch), "Lançar"),
        [launch]
    );
}

#[test]
fn every_word_searched_for_has_to_be_in_the_dictation() {
    let notes = dictated("Send me the notes from this morning", 1_700_000_000);
    let other = dictated("The notes are in the shared folder", 1_700_000_060);
    assert_eq!(
        search_after(&[notes.clone(), other], "morning notes"),
        [notes]
    );
}

#[test]
fn an_empty_search_lists_the_newest_dictations_first() {
    let folder = tempfile::tempdir().unwrap();
    let history = History::open(folder.path().join("history.jsonl"));
    for (text, at) in [("first", 1), ("second", 2), ("third", 3)] {
        history.add(&dictated(text, at)).unwrap();
    }

    let listed: Vec<_> = history
        .search("  ", 2)
        .into_iter()
        .map(|e| e.text)
        .collect();

    assert_eq!(listed, ["third", "second"]);
}

#[test]
fn clearing_forgets_every_dictation() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("history.jsonl");
    let history = History::open(file.clone());
    history.add(&dictated("Something private", 1)).unwrap();

    history.clear().unwrap();

    assert!(History::open(file.clone()).search("", 50).is_empty());
    assert!(!file.exists(), "the words are gone from the disk too");
}

#[test]
fn clearing_an_empty_history_is_not_an_error() {
    let folder = tempfile::tempdir().unwrap();
    History::open(folder.path().join("history.jsonl"))
        .clear()
        .unwrap();
}
