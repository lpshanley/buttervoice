//! Native window titles, menu bar items, and errors shown by the webviews.

use buttervoice_core::language::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Message {
    MarkLastDictationAsBad,
    StopDictation,
    CopyLastDictation,
    RecordSamples,
    OpenSettings,
    Quit,
    SettingsTitle,
    SamplesTitle,
    EnterAKeyFirst,
    KeyNotSaved,
    KeyNotRemoved,
    ChoiceNotSaved,
    SamplesNotSaved,
    TakeNotSaved,
    DownloadCancelled,
    DownloadInterrupted,
    DownloadDamaged,
    DownloadNotSaved,
    DownloadUnreachable,
    UpdateCheckFailed,
    UpdateNotInstalled,
    InstallUpdate,
    PackageNotMade,
}

impl Message {
    fn key(self) -> &'static str {
        match self {
            Message::MarkLastDictationAsBad => "tray.mark_bad",
            Message::StopDictation => "tray.stop",
            Message::CopyLastDictation => "tray.copy_last",
            Message::RecordSamples => "tray.record_samples",
            Message::OpenSettings => "tray.settings",
            Message::Quit => "tray.quit",
            Message::SettingsTitle => "window.settings",
            Message::SamplesTitle => "window.samples",
            Message::EnterAKeyFirst => "error.enter_a_key",
            Message::KeyNotSaved => "error.key_not_saved",
            Message::KeyNotRemoved => "error.key_not_removed",
            Message::ChoiceNotSaved => "error.choice_not_saved",
            Message::SamplesNotSaved => "error.samples_not_saved",
            Message::TakeNotSaved => "error.take_not_saved",
            Message::DownloadCancelled => "download.cancelled",
            Message::DownloadInterrupted => "download.interrupted",
            Message::DownloadDamaged => "download.damaged",
            Message::DownloadNotSaved => "download.not_saved",
            Message::DownloadUnreachable => "download.unreachable",
            Message::UpdateCheckFailed => "update.check_failed",
            Message::UpdateNotInstalled => "update.not_installed",
            Message::InstallUpdate => "tray.install_update",
            Message::PackageNotMade => "error.package_not_made",
        }
    }

    pub fn text(self, language: Language) -> String {
        rust_i18n::t!(self.key(), locale = language.tag()).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY_MESSAGE: [Message; 23] = [
        Message::PackageNotMade,
        Message::InstallUpdate,
        Message::UpdateCheckFailed,
        Message::UpdateNotInstalled,
        Message::DownloadCancelled,
        Message::DownloadInterrupted,
        Message::DownloadDamaged,
        Message::DownloadNotSaved,
        Message::DownloadUnreachable,
        Message::RecordSamples,
        Message::SamplesNotSaved,
        Message::TakeNotSaved,
        Message::MarkLastDictationAsBad,
        Message::StopDictation,
        Message::CopyLastDictation,
        Message::OpenSettings,
        Message::Quit,
        Message::SettingsTitle,
        Message::SamplesTitle,
        Message::EnterAKeyFirst,
        Message::KeyNotSaved,
        Message::KeyNotRemoved,
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
