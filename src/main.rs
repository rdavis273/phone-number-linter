use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;

use phonelint::{lint_text, Severity};

const STDIN_LABEL: &str = "<stdin>";

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

fn main() -> ExitCode {
    let mut paths: Vec<String> = env::args().skip(1).collect();
    if paths.is_empty() {
        paths.push("-".to_string());
    }

    let mut had_error = false;
    let mut had_read_failure = false;

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

        for finding in lint_text(&text) {
            println!(
                "{}:{}:{}: {}: {}",
                label,
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
