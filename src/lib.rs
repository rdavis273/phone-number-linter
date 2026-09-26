//! Core linting logic for phone number formatting.
//!
//! Every public function in this module is pure: given the same input it
//! always produces the same output, and none of them touch a file, a
//! clock, or any other outside state. `main.rs` is the only place that
//! does I/O. Keeping the split this strict means every rule below can be
//! unit tested with a plain string in, a `Vec<Finding>` out.

use std::collections::HashMap;

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

/// Flag NANP numbers that fall in the reserved fictional range
/// 555-0100 through 555-0199, e.g. "555-555-0199" or "(212) 555-0142".
/// These are set aside by the numbering plan specifically so they can't
/// be assigned to a real subscriber, which makes them a common sign that
/// a placeholder slipped into real content instead of getting swapped
/// out before publishing.
pub fn check_placeholder_number(candidate: &str) -> Option<String> {
    let digits: String = candidate.chars().filter(|c| c.is_ascii_digit()).collect();
    let local = match digits.len() {
        11 if digits.starts_with('1') => &digits[1..],
        10 => &digits[..],
        7 => &digits[..],
        _ => return None,
    };

    let (exchange, subscriber) = if local.len() == 10 {
        (&local[3..6], &local[6..10])
    } else {
        (&local[0..3], &local[3..7])
    };

    if exchange == "555" && subscriber.starts_with("01") {
        Some(format!(
            "\"{}\" is in the reserved placeholder range 555-0100 through 555-0199",
            candidate
        ))
    } else {
        None
    }
}

type Check = fn(&str) -> Option<String>;

const CHECKS: [(&str, Check, Severity); 5] = [
    ("mixed_separators", check_mixed_separators, Severity::Warning),
    ("digit_count", check_digit_count, Severity::Error),
    ("balanced_parens", check_balanced_parens, Severity::Error),
    ("nanp_grouping", check_nanp_grouping, Severity::Warning),
    ("placeholder_number", check_placeholder_number, Severity::Warning),
];

/// A per-rule severity override loaded from a config file. `Off`
/// disables the rule outright, rather than just changing how loud it
/// reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeverityOverride {
    Warning,
    Error,
    Off,
}

/// Rule name (matching one of the names in [`CHECKS`], e.g.
/// "mixed_separators") to the severity a config file wants it reported
/// at. A rule with no entry keeps its built-in default severity.
pub type RuleOverrides = HashMap<String, SeverityOverride>;

/// Parse a config file of `rule = severity` lines, one override per
/// line. Blank lines and anything from `#` to the end of a line are
/// ignored. `severity` is one of "warning", "error", or "off". Unknown
/// rule names and unknown severities are reported as errors with a line
/// number, since a silently-ignored typo would leave a rule running at
/// a severity nobody asked for.
pub fn parse_config(text: &str) -> Result<RuleOverrides, String> {
    let mut overrides = RuleOverrides::new();
    for (i, raw_line) in text.lines().enumerate() {
        let line_number = i + 1;
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        let (name, severity) = line.split_once('=').ok_or_else(|| {
            format!(
                "line {}: expected \"rule = severity\", got \"{}\"",
                line_number, raw_line
            )
        })?;
        let name = name.trim();
        let severity = severity.trim();

        if !CHECKS.iter().any(|(rule_name, _, _)| *rule_name == name) {
            let known: Vec<&str> = CHECKS.iter().map(|(rule_name, _, _)| *rule_name).collect();
            return Err(format!(
                "line {}: unknown rule \"{}\", expected one of {:?}",
                line_number, name, known
            ));
        }

        let severity = match severity {
            "warning" => SeverityOverride::Warning,
            "error" => SeverityOverride::Error,
            "off" => SeverityOverride::Off,
            other => {
                return Err(format!(
                    "line {}: unknown severity \"{}\", expected warning, error, or off",
                    line_number, other
                ))
            }
        };

        overrides.insert(name.to_string(), severity);
    }
    Ok(overrides)
}

/// Run every rule against one candidate, tagging results with the line
/// they came from. `overrides` is applied by rule name; a rule set to
/// `Off` is skipped entirely rather than downgraded.
pub fn lint_candidate(line: usize, candidate: &Candidate, overrides: &RuleOverrides) -> Vec<Finding> {
    CHECKS
        .iter()
        .filter_map(|(name, check, default_severity)| {
            let severity = match overrides.get(*name) {
                Some(SeverityOverride::Off) => return None,
                Some(SeverityOverride::Warning) => Severity::Warning,
                Some(SeverityOverride::Error) => Severity::Error,
                None => *default_severity,
            };
            check(&candidate.text).map(|message| Finding {
                line,
                column: candidate.column,
                severity,
                message,
            })
        })
        .collect()
}

/// Extract and lint every candidate on one line.
pub fn lint_line(line_number: usize, line: &str, overrides: &RuleOverrides) -> Vec<Finding> {
    extract_candidates(line)
        .iter()
        .flat_map(|c| lint_candidate(line_number, c, overrides))
        .collect()
}

/// Lint a whole file's worth of text, one line at a time. Line numbers
/// in the results are 1-based.
pub fn lint_text(text: &str, overrides: &RuleOverrides) -> Vec<Finding> {
    text.lines()
        .enumerate()
        .flat_map(|(i, line)| lint_line(i + 1, line, overrides))
        .collect()
}

/// Escape a string for embedding in a JSON string literal.
fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

impl Finding {
    /// Render as a single-line JSON object. `file` is the source label
    /// (a path, or "<stdin>") rather than a field on `Finding` itself,
    /// since callers already loop file-by-file and know their own label.
    pub fn to_json(&self, file: &str) -> String {
        format!(
            "{{\"file\":\"{}\",\"line\":{},\"column\":{},\"severity\":\"{}\",\"message\":\"{}\"}}",
            escape_json(file),
            self.line,
            self.column,
            self.severity.as_str(),
            escape_json(&self.message)
        )
    }
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
    fn flags_placeholder_range() {
        assert!(check_placeholder_number("555-555-0199").is_some());
        assert!(check_placeholder_number("(212) 555-0142").is_some());
        assert!(check_placeholder_number("+1-555-555-0100").is_some());
        assert!(check_placeholder_number("555-0123").is_some());
    }

    #[test]
    fn placeholder_check_ignores_real_looking_numbers() {
        assert!(check_placeholder_number("555-123-4567").is_none());
        assert!(check_placeholder_number("212-555-0200").is_none());
        assert!(check_placeholder_number("212-555-1234").is_none());
    }

    #[test]
    fn placeholder_check_ignores_numbers_with_no_nanp_shape() {
        assert!(check_placeholder_number("+14155552671").is_none());
        assert!(check_placeholder_number("12345").is_none());
    }

    #[test]
    fn finding_to_json_escapes_quotes_and_backslashes() {
        let finding = Finding {
            line: 3,
            column: 5,
            severity: Severity::Warning,
            message: "mixed separators in \"555-1.234\"".to_string(),
        };
        let json = finding.to_json("contacts.txt");
        assert_eq!(
            json,
            "{\"file\":\"contacts.txt\",\"line\":3,\"column\":5,\"severity\":\"warning\",\"message\":\"mixed separators in \\\"555-1.234\\\"\"}"
        );
    }

    #[test]
    fn lint_text_reports_correct_line_numbers() {
        let text = "no numbers here\ncall (555 123-4567 now\nfine: 555-123-4567";
        let findings = lint_text(text, &RuleOverrides::new());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn parse_config_reads_valid_overrides() {
        let config = "mixed_separators = error\n# a comment line\n\nnanp_grouping = off\n";
        let overrides = parse_config(config).unwrap();
        assert_eq!(
            overrides.get("mixed_separators"),
            Some(&SeverityOverride::Error)
        );
        assert_eq!(overrides.get("nanp_grouping"), Some(&SeverityOverride::Off));
        assert_eq!(overrides.len(), 2);
    }

    #[test]
    fn parse_config_ignores_inline_comments_and_blank_lines() {
        let config = "\n  \nplaceholder_number = warning # keep this one loud\n";
        let overrides = parse_config(config).unwrap();
        assert_eq!(
            overrides.get("placeholder_number"),
            Some(&SeverityOverride::Warning)
        );
    }

    #[test]
    fn parse_config_rejects_unknown_rule() {
        let err = parse_config("not_a_real_rule = error").unwrap_err();
        assert!(err.contains("unknown rule"));
        assert!(err.contains("not_a_real_rule"));
    }

    #[test]
    fn parse_config_rejects_unknown_severity() {
        let err = parse_config("digit_count = catastrophic").unwrap_err();
        assert!(err.contains("unknown severity"));
    }

    #[test]
    fn parse_config_rejects_malformed_line() {
        let err = parse_config("digit_count error").unwrap_err();
        assert!(err.contains("line 1"));
    }

    #[test]
    fn override_changes_reported_severity() {
        let candidate = Candidate {
            column: 1,
            text: "555-123.4567".to_string(),
        };
        let mut overrides = RuleOverrides::new();
        overrides.insert("mixed_separators".to_string(), SeverityOverride::Error);
        let findings = lint_candidate(1, &candidate, &overrides);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn override_off_suppresses_the_rule() {
        let candidate = Candidate {
            column: 1,
            text: "(555 123-4567".to_string(),
        };
        let mut overrides = RuleOverrides::new();
        overrides.insert("balanced_parens".to_string(), SeverityOverride::Off);
        let findings = lint_candidate(1, &candidate, &overrides);
        assert!(findings.is_empty());
    }
}
