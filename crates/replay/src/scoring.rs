/// The share of the reference's words that were heard wrongly, missed or
/// added, ignoring case and punctuation. 0.0 is a perfect match.
pub fn word_error_rate(reference: &str, heard: &str) -> f64 {
    let reference = words(reference);
    let heard = words(heard);
    word_edits(&reference, &heard) as f64 / reference.len() as f64
}

fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|word| {
            word.chars()
                .map(|c| if c == '\u{2019}' { '\'' } else { c })
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

/// The fewest substitutions, deletions and insertions that turn `reference`
/// into `heard`.
fn word_edits(reference: &[String], heard: &[String]) -> usize {
    let mut previous: Vec<usize> = (0..=heard.len()).collect();
    for (i, reference_word) in reference.iter().enumerate() {
        let mut current = vec![i + 1];
        for (j, heard_word) in heard.iter().enumerate() {
            let substitution = previous[j] + usize::from(reference_word != heard_word);
            current.push(substitution.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[heard.len()]
}
