use buttervoice_core::language::Language;
use buttervoice_samples::Kind;
use buttervoice_samples::script::script;

#[test]
fn every_language_has_a_script_that_asks_for_every_kind_of_sample() {
    for language in Language::ALL {
        let prompts = script(language);
        for kind in Kind::ALL {
            assert!(
                prompts.iter().any(|prompt| prompt.kind == kind),
                "the {} script has no {kind:?} prompt",
                language.tag()
            );
        }
    }
}

#[test]
fn every_prompt_gives_the_person_something_to_go_on() {
    for language in Language::ALL {
        for prompt in script(language) {
            let words = prompt.say.as_deref().unwrap_or_default();
            let instruction = prompt.instruction.as_deref().unwrap_or_default();
            assert!(
                !words.trim().is_empty() || !instruction.trim().is_empty(),
                "an empty {:?} prompt in the {} script",
                prompt.kind,
                language.tag()
            );
        }
    }
}

#[test]
fn only_prompts_with_words_to_say_give_words_and_silence_never_does() {
    for language in Language::ALL {
        for prompt in script(language) {
            match prompt.kind {
                Kind::Silence | Kind::NaturalSpeech => assert_eq!(prompt.say, None),
                Kind::Conditions => {
                    assert!(prompt.say.is_some());
                    assert!(prompt.conditions.is_some());
                }
                _ => assert!(prompt.say.is_some()),
            }
        }
    }
}

#[test]
fn the_scripts_ask_for_the_same_things_in_the_same_order_in_every_language() {
    let shape = |language| -> Vec<(Kind, Option<String>)> {
        script(language)
            .into_iter()
            .map(|prompt| (prompt.kind, prompt.conditions))
            .collect()
    };

    assert_eq!(
        shape(Language::English),
        shape(Language::BrazilianPortuguese)
    );
    assert_eq!(shape(Language::English).len(), 24);
}

#[test]
fn the_two_languages_are_not_given_the_same_words() {
    let first_words = |language| script(language)[0].say.clone();

    assert_eq!(
        first_words(Language::English).as_deref(),
        Some("Can you send me the notes from this morning's meeting?")
    );
    assert_eq!(
        first_words(Language::BrazilianPortuguese).as_deref(),
        Some("Você pode me mandar as anotações da reunião de hoje de manhã?")
    );
}
