# phonelint

A command-line linter that scans text files for phone numbers and reports
formatting problems, with a file, line, and column for each finding, like
a compiler warning.

## Why

Phone numbers end up scattered across codebases and docs in every
convention imaginable: `555-123-4567`, `(555) 123.4567`, `+1 555 123
4567`, sometimes all three in the same file. Most of the time nobody
notices until a support script or a regex somewhere chokes on the one
that's formatted differently. This tool catches the obvious problems
(mixed separators, unbalanced parentheses, digit counts that can't be a
real number, 10-digit numbers grouped on the wrong boundaries) before
they cause that kind of bug.

It is not a number *validator* in the libphonenumber sense — it doesn't
know that `+1 555 0100` isn't a real assigned NANP number. It checks
shape, not assignment.

## Usage

```
$ cargo run -- contacts.txt
contacts.txt:2:6: error: unbalanced parentheses in phone number "(555 123-4567"
contacts.txt:5:12: warning: mixed separators ['-', '.'] in phone number "555-123.4567"
```

Given `contacts.txt`:

```
Support: 555-123-4567
Sales: (555 123-4567 (ask for Dana)
International: +14155552671
Fax: 555.123.4567
Backup line: 555-123.4567
```

Reads from stdin if you pass no files, or pass `-` in place of a
filename, so it fits into a pipeline:

```
$ git show HEAD:contacts.txt | cargo run --
<stdin>:2:6: error: unbalanced parentheses in phone number "(555 123-4567"
```

Pass `--format json` for machine-readable output — a single JSON array
of finding objects across all files given, in the order they were
found:

```
$ cargo run -- --format json contacts.txt
[{"file":"contacts.txt","line":2,"column":6,"severity":"error","message":"unbalanced parentheses in phone number \"(555 123-4567\""}]
```

Exit code is `0` when nothing errors, `1` when at least one error-level
finding is reported, and `2` if a file couldn't be read at all, or the
command line couldn't be parsed.

## Building

Standard library only, no external crates:

```
cargo build --release
```

## Design

All of the linting logic lives in `src/lib.rs` as pure functions —
`extract_candidates`, `check_mixed_separators`, `check_digit_count`,
`check_balanced_parens`, `check_nanp_grouping`, `lint_line`, `lint_text`,
`Finding::to_json`. None of them touch a
file or the environment; they take a string, return data. `src/main.rs`
is the only part that does I/O (reading files, printing to stdout). That
split is deliberate: every rule can be tested with a plain string in, a
`Vec<Finding>` out, no fixtures or temp files needed. See the tests at
the bottom of `src/lib.rs` for examples.

## License

MIT, see [LICENSE](LICENSE).
