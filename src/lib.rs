//! Core linting logic for phone number formatting.
//!
//! Every public function in this module is pure: given the same input it
//! always produces the same output, and none of them touch a file, a
//! clock, or any other outside state. `main.rs` is the only place that
//! does I/O. Keeping the split this strict means every rule below can be
//! unit tested with a plain string in, a `Vec<Finding>` out.

/// A local number needs at least this many digits to be worth flagging.
/// Below this, a digit run is more likely a date, a price, or a version
/// number than a phone number.
pub const MIN_PHONE_DIGITS: usize = 7;

/// E.164 caps international numbers at 15 digits, so anything past that
/// is definitely not a phone number.
pub const MAX_PHONE_DIGITS: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub column: usize,
    pub severity: Severity,
    pub message: String,
}

/// A span of text that looks enough like a phone number to be checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub column: usize,
    pub text: String,
}

/// Characters allowed inside a phone-number-shaped run of text, besides
/// digits and a leading '+'.
const SEPARATORS: [char; 5] = ['-', '.', ' ', '(', ')'];

/// Scan a single line for substrings that look like phone numbers.
///
/// A run qualifies if it consists only of digits, the separator
/// characters above, and an optional leading '+', and it contains at
/// least [`MIN_PHONE_DIGITS`] digits.
pub fn extract_candidates(line: &str) -> Vec<Candidate> {
    let chars: Vec<char> = line.chars().collect();
    let mut candidates = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let is_start = chars[i] == '+' || chars[i].is_ascii_digit();
        if !is_start {
            i += 1;
            continue;
        }

        let start = i;
        let mut j = i;
        let mut digits = 0usize;
        // '+' only counts as a run opener, never a run character on its own.
        if chars[j] == '+' {
            j += 1;
        }
        while j < chars.len() {
            let c = chars[j];
            if c.is_ascii_digit() {
                digits += 1;
                j += 1;
            } else if SEPARATORS.contains(&c) {
                j += 1;
            } else {
                break;
            }
        }

        if digits >= MIN_PHONE_DIGITS {
            let raw: String = chars[start..j].iter().collect();
            let trimmed = raw.trim_end_matches(|c: char| SEPARATORS.contains(&c));
            candidates.push(Candidate {
                column: start + 1, // 1-based, matches how editors report columns
                text: trimmed.to_string(),
            });
        }
        i = j.max(i + 1);
    }
    candidates
}

fn digit_count(text: &str) -> usize {
    text.chars().filter(|c| c.is_ascii_digit()).count()
}

/// Flag numbers that mix more than one separator convention, e.g.
/// "555-123.4567". Real numbers stick to one convention; a mix is
/// almost always a copy-paste or find-and-replace slip.
pub fn check_mixed_separators(candidate: &str) -> Option<String> {
    let mut seen: Vec<char> = Vec::new();
    for c in candidate.chars() {
        if matches!(c, '-' | '.' | ' ') && !seen.contains(&c) {
            seen.push(c);
        }
    }
    if seen.len() > 1 {
        Some(format!(
            "mixed separators {:?} in phone number \"{}\"",
            seen, candidate
        ))
    } else {
        None
    }
}

/// Flag digit counts outside what a real phone number can have.
pub fn check_digit_count(candidate: &str) -> Option<String> {
    let count = digit_count(candidate);
    if count > MAX_PHONE_DIGITS {
        Some(format!(
            "\"{}\" has {} digits, more than the E.164 maximum of {}",
            candidate, count, MAX_PHONE_DIGITS
        ))
    } else {
        None
    }
}

/// Flag an area-code paren that was opened but never closed, or vice
/// versa, e.g. "(555 123-4567".
pub fn check_balanced_parens(candidate: &str) -> Option<String> {
    let open = candidate.chars().filter(|&c| c == '(').count();
    let close = candidate.chars().filter(|&c| c == ')').count();
    if open != close {
        Some(format!(
            "unbalanced parentheses in phone number \"{}\"",
            candidate
        ))
    } else {
        None
    }
}

/// Flag a 10-digit North American number whose separators don't fall on
/// the standard 3-3-4 boundaries (area code, exchange, subscriber), e.g.
/// "55-512-34567" instead of "555-123-4567". A leading "1" country code
/// group is set aside first, since "+1-555-123-4567" is still 3-3-4
/// underneath. A single unbroken run of 10 digits isn't claiming any
/// grouping at all, so it's left alone.
pub fn check_nanp_grouping(candidate: &str) -> Option<String> {
    let mut groups: Vec<&str> = candidate
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .collect();

    if groups.len() < 2 {
        return None;
    }

    if groups.len() == 4 && groups[0] == "1" {
        groups.remove(0);
    }

    let lengths: Vec<usize> = groups.iter().map(|g| g.len()).collect();
    let total_digits: usize = lengths.iter().sum();
    if total_digits != 10 {
        return None;
    }

    if lengths != [3, 3, 4] {
        Some(format!(
            "\"{}\" groups digits as {:?}, not the NANP 3-3-4 pattern",
            candidate, lengths
        ))
    } else {
        None
    }
}

type Check = fn(&str) -> Option<String>;

const CHECKS: [(Check, Severity); 4] = [
    (check_mixed_separators, Severity::Warning),
    (check_digit_count, Severity::Error),
    (check_balanced_parens, Severity::Error),
    (check_nanp_grouping, Severity::Warning),
];

/// Run every rule against one candidate, tagging results with the line
/// they came from.
pub fn lint_candidate(line: usize, candidate: &Candidate) -> Vec<Finding> {
    CHECKS
        .iter()
        .filter_map(|(check, severity)| {
            check(&candidate.text).map(|message| Finding {
                line,
                column: candidate.column,
                severity: *severity,
                message,
            })
        })
        .collect()
}

/// Extract and lint every candidate on one line.
pub fn lint_line(line_number: usize, line: &str) -> Vec<Finding> {
    extract_candidates(line)
        .iter()
        .flat_map(|c| lint_candidate(line_number, c))
        .collect()
}

/// Lint a whole file's worth of text, one line at a time. Line numbers
/// in the results are 1-based.
pub fn lint_text(text: &str) -> Vec<Finding> {
    text.lines()
        .enumerate()
        .flat_map(|(i, line)| lint_line(i + 1, line))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_short_digit_runs() {
        assert!(extract_candidates("order #12345 shipped").is_empty());
    }

    #[test]
    fn finds_a_plain_number() {
        let found = extract_candidates("call 555-123-4567 today");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "555-123-4567");
        assert_eq!(found[0].column, 6);
    }

    #[test]
    fn finds_an_international_number() {
        let found = extract_candidates("reach us at +14155552671.");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].text, "+14155552671");
    }

    #[test]
    fn trims_trailing_punctuation() {
        let found = extract_candidates("phone: 555-123-4567.");
        assert_eq!(found[0].text, "555-123-4567");
    }

    #[test]
    fn flags_mixed_separators() {
        assert!(check_mixed_separators("555-123.4567").is_some());
        assert!(check_mixed_separators("555-123-4567").is_none());
    }

    #[test]
    fn flags_too_many_digits() {
        assert!(check_digit_count("1234567890123456").is_some());
        assert!(check_digit_count("+14155552671").is_none());
    }

    #[test]
    fn flags_unbalanced_parens() {
        assert!(check_balanced_parens("(555 123-4567").is_some());
        assert!(check_balanced_parens("(555) 123-4567").is_none());
    }

    #[test]
    fn flags_bad_nanp_grouping() {
        assert!(check_nanp_grouping("55-512-34567").is_some());
        assert!(check_nanp_grouping("555-123-4567").is_none());
    }

    #[test]
    fn nanp_grouping_allows_leading_country_code() {
        assert!(check_nanp_grouping("+1-555-123-4567").is_none());
        assert!(check_nanp_grouping("1-555-123-4567").is_none());
    }

    #[test]
    fn nanp_grouping_ignores_unformatted_runs() {
        // A single unbroken run of digits isn't claiming any grouping.
        assert!(check_nanp_grouping("5551234567").is_none());
    }

    #[test]
    fn nanp_grouping_ignores_non_nanp_lengths() {
        // 12 digits isn't a NANP number at all, grouped or not.
        assert!(check_nanp_grouping("55-512-345-6789").is_none());
    }

    #[test]
    fn lint_text_reports_correct_line_numbers() {
        let text = "no numbers here\ncall (555 123-4567 now\nfine: 555-123-4567";
        let findings = lint_text(text);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
        assert_eq!(findings[0].severity, Severity::Error);
    }
}
