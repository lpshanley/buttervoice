//! Drops the sounds people make while thinking ("um", "hã") from dictated
//! text. Only sounds that are never words in the dictation language are
//! dropped, so "a", "é" and Portuguese "um" always stay.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::language::Language;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithoutFillers {
    pub text: String,
    /// How many thinking sounds were taken out.
    pub dropped: usize,
}

/// What was heard before thinking sounds were taken out of it, kept so a
/// word taken out by mistake can be traced to the filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedFillers {
    pub heard: String,
    pub count: usize,
}

pub fn without_fillers(text: &str, language: Language) -> WithoutFillers {
    let has_lower_case = text.chars().any(char::is_lowercase);
    let mut words = Words::split(text);
    let mut index = 0;
    let mut dropped = 0;
    while index < words.pieces.len() {
        let word = &words.pieces[index].word;
        let acronym = has_lower_case && !word.chars().any(char::is_lowercase);
        let asked_or_exclaimed = words.gap_after(index).trim_start().starts_with(['?', '!']);
        if !acronym && !asked_or_exclaimed && is_filler(word, language) {
            words.remove(index);
            dropped += 1;
        } else {
            index += 1;
        }
    }
    WithoutFillers {
        text: words.join(),
        dropped,
    }
}

/// A word and the spaces and punctuation that come before it.
struct Piece {
    gap: String,
    word: String,
}

struct Words {
    pieces: Vec<Piece>,
    tail: String,
}

impl Words {
    fn split(text: &str) -> Self {
        let mut pieces = Vec::new();
        let mut gap = String::new();
        let mut word = String::new();
        let mut characters = text.chars().peekable();
        while let Some(character) = characters.next() {
            let joins_a_name = character == '.'
                && !word.is_empty()
                && characters
                    .peek()
                    .is_some_and(|&next| is_word_character(next));
            if is_word_character(character) || joins_a_name {
                word.push(character);
            } else {
                if !word.is_empty() {
                    pieces.push(Piece {
                        gap: std::mem::take(&mut gap),
                        word: std::mem::take(&mut word),
                    });
                }
                gap.push(character);
            }
        }
        if !word.is_empty() {
            pieces.push(Piece {
                gap: std::mem::take(&mut gap),
                word,
            });
        }
        Self { pieces, tail: gap }
    }

    fn gap_after(&self, index: usize) -> &str {
        self.pieces
            .get(index + 1)
            .map_or(self.tail.as_str(), |next| next.gap.as_str())
    }

    fn remove(&mut self, index: usize) {
        let removed = self.pieces.remove(index);
        let opens_text = index == 0;
        let opens_sentence = opens_text || removed.gap.trim_end().ends_with(['.', '!', '?']);
        let ends_text = index == self.pieces.len();
        let after = match self.pieces.get_mut(index) {
            Some(next) => &mut next.gap,
            None => &mut self.tail,
        };
        let mut left = removed.gap.as_str();
        let mut right = after.as_str();
        if let Some(rest) = without_leading_pause(right) {
            right = rest;
            left = trailing_pause_dropped(left);
        } else if let Some(rest) = right.strip_prefix('.') {
            if opens_sentence {
                right = rest;
            } else {
                left = trailing_pause_dropped(left);
            }
        } else if ends_text && right.trim().is_empty() {
            left = trailing_pause_dropped(left);
        }
        let mut joined = spaced(left, right);
        if opens_text {
            joined = joined.trim_start().to_string();
        }
        if ends_text {
            joined = joined.trim_end().to_string();
        }
        *after = joined;
        if opens_sentence
            && starts_with_capital(&removed.word)
            && let Some(next) = self.pieces.get_mut(index)
        {
            next.word = capitalized(&next.word);
        }
    }

    fn join(self) -> String {
        let mut text = String::new();
        for piece in self.pieces {
            text.push_str(&piece.gap);
            text.push_str(&piece.word);
        }
        text.push_str(&self.tail);
        text
    }
}

/// Commas and ellipses mark where someone paused, which a dropped sound
/// usually filled.
const PAUSES: [&str; 3] = [",", "…", "..."];

fn without_leading_pause(gap: &str) -> Option<&str> {
    PAUSES.iter().find_map(|pause| gap.strip_prefix(pause))
}

/// A pause left with nothing to separate once the word after it is dropped.
fn trailing_pause_dropped(gap: &str) -> &str {
    let trimmed = gap.trim_end();
    PAUSES
        .iter()
        .find_map(|pause| trimmed.strip_suffix(pause))
        .unwrap_or(gap)
}

/// Joins what came before and after a dropped word, leaving one space where
/// either side had any.
fn spaced(left: &str, right: &str) -> String {
    let trimmed_left = left.trim_end();
    let trimmed_right = right.trim_start();
    let space = if trimmed_left.len() < left.len() || trimmed_right.len() < right.len() {
        " "
    } else {
        ""
    };
    format!("{trimmed_left}{space}{trimmed_right}")
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric()
        || is_combining_mark(character)
        || matches!(character, '_' | '/' | '\'' | '’' | '-')
}

fn is_filler(word: &str, language: Language) -> bool {
    // An accent can arrive as its own mark after the letter: "ha\u{303}" is "hã".
    let runs = letter_runs(&word.nfc().collect::<String>().to_lowercase());
    match language {
        Language::English => matches!(
            runs.as_slice(),
            [('u', _), ('m', _)]
                | [('u', _), ('h', _)]
                | [('u', _), ('h', _), ('m', _)]
                | [('e', 1), ('r', 1)]
                | [('e', _), ('r', _), ('m', _)]
                | [('h', _), ('m', _)]
                | [('m', 3..)]
        ),
        Language::BrazilianPortuguese => matches!(
            runs.as_slice(),
            [('h', _), ('ã', _)]
                | [('ã', _), ('h', _)]
                | [('h', _), ('u', _), ('m', _)]
                | [('h', _), ('m', _)]
                | [('m', 3..)]
                | [('u', _), ('h', _)]
                | [('u', _), ('h', _), ('m', _)]
                | [('a' | 'ã', _), ('h', _), ('n', _)]
                | [('é', 2..)]
        ),
    }
}

/// Each letter with how many times it repeats in a row: "ummm" is u×1, m×3.
fn letter_runs(word: &str) -> Vec<(char, usize)> {
    let mut runs: Vec<(char, usize)> = Vec::new();
    for letter in word.chars() {
        match runs.last_mut() {
            Some((last, count)) if *last == letter => *count += 1,
            _ => runs.push((letter, 1)),
        }
    }
    runs
}

fn starts_with_capital(word: &str) -> bool {
    word.chars().next().is_some_and(char::is_uppercase)
}

fn capitalized(word: &str) -> String {
    let mut characters = word.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}
