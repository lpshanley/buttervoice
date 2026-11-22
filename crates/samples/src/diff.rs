//! Where a transcript differs from what was said, word by word, so a person
//! can see at a glance what a model got wrong.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Part {
    Same(String),
    /// Said, but not in the transcript.
    Missing(String),
    /// In the transcript, but not said.
    Extra(String),
    Changed {
        said: String,
        heard: String,
    },
}

/// Case and punctuation are ignored when comparing; the parts keep the words
/// as they were written.
pub fn word_diff(said: &str, heard: &str) -> Vec<Part> {
    let said: Vec<&str> = said.split_whitespace().collect();
    let heard: Vec<&str> = heard.split_whitespace().collect();
    let key = |word: &str| -> String {
        word.chars()
            .map(|c| if c == '\u{2019}' { '\'' } else { c })
            .filter(|c| c.is_alphanumeric() || *c == '\'')
            .flat_map(char::to_lowercase)
            .collect()
    };
    let said_keys: Vec<String> = said.iter().map(|word| key(word)).collect();
    let heard_keys: Vec<String> = heard.iter().map(|word| key(word)).collect();

    // edits[i][j]: the fewest edits turning the first i words said into the
    // first j words heard.
    let mut edits = vec![vec![0usize; heard.len() + 1]; said.len() + 1];
    for (i, row) in edits.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in edits[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=said.len() {
        for j in 1..=heard.len() {
            let substitution = usize::from(said_keys[i - 1] != heard_keys[j - 1]);
            edits[i][j] = (edits[i - 1][j - 1] + substitution)
                .min(edits[i - 1][j] + 1)
                .min(edits[i][j - 1] + 1);
        }
    }

    let (mut i, mut j) = (said.len(), heard.len());
    let mut parts = Vec::new();
    while i > 0 || j > 0 {
        let matches = i > 0 && j > 0 && said_keys[i - 1] == heard_keys[j - 1];
        if i > 0 && j > 0 && edits[i][j] == edits[i - 1][j - 1] + usize::from(!matches) {
            parts.push(if matches {
                Part::Same(said[i - 1].to_string())
            } else {
                Part::Changed {
                    said: said[i - 1].to_string(),
                    heard: heard[j - 1].to_string(),
                }
            });
            i -= 1;
            j -= 1;
        } else if i > 0 && edits[i][j] == edits[i - 1][j] + 1 {
            parts.push(Part::Missing(said[i - 1].to_string()));
            i -= 1;
        } else {
            parts.push(Part::Extra(heard[j - 1].to_string()));
            j -= 1;
        }
    }
    parts.reverse();
    parts
}
