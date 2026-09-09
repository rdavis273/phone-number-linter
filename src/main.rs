use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

use phonelint::{lint_text, Finding, Severity};

const STDIN_LABEL: &str = "<stdin>";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Text,
    Json,
}

/// Read a source's full text. "-" (or no path at all, handled by the
/// caller) means stdin, matching the convention of grep, cat, etc.
fn read_source(path: &str) -> io::Result<String> {
    if path == "-" {
        let mut buf = String::new();
        io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        fs::read_to_string(path)
    }
}

fn parse_format(value: &str) -> Result<Format, String> {
    match value {
        "text" => Ok(Format::Text),
        "json" => Ok(Format::Json),
        other => Err(format!("unknown format \"{}\", expected text or json", other)),
    }
}

/// Split CLI args into file paths and an output format, accepting both
/// `--format json` and `--format=json`.
fn parse_args(args: Vec<String>) -> Result<(Vec<String>, Format), String> {
    let mut paths = Vec::new();
    let mut format = Format::Text;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--format" {
            let value = iter
                .next()
                .ok_or_else(|| "--format requires an argument (text or json)".to_string())?;
            format = parse_format(&value)?;
        } else if let Some(value) = arg.strip_prefix("--format=") {
            format = parse_format(value)?;
        } else {
            paths.push(arg);
        }
    }
    if paths.is_empty() {
        paths.push("-".to_string());
    }
    Ok((paths, format))
}

fn print_text(label: &str, findings: &[Finding]) {
    for finding in findings {
        println!(
            "{}:{}:{}: {}: {}",
            label,
            finding.line,
            finding.column,
            finding.severity.as_str(),
            finding.message
        );
    }
}

fn main() -> ExitCode {
    let (paths, format) = match parse_args(env::args().skip(1).collect()) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("{}", err);
            return ExitCode::from(2);
        }
    };

    let mut had_error = false;
    let mut had_read_failure = false;
    let mut json_entries: Vec<String> = Vec::new();

    for path in &paths {
        let text = match read_source(path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("{}: {}", path, err);
                had_read_failure = true;
                continue;
            }
        };

        let label = if path == "-" { STDIN_LABEL } else { path };
        let findings = lint_text(&text);

        if findings.iter().any(|f| f.severity == Severity::Error) {
            had_error = true;
        }

        match format {
            Format::Text => print_text(label, &findings),
            Format::Json => json_entries.extend(findings.iter().map(|f| f.to_json(label))),
        }
    }

    if format == Format::Json {
        println!("[{}]", json_entries.join(","));
    }

    if had_read_failure {
        ExitCode::from(2)
    } else if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
