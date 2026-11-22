//! What a recording session asks for, per language. The words live in
//! `scripts/`, one file per language, so they can be changed without code.

use buttervoice_core::language::Language;
use serde::Deserialize;

use crate::Kind;
use crate::session::Prompt;

#[derive(Deserialize)]
struct Entry {
    kind: String,
    say: Option<String>,
    #[serde(rename = "do")]
    instruction: Option<String>,
    conditions: Option<String>,
}

/// An entry this version does not understand is left out, so one bad line
/// costs a prompt and not the whole session.
pub fn script(language: Language) -> Vec<Prompt> {
    let written = match language {
        Language::English => include_str!("../scripts/en.json"),
        Language::BrazilianPortuguese => include_str!("../scripts/pt-BR.json"),
    };
    let entries: Vec<Entry> = serde_json::from_str(written).unwrap_or_default();
    entries
        .into_iter()
        .filter_map(|entry| {
            Some(Prompt {
                kind: Kind::ALL
                    .into_iter()
                    .find(|kind| kind.name() == entry.kind)?,
                say: entry.say,
                instruction: entry.instruction,
                conditions: entry.conditions,
            })
        })
        .collect()
}
