//! Filters whisper hallucinations from transcripts.
//!
//! Whisper models hallucinate on silent or near-silent audio: bracketed
//! non-speech annotations ("[BLANK_AUDIO]", "[Music]"), YouTube-style
//! outros ("Thank you for watching"), and subtitle credits. This module
//! strips annotations unconditionally and drops whole-transcript
//! hallucination phrases when token confidence indicates the model was
//! guessing.

use std::sync::LazyLock;

use regex::Regex;

/// Average token probability below which known hallucination phrases are
/// dropped. Deliberate dictations of the same phrases score well above this.
const PHRASE_DROP_MAX_AVG_PROB: f32 = 0.55;

/// Segment no-speech probability above which known hallucination phrases
/// are dropped. This is the primary hallucination signal: whisper emits
/// hallucinated text with HIGH token probabilities, but the segment's
/// no-speech probability stays high because the audio really was silence.
/// Real speech scores well under 0.1 here.
const PHRASE_DROP_MIN_NO_SPEECH_PROB: f32 = 0.4;

/// Non-speech annotations whisper emits for silence/noise, e.g.
/// "[BLANK_AUDIO]", "(silence)", "*music*". Keyword-scoped so legitimate
/// bracketed dictation is left alone.
static ANNOTATION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[\[(*]\s*(?:blank[_ ]?audio|silence|silent|music|applause|laughter|laughs|laughing|noise|inaudible|unintelligible|static|typing|cough(?:ing)?|breath(?:ing)?|sigh(?:s|ing)?|bell|beep|chime|click(?:s|ing)?|hum(?:ming)?|whoosh|sound(?:s)? of[^\])*]*)\s*[\])*]",
    )
    .expect("annotation regex must compile")
});

static MUSIC_NOTES_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[♪♫♬]+").expect("music note regex must compile"));

/// Phrases whisper produces for silent audio that are never plausible
/// dictations, dropped regardless of token confidence.
const ALWAYS_DROP_PREFIXES: &[&str] = &[
    "subtitles by",
    "subtitled by",
    "transcribed by",
    "translated by",
    "captions by",
    "captioning by",
];

/// Common silence hallucinations that could also be deliberate dictations;
/// dropped only when token confidence indicates guessing.
const CONFIDENCE_GATED_PHRASES: &[&str] = &[
    "thank you",
    "thank you very much",
    "thank you so much",
    "thanks for watching",
    "thank you for watching",
    "thank you so much for watching",
    "thanks for watching and see you in the next video",
    "see you in the next video",
    "please subscribe",
    "don't forget to subscribe",
    "you",
    "bye",
    "bye bye",
    "so",
    "the end",
];

#[derive(Debug, Clone, PartialEq)]
pub struct FilterOutcome {
    pub text: String,
    pub annotations_stripped: bool,
    pub phrase_dropped: bool,
}

/// Strip non-speech annotations and drop whole-transcript hallucination
/// phrases. `avg_token_prob` (mean whisper token probability) and
/// `no_speech_prob` (max per-segment no-speech probability) come from the
/// local backend; when both are `None` (remote backends) only the
/// unconditional rules apply, so a deliberate "Thank you." is never dropped.
pub fn filter_hallucinations(
    text: &str,
    avg_token_prob: Option<f32>,
    no_speech_prob: Option<f32>,
) -> FilterOutcome {
    let without_annotations = MUSIC_NOTES_RE.replace_all(text, " ");
    let without_annotations = ANNOTATION_RE.replace_all(&without_annotations, " ");
    let cleaned = collapse_whitespace(&without_annotations);
    let annotations_stripped = cleaned != text.trim();

    if cleaned.is_empty() {
        return FilterOutcome {
            text: cleaned,
            annotations_stripped,
            phrase_dropped: false,
        };
    }

    if is_hallucinated_phrase(&cleaned, avg_token_prob, no_speech_prob) {
        return FilterOutcome {
            text: String::new(),
            annotations_stripped,
            phrase_dropped: true,
        };
    }

    FilterOutcome {
        text: cleaned,
        annotations_stripped,
        phrase_dropped: false,
    }
}

fn is_hallucinated_phrase(
    cleaned: &str,
    avg_token_prob: Option<f32>,
    no_speech_prob: Option<f32>,
) -> bool {
    let normalized = normalize_phrase(cleaned);
    if normalized.is_empty() {
        return true;
    }

    if ALWAYS_DROP_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
        || normalized.contains("amara.org")
        || normalized.starts_with("www.")
    {
        return true;
    }

    let looks_like_silence = no_speech_prob
        .is_some_and(|prob| prob > PHRASE_DROP_MIN_NO_SPEECH_PROB)
        || avg_token_prob.is_some_and(|prob| prob < PHRASE_DROP_MAX_AVG_PROB);
    looks_like_silence
        && CONFIDENCE_GATED_PHRASES
            .iter()
            .any(|phrase| normalized == *phrase)
}

/// Lowercase and strip surrounding quotes plus trailing punctuation so
/// "Thank you." and "thank you" compare equal.
fn normalize_phrase(text: &str) -> String {
    text.trim()
        .trim_matches(|c: char| matches!(c, '"' | '\u{201c}' | '\u{201d}' | '\''))
        .trim_end_matches(['.', '!', ',', '…'])
        .trim()
        .to_lowercase()
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(text: &str, prob: Option<f32>) -> FilterOutcome {
        filter_hallucinations(text, prob, None)
    }

    #[test]
    fn strips_blank_audio_annotation() {
        let outcome = filter("[BLANK_AUDIO]", Some(0.9));
        assert_eq!(outcome.text, "");
        assert!(outcome.annotations_stripped);
    }

    #[test]
    fn strips_annotation_variants() {
        for input in [
            "(silence)",
            "*silence*",
            "[ Silence ]",
            "♪♪",
            "[blank audio]",
        ] {
            let outcome = filter(input, Some(0.9));
            assert_eq!(outcome.text, "", "expected {input:?} to be stripped");
        }
    }

    #[test]
    fn strips_annotation_inside_text() {
        let outcome = filter("hello [Music] world", Some(0.9));
        assert_eq!(outcome.text, "hello world");
        assert!(outcome.annotations_stripped);
    }

    #[test]
    fn drops_thank_you_at_low_confidence() {
        let outcome = filter("Thank you.", Some(0.3));
        assert_eq!(outcome.text, "");
        assert!(outcome.phrase_dropped);
    }

    #[test]
    fn keeps_thank_you_at_high_confidence() {
        let outcome = filter("Thank you.", Some(0.9));
        assert_eq!(outcome.text, "Thank you.");
        assert!(!outcome.phrase_dropped);
    }

    #[test]
    fn keeps_thank_you_without_confidence_data() {
        let outcome = filter("Thank you.", None);
        assert_eq!(outcome.text, "Thank you.");
        assert!(!outcome.phrase_dropped);
    }

    #[test]
    fn drops_lone_you_at_low_confidence() {
        let outcome = filter("you", Some(0.2));
        assert_eq!(outcome.text, "");
        assert!(outcome.phrase_dropped);
    }

    #[test]
    fn drops_subtitle_credits_regardless_of_confidence() {
        let outcome = filter("Subtitles by the Amara.org community", Some(0.95));
        assert_eq!(outcome.text, "");
        assert!(outcome.phrase_dropped);
    }

    #[test]
    fn legitimate_text_passes_through() {
        let text = "Schedule the meeting for Thursday at 3pm.";
        let outcome = filter(text, Some(0.4));
        assert_eq!(outcome.text, text);
        assert!(!outcome.annotations_stripped);
        assert!(!outcome.phrase_dropped);
    }

    #[test]
    fn legitimate_brackets_are_kept() {
        let text = "the array index [0] is out of bounds";
        let outcome = filter(text, Some(0.9));
        assert_eq!(outcome.text, text);
    }

    #[test]
    fn longer_sentence_containing_thank_you_is_kept() {
        let text = "thank you for sending the report over";
        let outcome = filter(text, Some(0.2));
        assert_eq!(outcome.text, text);
        assert!(!outcome.phrase_dropped);
    }

    // ── no_speech_prob gate ──

    #[test]
    fn drops_thank_you_with_high_no_speech_prob_despite_confident_tokens() {
        // The hallucination signature: confident tokens on a segment
        // whisper believed was silence.
        let outcome = filter_hallucinations("Thank you.", Some(0.95), Some(0.8));
        assert_eq!(outcome.text, "");
        assert!(outcome.phrase_dropped);
    }

    #[test]
    fn keeps_thank_you_with_low_no_speech_prob() {
        let outcome = filter_hallucinations("Thank you.", Some(0.95), Some(0.05));
        assert_eq!(outcome.text, "Thank you.");
        assert!(!outcome.phrase_dropped);
    }

    #[test]
    fn high_no_speech_prob_does_not_drop_arbitrary_text() {
        let text = "move the deploy to friday";
        let outcome = filter_hallucinations(text, Some(0.9), Some(0.9));
        assert_eq!(outcome.text, text);
        assert!(!outcome.phrase_dropped);
    }
}
