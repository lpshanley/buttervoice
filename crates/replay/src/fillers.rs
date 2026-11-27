//! Runs what the app would type through the same filler filter, so a suite
//! shows both what the filter fixes and any word it takes that was said.

use buttervoice_core::fillers::without_fillers;

use crate::summary::Scored;

#[derive(Debug, Clone, PartialEq)]
pub struct FillerCheck {
    /// The same samples with thinking sounds taken out of what was heard.
    pub after: Vec<Scored>,
    pub dropped: usize,
    pub wrongly_dropped: Vec<WronglyDropped>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WronglyDropped {
    pub said: String,
    pub word: String,
}

pub fn check_fillers(scored: &[Scored]) -> FillerCheck {
    let mut dropped = 0;
    let mut wrongly_dropped = Vec::new();
    let after = scored
        .iter()
        .map(|sample| {
            let mut after = sample.clone();
            if let Ok(text) = &sample.heard {
                let cleaned = without_fillers(text, sample.language);
                dropped += cleaned.dropped;
                for word in said_but_taken(&sample.said, text, &cleaned.text) {
                    wrongly_dropped.push(WronglyDropped {
                        said: sample.said.clone(),
                        word,
                    });
                }
                after.heard = Ok(cleaned.text);
            }
            after
        })
        .collect();
    FillerCheck {
        after,
        dropped,
        wrongly_dropped,
    }
}

/// Words the filter took out of `before` that `said` has more of than
/// `after` still does.
fn said_but_taken(said: &str, before: &str, after: &str) -> Vec<String> {
    let said = words(said);
    let mut left = words(after);
    let mut taken = Vec::new();
    for word in words(before) {
        match left.iter().position(|kept| *kept == word) {
            Some(found) => {
                left.remove(found);
            }
            None => taken.push(word),
        }
    }
    let after = words(after);
    let count = |list: &[String], word: &str| list.iter().filter(|each| *each == word).count();
    taken.dedup();
    taken
        .into_iter()
        .filter(|word| count(&said, word) > count(&after, word))
        .collect()
}

/// Lower-cased, without the punctuation around each word.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.trim_matches(|character: char| !character.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|word| !word.is_empty())
        .collect()
}
