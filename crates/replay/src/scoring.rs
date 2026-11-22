use buttervoice_core::language::Language;

/// The share of the reference's words that were heard wrongly, missed or
/// added, ignoring case and punctuation, for English. 0.0 is a perfect
/// match.
pub fn word_error_rate(reference: &str, heard: &str) -> f64 {
    let (errors, said) = word_errors(Language::English, reference, heard);
    errors as f64 / said as f64
}

/// How many words were heard wrongly, missed or added, and how many words
/// the reference has, with numbers read the way `language` writes them.
pub fn word_errors(language: Language, reference: &str, heard: &str) -> (usize, usize) {
    let reference = words(language, reference);
    let heard = words(language, heard);
    (word_edits(&reference, &heard), reference.len())
}

/// The words of `text`, lower-cased and without punctuation, with numbers
/// written one way however they were written. In English "nine thirty" and
/// "9:30" both become `9 30`; in Portuguese "nove e meia" and "9h30" both
/// become `9 e 30`, and "R$ 1.250" becomes `1250 reais`.
fn words(language: Language, text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    // "R$ 1.250" puts the currency before the amount, apart from it.
    let mut currency_owed = None;
    for raw in text.split_whitespace() {
        let raw = raw.replace('\u{2019}', "'").to_lowercase();
        let trimmed = raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '$' && c != '\'');
        let (amount, currency) = match currency_of(language, trimmed) {
            Some(("", currency)) => {
                currency_owed = Some(currency);
                continue;
            }
            Some((amount, currency)) => (amount, Some(currency)),
            None => (trimmed, currency_owed.take()),
        };
        tokens.extend(figures(language, amount));
        tokens.extend(currency.map(str::to_string));
    }
    tokens.extend(currency_owed.map(str::to_string));
    match language {
        Language::English => english_numbers_as_figures(&tokens),
        Language::BrazilianPortuguese => portuguese_numbers_as_figures(&tokens),
    }
}

/// The amount written after a currency sign, and the currency's name.
fn currency_of(language: Language, token: &str) -> Option<(&str, &'static str)> {
    match language {
        Language::English => token.strip_prefix('$').map(|amount| (amount, "dollars")),
        Language::BrazilianPortuguese => token.strip_prefix("r$").map(|amount| (amount, "reais")),
    }
}

fn digits(part: &str) -> bool {
    !part.is_empty() && part.chars().all(|c| c.is_ascii_digit())
}

/// Splits a token written with figures into the words it stands for.
fn figures(language: Language, token: &str) -> Vec<String> {
    let written = match language {
        Language::English => english_figures(token),
        Language::BrazilianPortuguese => portuguese_figures(token),
    };
    written.unwrap_or_else(|| {
        token
            .split('-')
            .map(|part| {
                part.chars()
                    .filter(|c| c.is_alphanumeric() || *c == '\'')
                    .collect::<String>()
            })
            .filter(|part| !part.is_empty())
            .collect()
    })
}

fn english_figures(token: &str) -> Option<Vec<String>> {
    let without_commas = token.replace(',', "");
    if token.contains(',') && digits(&without_commas) {
        return Some(vec![without_commas]);
    }
    if let Some((hours, minutes)) = token.split_once(':')
        && digits(hours)
        && digits(minutes)
    {
        return Some(vec![hours.to_string(), minutes.to_string()]);
    }
    if let Some((whole, fraction)) = token.split_once('.')
        && digits(whole)
        && digits(fraction)
    {
        return Some(vec![
            whole.to_string(),
            "point".to_string(),
            fraction.to_string(),
        ]);
    }
    ["st", "nd", "rd", "th"].iter().find_map(|suffix| {
        let number = token.strip_suffix(suffix)?;
        digits(number).then(|| vec![number.to_string()])
    })
}

/// Portuguese groups thousands with a dot and writes decimals with a comma,
/// though a version number still takes a dot.
fn portuguese_figures(token: &str) -> Option<Vec<String>> {
    let groups: Vec<&str> = token.split('.').collect();
    if groups.len() > 1
        && (1..=3).contains(&groups[0].len())
        && groups.iter().all(|group| digits(group))
        && groups[1..].iter().all(|group| group.len() == 3)
    {
        return Some(vec![groups.concat()]);
    }
    for (mark, said) in [(',', "vírgula"), ('.', "ponto")] {
        if let Some((whole, fraction)) = token.split_once(mark)
            && digits(whole)
            && digits(fraction)
        {
            return Some(vec![
                whole.to_string(),
                said.to_string(),
                fraction.to_string(),
            ]);
        }
    }
    // Times are said with "e": "9:15" is "nove e quinze", "9h" "nove horas".
    if let Some(hours) = token.strip_suffix('h')
        && digits(hours)
    {
        return Some(vec![hours.to_string(), "horas".to_string()]);
    }
    if let Some((hours, minutes)) = token.split_once(':').or_else(|| token.split_once('h'))
        && digits(hours)
        && let Ok(minutes) = minutes.parse::<u64>()
    {
        return Some(vec![
            hours.to_string(),
            "e".to_string(),
            minutes.to_string(),
        ]);
    }
    ["º", "ª"].iter().find_map(|suffix| {
        let number = token.strip_suffix(suffix)?;
        digits(number).then(|| vec![number.to_string()])
    })
}

#[derive(Clone, Copy, PartialEq)]
enum NumberWord {
    Unit(u64),
    Teen(u64),
    Tens(u64),
    /// English "hundred", which multiplies what came before it.
    Hundred,
    /// Portuguese "duzentos" and the like, each a value of its own.
    Hundreds(u64),
    Scale(u64),
}

/// A number being read word by word.
#[derive(Clone, Copy)]
struct Number {
    total: u64,
    current: u64,
    last: NumberWord,
}

impl Number {
    fn starting(word: NumberWord) -> Self {
        Self {
            total: 0,
            current: 0,
            last: word,
        }
        .and(word)
    }

    fn and(self, word: NumberWord) -> Self {
        let Self { total, current, .. } = self;
        let (total, current) = match word {
            NumberWord::Unit(n)
            | NumberWord::Teen(n)
            | NumberWord::Tens(n)
            | NumberWord::Hundreds(n) => (total, current + n),
            NumberWord::Hundred => (total, current.max(1) * 100),
            NumberWord::Scale(scale) => (total + current.max(1) * scale, 0),
        };
        Self {
            total,
            current,
            last: word,
        }
    }

    fn figures(self) -> String {
        (self.total + self.current).to_string()
    }
}

fn english_number_word(word: &str) -> Option<NumberWord> {
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
fn english_numbers_as_figures(tokens: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut number: Option<Number> = None;
    for token in tokens {
        let Some(word) = english_number_word(token) else {
            out.extend(number.take().map(Number::figures));
            out.push(token.clone());
            continue;
        };
        let carries_on = matches!(
            (number.map(|number| number.last), word),
            (Some(NumberWord::Tens(_)), NumberWord::Unit(_))
                | (
                    Some(NumberWord::Unit(_) | NumberWord::Teen(_) | NumberWord::Tens(_)),
                    NumberWord::Hundred | NumberWord::Scale(_),
                )
                | (
                    Some(NumberWord::Hundred),
                    NumberWord::Unit(_)
                        | NumberWord::Teen(_)
                        | NumberWord::Tens(_)
                        | NumberWord::Scale(_),
                )
                | (
                    Some(NumberWord::Scale(_)),
                    NumberWord::Unit(_) | NumberWord::Teen(_) | NumberWord::Tens(_),
                )
        );
        number = Some(match number {
            Some(number) if carries_on => number.and(word),
            _ => {
                out.extend(number.take().map(Number::figures));
                Number::starting(word)
            }
        });
    }
    out.extend(number.map(Number::figures));
    out
}

fn portuguese_number_word(word: &str) -> Option<NumberWord> {
    const TEENS: [&str; 10] = [
        "dez",
        "onze",
        "doze",
        "treze",
        "quatorze",
        "quinze",
        "dezesseis",
        "dezessete",
        "dezoito",
        "dezenove",
    ];
    const TENS: [&str; 8] = [
        "vinte",
        "trinta",
        "quarenta",
        "cinquenta",
        "sessenta",
        "setenta",
        "oitenta",
        "noventa",
    ];
    // Each hundred from two up, in its masculine form; the feminine swaps
    // the final "os" for "as".
    const HUNDREDS: [&str; 8] = [
        "duzentos",
        "trezentos",
        "quatrocentos",
        "quinhentos",
        "seiscentos",
        "setecentos",
        "oitocentos",
        "novecentos",
    ];
    let position = |list: &[&str]| list.iter().position(|known| *known == word);
    if let Some(n) = position(&TEENS) {
        return Some(NumberWord::Teen(10 + n as u64));
    }
    if let Some(n) = position(&TENS) {
        return Some(NumberWord::Tens(20 + 10 * n as u64));
    }
    let masculine = word.strip_suffix("as").map(|stem| format!("{stem}os"));
    if let Some(n) = HUNDREDS
        .iter()
        .position(|known| *known == word || Some(*known) == masculine.as_deref())
    {
        return Some(NumberWord::Hundreds(200 + 100 * n as u64));
    }
    match word {
        "zero" => Some(NumberWord::Unit(0)),
        // Dates use the ordinal only for the first: "primeiro de março" but
        // "dois de março".
        "um" | "uma" | "primeiro" | "primeira" => Some(NumberWord::Unit(1)),
        "dois" | "duas" => Some(NumberWord::Unit(2)),
        "três" => Some(NumberWord::Unit(3)),
        "quatro" => Some(NumberWord::Unit(4)),
        "cinco" => Some(NumberWord::Unit(5)),
        "seis" => Some(NumberWord::Unit(6)),
        "sete" => Some(NumberWord::Unit(7)),
        "oito" => Some(NumberWord::Unit(8)),
        "nove" => Some(NumberWord::Unit(9)),
        "catorze" => Some(NumberWord::Teen(14)),
        "cem" | "cento" => Some(NumberWord::Hundreds(100)),
        "mil" => Some(NumberWord::Scale(1_000)),
        "milhão" | "milhões" => Some(NumberWord::Scale(1_000_000)),
        _ => None,
    }
}

/// Turns each run of number words into figures. Portuguese joins the parts
/// of one number with "e", "duzentos e cinquenta", so an "e" that cannot
/// join two parts is the word "and" between two numbers: "um e dois", or
/// the hour and minutes of "nove e quinze". "e meia" after a number is half
/// past.
fn portuguese_numbers_as_figures(tokens: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut number: Option<Number> = None;
    let mut rest = tokens.iter().peekable();
    while let Some(token) = rest.next() {
        if let Some(word) = portuguese_number_word(token) {
            let joins = number.is_some_and(|number| {
                matches!(
                    (number.last, word),
                    (_, NumberWord::Scale(_)) | (NumberWord::Scale(_), NumberWord::Hundreds(_))
                )
            });
            number = Some(match number {
                Some(number) if joins => number.and(word),
                _ => {
                    out.extend(number.take().map(Number::figures));
                    Number::starting(word)
                }
            });
            continue;
        }
        if token == "e"
            && let Some(so_far) = number
            && let Some(next) = rest.peek()
        {
            if *next == "meia" {
                rest.next();
                out.push(so_far.figures());
                out.extend(["e".to_string(), "30".to_string()]);
                number = None;
                continue;
            }
            let joins_with_e = portuguese_number_word(next).filter(|word| {
                matches!(
                    (so_far.last, word),
                    (NumberWord::Tens(_), NumberWord::Unit(_))
                        | (
                            NumberWord::Hundreds(_),
                            NumberWord::Unit(_) | NumberWord::Teen(_) | NumberWord::Tens(_),
                        )
                        | (
                            NumberWord::Scale(_),
                            NumberWord::Unit(_)
                                | NumberWord::Teen(_)
                                | NumberWord::Tens(_)
                                | NumberWord::Hundreds(_),
                        )
                )
            });
            if let Some(word) = joins_with_e {
                rest.next();
                number = Some(so_far.and(word));
                continue;
            }
        }
        out.extend(number.take().map(Number::figures));
        out.push(token.clone());
    }
    out.extend(number.map(Number::figures));
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
