use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use phonelint::{lint_text, parse_config, Finding, RuleOverrides, Severity};

const STDIN_LABEL: &str = "<stdin>";
const DEFAULT_CONFIG_NAME: &str = ".phonelintrc";

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

struct Args {
    paths: Vec<String>,
    format: Format,
    config_path: Option<String>,
    no_config: bool,
}

/// Split CLI args into file paths, an output format, and config options,
/// accepting both `--flag value` and `--flag=value`.
fn parse_args(args: Vec<String>) -> Result<Args, String> {
    let mut paths = Vec::new();
    let mut format = Format::Text;
    let mut config_path = None;
    let mut no_config = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--format" {
            let value = iter
                .next()
                .ok_or_else(|| "--format requires an argument (text or json)".to_string())?;
            format = parse_format(&value)?;
        } else if let Some(value) = arg.strip_prefix("--format=") {
            format = parse_format(value)?;
        } else if arg == "--config" {
            let value = iter
                .next()
                .ok_or_else(|| "--config requires a path argument".to_string())?;
            config_path = Some(value);
        } else if let Some(value) = arg.strip_prefix("--config=") {
            config_path = Some(value.to_string());
        } else if arg == "--no-config" {
            no_config = true;
        } else {
            paths.push(arg);
        }
    }
    if config_path.is_some() && no_config {
        return Err("--config and --no-config cannot be used together".to_string());
    }
    if paths.is_empty() {
        paths.push("-".to_string());
    }
    Ok(Args {
        paths,
        format,
        config_path,
        no_config,
    })
}

/// Look for a `.phonelintrc` in the current directory, then each parent,
/// so the linter behaves the same when run from a subdirectory of a
/// project. The nearest file wins; configs are not merged.
fn find_default_config() -> Option<PathBuf> {
    let cwd = env::current_dir().ok()?;
    cwd.ancestors()
        .map(|dir| dir.join(DEFAULT_CONFIG_NAME))
        .find(|candidate| candidate.is_file())
}

/// Load and parse the rule overrides from an explicit `--config` path, or
/// failing that a discovered `.phonelintrc`. Any read or parse failure is
/// fatal, since a config that silently doesn't apply would be worse than
/// an error.
fn load_overrides(config_path: Option<&str>, no_config: bool) -> Result<RuleOverrides, String> {
    let path = match config_path {
        Some(path) => PathBuf::from(path),
        None if no_config => return Ok(RuleOverrides::new()),
        None => match find_default_config() {
            Some(path) => path,
            None => return Ok(RuleOverrides::new()),
        },
    };
    let shown = path.display();
    let text = fs::read_to_string(&path).map_err(|err| format!("{}: {}", shown, err))?;
    parse_config(&text).map_err(|err| format!("{}: {}", shown, err))
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
    let Args {
        paths,
        format,
        config_path,
        no_config,
    } = match parse_args(env::args().skip(1).collect()) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!("{}", err);
            return ExitCode::from(2);
        }
    };

    let overrides = match load_overrides(config_path.as_deref(), no_config) {
        Ok(overrides) => overrides,
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
        let findings = lint_text(&text, &overrides);

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
