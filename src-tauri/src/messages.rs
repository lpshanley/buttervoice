//! The text the Rust side shows a person: the menu bar item and the errors
//! that commands return. The windows get theirs from the same catalogs.

use buttervoice_core::language::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    MarkLastDictationAsBad,
    RecordSamples,
    OpenSettings,
    Quit,
    EnterAKeyFirst,
    ChoiceNotSaved,
    SamplesNotSaved,
    DownloadCancelled,
    DownloadInterrupted,
    DownloadDamaged,
    DownloadNotSaved,
    DownloadUnreachable,
}

impl Message {
    fn key(self) -> &'static str {
        match self {
            Message::MarkLastDictationAsBad => "tray.mark_bad",
            Message::RecordSamples => "tray.record_samples",
            Message::OpenSettings => "tray.settings",
            Message::Quit => "tray.quit",
            Message::EnterAKeyFirst => "error.enter_a_key",
            Message::ChoiceNotSaved => "error.choice_not_saved",
            Message::SamplesNotSaved => "error.samples_not_saved",
            Message::DownloadCancelled => "download.cancelled",
            Message::DownloadInterrupted => "download.interrupted",
            Message::DownloadDamaged => "download.damaged",
            Message::DownloadNotSaved => "download.not_saved",
            Message::DownloadUnreachable => "download.unreachable",
        }
    }

    pub fn text(self, language: Language) -> String {
        rust_i18n::t!(self.key(), locale = language.tag()).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_MESSAGE: [Message; 12] = [
        Message::DownloadCancelled,
        Message::DownloadInterrupted,
        Message::DownloadDamaged,
        Message::DownloadNotSaved,
        Message::DownloadUnreachable,
        Message::RecordSamples,
        Message::SamplesNotSaved,
        Message::MarkLastDictationAsBad,
        Message::OpenSettings,
        Message::Quit,
        Message::EnterAKeyFirst,
        Message::ChoiceNotSaved,
    ];

    #[test]
    fn every_message_has_text_in_every_language() {
        for language in Language::ALL {
            for message in EVERY_MESSAGE {
                let text = message.text(language);
                assert!(
                    !text.is_empty() && !text.contains(message.key()),
                    "{message:?} has no text in {}: got {text:?}",
                    language.tag()
                );
            }
        }
    }

    #[test]
    fn the_two_languages_do_not_share_their_text() {
        assert_eq!(Message::Quit.text(Language::English), "Quit ButterVoice");
        assert_eq!(
            Message::Quit.text(Language::BrazilianPortuguese),
            "Encerrar o ButterVoice"
        );
    }
}
