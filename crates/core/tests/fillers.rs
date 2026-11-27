use buttervoice_core::fillers::without_fillers;
use buttervoice_core::language::Language;

fn english(text: &str) -> String {
    without_fillers(text, Language::English).text
}

#[test]
fn a_filler_that_opens_a_sentence_goes_and_the_next_word_takes_its_capital() {
    assert_eq!(
        english("Um, so I think we should ship."),
        "So I think we should ship."
    );
}

#[test]
fn a_filler_set_off_by_commas_mid_sentence_takes_both_commas_with_it() {
    assert_eq!(
        english("We should, uh, ship it on Friday."),
        "We should ship it on Friday."
    );
}

#[test]
fn every_english_thinking_sound_goes_however_long_it_is_drawn_out() {
    for sound in [
        "um", "ummm", "uh", "uhhh", "uhm", "er", "erm", "errrm", "hm", "hmmm", "mmm",
    ] {
        assert_eq!(english(&format!("ship {sound} it")), "ship it", "{sound}");
    }
}

#[test]
fn words_that_only_contain_or_resemble_a_thinking_sound_stay() {
    for text in [
        "I need a hammer.",
        "A good plan is a plan.",
        "Ah, I see what you mean.",
        "Uh-huh, sounds good.",
        "Mm-hmm, yes.",
        "Umberto ate hummus.",
        "Rename the umm_count field",
        "I like it, so well done.",
    ] {
        assert_eq!(english(text), text);
    }
}

#[test]
fn a_file_name_that_starts_like_a_thinking_sound_stays() {
    assert_eq!(english("Open um.txt now."), "Open um.txt now.");
}

#[test]
fn a_sound_written_in_capitals_among_lower_case_words_is_taken_as_an_acronym() {
    assert_eq!(english("Take him to the ER."), "Take him to the ER.");
    assert_eq!(english("The UM team met today."), "The UM team met today.");
}

#[test]
fn text_written_all_in_capitals_still_loses_its_thinking_sounds() {
    assert_eq!(english("UM, SO I THINK"), "SO I THINK");
}

#[test]
fn a_sound_asked_or_exclaimed_is_kept_because_it_means_something() {
    assert_eq!(english("Hmm? What did you say?"), "Hmm? What did you say?");
    assert_eq!(english("Uh! That hurt."), "Uh! That hurt.");
}

#[test]
fn a_sound_that_trails_off_a_sentence_leaves_the_full_stop_and_takes_its_comma() {
    assert_eq!(english("Let's go, uh."), "Let's go.");
    assert_eq!(
        english("We should, um. Then we ship."),
        "We should. Then we ship."
    );
}

#[test]
fn a_sound_that_is_a_whole_sentence_goes_with_its_full_stop() {
    assert_eq!(english("Um. Okay, ship it."), "Okay, ship it.");
    assert_eq!(english("Okay. Hmm. Ship it."), "Okay. Ship it.");
    assert_eq!(english("Um."), "");
}

#[test]
fn a_sound_between_two_pauses_leaves_a_single_space() {
    assert_eq!(english("Ship it… uh… tomorrow."), "Ship it tomorrow.");
    assert_eq!(english("Ship it... uh... tomorrow."), "Ship it tomorrow.");
}

#[test]
fn a_sound_that_ends_the_text_takes_the_comma_before_it() {
    assert_eq!(english("Let's go, uh"), "Let's go");
}

#[test]
fn only_a_capital_the_sound_had_is_handed_on() {
    assert_eq!(english("Okay. Um, so we ship."), "Okay. So we ship.");
    assert_eq!(english("um, so i think"), "so i think");
}

#[test]
fn sounds_in_a_row_all_go() {
    assert_eq!(english("Um, uh, so we ship."), "So we ship.");
}

fn portuguese(text: &str) -> String {
    without_fillers(text, Language::BrazilianPortuguese).text
}

#[test]
fn every_portuguese_thinking_sound_goes_however_long_it_is_drawn_out() {
    for sound in [
        "hã", "hããã", "ãh", "hum", "hm", "hmmm", "mmm", "uh", "uhm", "ahn", "ãhn", "éé", "éééé",
    ] {
        assert_eq!(
            portuguese(&format!("vou {sound} enviar")),
            "vou enviar",
            "{sound}"
        );
    }
}

#[test]
fn portuguese_words_that_carry_meaning_stay() {
    for text in [
        "Eu tenho um carro.",
        "Um dia eu vou.",
        "É importante e é para amanhã.",
        "É, pode ser.",
        "Vou a São Paulo e à praia.",
        "Ah, entendi.",
        "Está bom, né?",
        "Então, tipo, vamos lançar.",
        "Qual é o tipo de arquivo?",
        "Humberto chegou com o humor dele.",
    ] {
        assert_eq!(portuguese(text), text);
    }
}

#[test]
fn a_sound_written_with_a_separate_accent_mark_goes_like_any_other() {
    assert_eq!(portuguese("Ha\u{303}, beleza."), "Beleza.");
}

#[test]
fn each_sound_taken_out_is_counted() {
    assert_eq!(
        without_fillers("Um, uh, so we ship. Hmm? Yes.", Language::English).dropped,
        2
    );
}
