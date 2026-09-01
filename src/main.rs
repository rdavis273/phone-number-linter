use std::env;
use std::fs;
use std::process::ExitCode;

use phonelint::{lint_text, Severity};

fn main() -> ExitCode {
    let paths: Vec<String> = env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: phonelint <file> [file...]");
        return ExitCode::from(2);
    }

    let mut had_error = false;
    let mut had_read_failure = false;

    for path in &paths {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) => {
                eprintln!("{}: {}", path, err);
                had_read_failure = true;
                continue;
            }
        };

        for finding in lint_text(&text) {
            println!(
                "{}:{}:{}: {}: {}",
                path,
                finding.line,
                finding.column,
                finding.severity.as_str(),
                finding.message
            );
            if finding.severity == Severity::Error {
                had_error = true;
            }
        }
    }

    if had_read_failure {
        ExitCode::from(2)
    } else if had_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
