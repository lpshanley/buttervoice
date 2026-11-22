/// The share of the reference's words that were heard wrongly, missed or
/// added, ignoring case and punctuation. 0.0 is a perfect match.
pub fn word_error_rate(reference: &str, heard: &str) -> f64 {
    let (errors, said) = word_errors(reference, heard);
    errors as f64 / said as f64
}

/// How many words were heard wrongly, missed or added, and how many words
/// the reference has.
pub fn word_errors(reference: &str, heard: &str) -> (usize, usize) {
    let reference = words(reference);
    let heard = words(heard);
    (word_edits(&reference, &heard), reference.len())
}

/// The words of `text`, lower-cased and without punctuation, with numbers
/// written one way however they were written: "nine thirty" and "9:30"
/// both become `9 30`, "$1,250" and "one thousand two hundred fifty
/// dollars" both become `1250 dollars`. English number words only.
fn words(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    for raw in text.split_whitespace() {
        let raw = raw.replace('\u{2019}', "'").to_lowercase();
        let trimmed = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '$' && c != '\'');
        tokens.extend(figures(trimmed));
    }
    spell_numbers_as_figures(&tokens)
}

/// Splits a token written with figures into the words it stands for.
fn figures(token: &str) -> Vec<String> {
    let digits = |part: &str| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
    if let Some(amount) = token.strip_prefix('$') {
        let mut out = figures(amount);
        out.push("dollars".to_string());
        return out;
    }
    let without_commas = token.replace(',', "");
    if token.contains(',') && digits(&without_commas) {
        return vec![without_commas];
    }
    if let Some((hours, minutes)) = token.split_once(':')
        && digits(hours)
        && digits(minutes)
    {
        return vec![hours.to_string(), minutes.to_string()];
    }
    if let Some((whole, fraction)) = token.split_once('.')
        && digits(whole)
        && digits(fraction)
    {
        return vec![whole.to_string(), "point".to_string(), fraction.to_string()];
    }
    for suffix in ["st", "nd", "rd", "th"] {
        if let Some(number) = token.strip_suffix(suffix)
            && digits(number)
        {
            return vec![number.to_string()];
        }
    }
    token
        .split('-')
        .map(|part| {
            part.chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .collect::<String>()
        })
        .filter(|part| !part.is_empty())
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum NumberWord {
    Unit(u64),
    Teen(u64),
    Tens(u64),
    Hundred,
    Scale(u64),
}

fn number_word(word: &str) -> Option<NumberWord> {
    const UNITS: [&str; 10] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ];
    const UNIT_ORDINALS: [&str; 10] = [
        "zeroth", "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth",
        "ninth",
    ];
    const TEENS: [&str; 10] = [
        "ten",
        "eleven",
        "twelve",
        "thirteen",
        "fourteen",
        "fifteen",
        "sixteen",
        "seventeen",
        "eighteen",
        "nineteen",
    ];
    const TEEN_ORDINALS: [&str; 10] = [
        "tenth",
        "eleventh",
        "twelfth",
        "thirteenth",
        "fourteenth",
        "fifteenth",
        "sixteenth",
        "seventeenth",
        "eighteenth",
        "nineteenth",
    ];
    const TENS: [&str; 8] = [
        "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
    ];
    const TENS_ORDINALS: [&str; 8] = [
        "twentieth",
        "thirtieth",
        "fortieth",
        "fiftieth",
        "sixtieth",
        "seventieth",
        "eightieth",
        "ninetieth",
    ];
    let position = |list: &[&str]| list.iter().position(|known| *known == word);
    if let Some(n) = position(&UNITS).or_else(|| position(&UNIT_ORDINALS)) {
        return Some(NumberWord::Unit(n as u64));
    }
    if let Some(n) = position(&TEENS).or_else(|| position(&TEEN_ORDINALS)) {
        return Some(NumberWord::Teen(10 + n as u64));
    }
    if let Some(n) = position(&TENS).or_else(|| position(&TENS_ORDINALS)) {
        return Some(NumberWord::Tens(20 + 10 * n as u64));
    }
    match word {
        "hundred" | "hundredth" => Some(NumberWord::Hundred),
        "thousand" | "thousandth" => Some(NumberWord::Scale(1_000)),
        "million" | "millionth" => Some(NumberWord::Scale(1_000_000)),
        _ => None,
    }
}

/// Turns each run of number words into figures. A run ends where English
/// would start a new number, so "nine thirty" is two numbers and "twenty
/// one" is one.
fn spell_numbers_as_figures(tokens: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut number: Option<(u64, u64, NumberWord)> = None;
    let flush = |number: &mut Option<(u64, u64, NumberWord)>, out: &mut Vec<String>| {
        if let Some((total, current, _)) = number.take() {
            out.push((total + current).to_string());
        }
    };
    for token in tokens {
        let Some(word) = number_word(token) else {
            flush(&mut number, &mut out);
            out.push(token.clone());
            continue;
        };
        let carries_on = match (number.map(|(_, _, last)| last), word) {
            (None, _) => false,
            (Some(NumberWord::Tens(_)), NumberWord::Unit(_)) => true,
            (
                Some(NumberWord::Unit(_) | NumberWord::Teen(_) | NumberWord::Tens(_)),
                NumberWord::Hundred | NumberWord::Scale(_),
            ) => true,
            (
                Some(NumberWord::Hundred),
                NumberWord::Unit(_)
                | NumberWord::Teen(_)
                | NumberWord::Tens(_)
                | NumberWord::Scale(_),
            ) => true,
            (
                Some(NumberWord::Scale(_)),
                NumberWord::Unit(_) | NumberWord::Teen(_) | NumberWord::Tens(_),
            ) => true,
            _ => false,
        };
        if !carries_on {
            flush(&mut number, &mut out);
        }
        let (total, current, _) = number.unwrap_or((0, 0, word));
        number = Some(match word {
            NumberWord::Unit(n) | NumberWord::Teen(n) | NumberWord::Tens(n) => {
                (total, current + n, word)
            }
            NumberWord::Hundred => (total, current.max(1) * 100, word),
            NumberWord::Scale(scale) => (total + current.max(1) * scale, 0, word),
        });
    }
    flush(&mut number, &mut out);
    out
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
