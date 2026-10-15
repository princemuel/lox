//! Conformance runner for the Crafting Interpreters Lox test suite.
//!
//! Each `.lox` file under `tests/fixtures` carries its own expectations in
//! comments (`// expect: ...`, `// expect runtime error: ...`, `// Error ...`).
//! This harness runs the interpreter binary on every file and reports one
//! test per file.
//!
//! Requires in Cargo.toml:
//! ```toml
//! [dev-dependencies]
//! libtest-mimic = "0.8"
//!
//! [[test]]
//! name = "fixtures"
//! harness = false
//! ```
//!
//! Run:
//!
//! ```sh
//! cargo test --test fixtures
//! LOX_BIN=/path/to/other/lox cargo test --test fixtures   # try another interpreter
//! ```

use core::time::Duration;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use libtest_mimic::{Arguments, Failed, Trial};

const FIXTURES: &str = "tests/fixtures";

/// Directories that never apply to a tree-walking interpreter.
const EXCLUDED_DIRS: &[&str] = &["benchmark", "limit"];

/// Fixture features not yet implemented
/// They still show up in the output, marked "ignored".
const PENDING: &[&str] = &[];

/// Which implementation-specific expectations apply.
const IMPLEMENTATION_TAG: &str = "java";

/// When true, error tests must match the book's format exactly
const STRICT_ERROR_LINES: bool = true;

/// Subcommands supported and already implemented. Tests that need any other
/// subcommands are reported as "ignored" instead of failing.
const IMPLEMENTED: &[&str] = &["tokenize", "parse"];

/// Which subcommand exercises which tests: the first matching path prefix wins
/// and everything else uses `DEFAULT_SUBCOMMAND`. This mirrors how the book's
/// own runner drives each chapter (scanning -> tokenize, expressions ->
/// parse/evaluate).
const SUBCOMMANDS_BY_PATH: &[(&str, &str)] = &[
    ("scanning/", "tokenize"),
    ("expressions/parse.lox", "parse"),
    ("expressions/evaluate.lox", "evaluate"),
];
const DEFAULT_SUBCOMMAND: &str = "run";

const RUN_SYNTAX_ERROR_TESTS_EARLY: bool = true;

/// Compile-time errors that jlox's *resolver* reports (`Resolver.java`), as
/// opposed to the scanner/parser. A test expecting any of these needs the
/// resolver, not just the parser.
const RESOLVER_ERRORS: &[&str] = &[
    "A class can't inherit from itself.",
    "Already a variable with this name in this scope.",
    "Can't read local variable in its own initializer.",
    "Can't return a value from an initializer.",
    "Can't return from top-level code.",
    "Can't use 'super' in a class with no superclass.",
    "Can't use 'super' outside of a class.",
    "Can't use 'this' outside of a class.",
];

const TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const MAX_CAPTURE_BYTES: u64 = 1 << 20; // 1 MiB per stream

const EXIT_COMPILE_ERROR: i32 = 65;
const EXIT_RUNTIME_ERROR: i32 = 70;

// ─────────────────────────────────── Entry ───────────────────────────────────

fn main() {
    let args = Arguments::from_args();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES);

    let mut trials = build_trials(&root);
    // `#[test]` functions are not run when `harness = false`, so the harness
    // tests itself through an ordinary trial.
    trials.push(Trial::test("harness/parser_self_test", parser_self_test));

    libtest_mimic::run(&args, trials).exit();
}

fn build_trials(root: &Path) -> Vec<Trial> {
    let files = match discover(root) {
        Ok(files) if !files.is_empty() => files,
        // Zero tests would "pass" silently, so an empty suite is a failure.
        Ok(_) => {
            return vec![setup_failure(format!("no .lox files found under {}", root.display()))];
        }
        Err(message) => return vec![setup_failure(message)],
    };

    files
        .into_iter()
        .map(|path| {
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            let subcommand = subcommand_for(&name);
            let ignored = !can_run_now(&path, &name, subcommand);

            Trial::test(name, move || run_case(&path, subcommand)).with_ignored_flag(ignored)
        })
        .collect()
}

/// Whether this test can meaningfully run yet; if not it is reported "ignored".
fn can_run_now(path: &Path, name: &str, subcommand: &str) -> bool {
    if PENDING.iter().any(|&prefix| name.starts_with(prefix)) || is_nontest(path) {
        return false;
    }
    IMPLEMENTED.contains(&subcommand)
        || (RUN_SYNTAX_ERROR_TESTS_EARLY && subcommand == "run" && is_syntax_error_test(path))
}

/// True if the file's only expected outcome is scanner/parser errors.
fn is_syntax_error_test(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    parse_expectations(&String::from_utf8_lossy(&bytes))
        .is_ok_and(|expectations| expects_only_syntax_errors(&expectations))
}

fn expects_only_syntax_errors(exp: &Expectations) -> bool {
    exp.runtime_error.is_none()
        && !exp.compile_errors.is_empty()
        && exp.compile_errors.iter().all(|error| {
            !RESOLVER_ERRORS
                .iter()
                .any(|resolver| error.message.ends_with(resolver))
        })
}

fn subcommand_for(name: &str) -> &'static str {
    SUBCOMMANDS_BY_PATH
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix))
        .map_or(DEFAULT_SUBCOMMAND, |&(_, subcommand)| subcommand)
}

fn setup_failure(message: String) -> Trial {
    Trial::test("harness/setup", move || Err(Failed::from(message)))
}

/// Finds every `.lox` file under `root`, skipping `EXCLUDED_DIRS`.
fn discover(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut pending_dirs = vec![root.to_path_buf()];

    while let Some(dir) = pending_dirs.pop() {
        let entries = fs::read_dir(&dir)
            .map_err(|e| format!("cannot read directory {}: {e}", dir.display()))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("cannot list {}: {e}", dir.display()))?;
            let kind = entry
                .file_type()
                .map_err(|e| format!("cannot stat {}: {e}", entry.path().display()))?;
            let path = entry.path();

            #[expect(clippy::else_if_without_else, reason = "will try a better alternative later")]
            if kind.is_dir() {
                let excluded = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| EXCLUDED_DIRS.contains(&n));
                if !excluded {
                    pending_dirs.push(path);
                }
            } else if kind.is_file() && path.extension().is_some_and(|e| e == "lox") {
                files.push(path);
            }
        }
    }

    files.sort(); // deterministic order
    Ok(files)
}

/// Files containing `// nontest` are helpers, not tests.
fn is_nontest(path: &Path) -> bool {
    fs::read(path).is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains("// nontest"))
}

#[derive(Debug, Default)]
struct ExpectedError {
    line: usize,
    message: String,
}

#[derive(Debug, Default)]
struct Expectations {
    stdout: Vec<String>,
    compile_errors: Vec<ExpectedError>,
    runtime_error: Option<ExpectedError>,
}

impl Expectations {
    pub(crate) const fn exit_code(&self) -> i32 {
        if self.runtime_error.is_some() {
            EXIT_RUNTIME_ERROR
        } else if !self.compile_errors.is_empty() {
            EXIT_COMPILE_ERROR
        } else {
            0
        }
    }
}

fn parse_expectations(src: &str) -> Result<Expectations, String> {
    let mut expected = Expectations::default();

    for (index, line) in src.lines().enumerate() {
        let line_no = index + 1;
        let Some(comment) = after(line, "// ") else {
            continue;
        };

        if let Some(rest) = comment.strip_prefix("expect runtime error: ") {
            expected.runtime_error =
                Some(ExpectedError { line: line_no, message: rest.trim().to_owned() });
        } else if let Some(rest) = comment.strip_prefix("expect:") {
            // `// expect:` with nothing after it means "an empty line".
            expected
                .stdout
                .push(rest.strip_prefix(' ').unwrap_or(rest).to_owned());
        } else if comment.starts_with("Error") {
            // `// Error at 'x': msg` refers to the line the comment is on.
            expected
                .compile_errors
                .push(ExpectedError { line: line_no, message: comment.trim().to_owned() });
        } else if let Some((tag, error_line, message)) = parse_tagged_error(comment) {
            // `// [line N] Error ...` applies to everyone; `// [java
            // line N]` and `// [c line N]` only to
            // the matching implementation.
            if tag.is_none() || tag == Some(IMPLEMENTATION_TAG) {
                expected
                    .compile_errors
                    .push(ExpectedError { line: error_line, message: message.trim().to_owned() });
            }
        }
    }

    if expected.runtime_error.is_some() && !expected.compile_errors.is_empty() {
        return Err("a test cannot expect both compile-time and runtime errors".into());
    }
    Ok(expected)
}

/// The text following the first occurrence of `marker`.
fn after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.find(marker).map(|i| &line[i + marker.len()..])
}

/// Parses `[line N] Error...`, `[java line N] Error...`, `[c line N] Error...`
/// into (implementation tag, line number, message).
fn parse_tagged_error(s: &str) -> Option<(Option<&str>, usize, &str)> {
    let s = s.strip_prefix('[')?;
    let (head, message) = s.split_once("] ")?;
    let (tag, number) = if let Some(number) = head.strip_prefix("line ") {
        (None, number)
    } else {
        let (tag, number) = head.split_once(" line ")?;
        (Some(tag), number)
    };
    let line = number.trim().parse().ok()?;
    message.starts_with("Error").then_some((tag, line, message))
}

struct Outcome {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

/// Resolved by Cargo at compile time: the `[[bin]] name`, not the package name.
const DEFAULT_BIN: &str = env!("CARGO_BIN_EXE_lox");

/// `LOX_BIN` overrides the binary, so you can validate the harness against a
/// known-good interpreter (e.g. the book's jlox).
fn interpreter_path() -> PathBuf {
    std::env::var_os("LOX_BIN").map_or_else(|| PathBuf::from(DEFAULT_BIN), PathBuf::from)
}

fn run_interpreter(bin: &Path, subcommand: &str, script: &Path) -> Result<Outcome, Failed> {
    let mut child = Command::new(bin)
        .arg(subcommand)
        .arg(script)
        .stdin(Stdio::null()) // a stray REPL/read can never hang the suite
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to launch {}: {e}", bin.display()))?;

    // Drain both pipes on their own threads. Otherwise a child that writes more
    // than the OS pipe buffer would block forever while we poll for its exit.
    let stdout_reader = spawn_reader(child.stdout.take());
    let stderr_reader = spawn_reader(child.stderr.take());

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > TIMEOUT => {
                let _unused = child.kill();
                let _unused = child.wait(); // reap, so no zombie is left behind
                return Err(format!(
                    "timed out after {TIMEOUT:?} (infinite loop?); process killed"
                )
                .into());
            }
            Ok(None) => thread::sleep(POLL_INTERVAL),
            Err(e) => return Err(format!("failed while waiting for interpreter: {e}").into()),
        }
    };

    Ok(Outcome { status, stdout: join_lossy(stdout_reader), stderr: join_lossy(stderr_reader) })
}

fn spawn_reader<R>(stream: Option<R>) -> JoinHandle<Vec<u8>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || stream.map(drain).unwrap_or_default())
}

/// Keeps the first `MAX_CAPTURE_BYTES`, then discards the rest so a runaway
/// printer can neither exhaust memory nor block on a full pipe.
fn drain<R>(mut stream: R) -> Vec<u8>
where
    R: Read,
{
    let mut buf = Vec::new();
    let _no_of_bytes_read = (&mut stream).take(MAX_CAPTURE_BYTES).read_to_end(&mut buf);
    let _no_of_bytes_copied = io::copy(&mut stream, &mut io::sink());
    buf
}

fn join_lossy(handle: JoinHandle<Vec<u8>>) -> String {
    String::from_utf8_lossy(&handle.join().unwrap_or_default()).into_owned()
}

#[cfg(unix)]
fn describe_status(status: ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt as _;
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exit code {code}"),
        (None, Some(signal)) => format!("killed by signal {signal}"),
        (None, None) => "unknown termination".to_owned(),
    }
}

#[cfg(not(unix))]
fn describe_status(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => "terminated abnormally".to_owned(),
    }
}

// ───────────────────────────────── One test case ─────────────────────────────

fn run_case(path: &Path, subcommand: &str) -> Result<(), Failed> {
    let bytes = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let source = String::from_utf8_lossy(&bytes);
    let expectations = parse_expectations(&source).map_err(|e| format!("bad test file: {e}"))?;

    let outcome = run_interpreter(&interpreter_path(), subcommand, path)?;

    let problems = verify(&expectations, &outcome);
    if problems.is_empty() {
        return Ok(());
    }

    let stderr = match tail(&outcome.stderr, 15) {
        t if t.is_empty() => "(empty)".to_owned(),
        t => t,
    };

    Err(format!("{}\n--- stderr (last lines) ---\n{stderr}", problems.join("\n")).into())
}

/// Collects every mismatch instead of stopping at the first one.
fn verify(exp: &Expectations, run: &Outcome) -> Vec<String> {
    let mut problems = Vec::new();

    // 1. Exit status first: a crash explains any stdout mismatch that follows.
    let expected_exit_code = exp.exit_code();
    if run.status.code() != Some(expected_exit_code) {
        let mut message = format!(
            "exit status: expected {expected_exit_code}, got {}",
            describe_status(run.status)
        );

        if run.status.code() == Some(101i32) {
            message.push_str(" (101 is what a Rust panic exits with)");
        }

        problems.push(message);
    }

    // 2. stdout, line for line.
    let actual: Vec<&str> = run.stdout.lines().collect();
    let expected: Vec<&str> = exp.stdout.iter().map(String::as_str).collect();
    if actual != expected {
        problems.push(first_difference(&expected, &actual));
    }

    // 3. stderr must mention each expected error.
    let mut needles = Vec::new();

    if let Some(err) = &exp.runtime_error {
        needles.push(err.message.clone());
        if STRICT_ERROR_LINES {
            needles.push(format!("[line {}]", err.line));
        }
    }

    for err in &exp.compile_errors {
        needles.push(if STRICT_ERROR_LINES {
            format!("[line {}] {}", err.line, err.message)
        } else {
            err.message.clone()
        });
    }

    problems.extend(
        needles
            .into_iter()
            .filter(|needle| !run.stderr.contains(needle.trim()))
            .map(|needle| format!("stderr is missing {needle:?}")),
    );

    // 4. ...and must not report errors that nobody expected.
    if STRICT_ERROR_LINES {
        problems.extend(
            unexpected_error_lines(exp, &run.stderr)
                .into_iter()
                .map(|line| format!("unexpected error on stderr: {line:?}")),
        );
    }

    problems
}

/// Lines of `stderr` shaped like a compile error (`[line N] Error...`) that the
/// test did not ask for. Other stderr text, and a runtime error's `[line N]`
/// trace line, are not compile errors and are ignored.
fn unexpected_error_lines(exp: &Expectations, stderr: &str) -> Vec<String> {
    let expected: Vec<String> = exp
        .compile_errors
        .iter()
        .map(|err| format!("[line {}] {}", err.line, err.message))
        .collect();

    stderr
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("[line ") && line.contains("] Error"))
        .filter(|line| !expected.iter().any(|want| want == line))
        .map(str::to_owned)
        .collect()
}

fn first_difference(expected: &[&str], got: &[&str]) -> String {
    let index = expected
        .iter()
        .zip(got)
        .position(|(e, g)| e != g)
        .unwrap_or(expected.len().min(got.len()));

    let show =
        |line: Option<&&str>| line.map_or("<no line>".to_owned(), |text| format!("{text:?}"));

    format!(
        "stdout differs at line {}: expected {}, got {} ({} lines expected, {} received)",
        index + 1,
        show(expected.get(index)),
        show(got.get(index)),
        expected.len(),
        got.len()
    )
}

fn tail(text: &str, max_lines: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(max_lines)..].join("\n")
}

fn parser_self_test() -> Result<(), Failed> {
    let source = "\
print 1; // expect: 1
print \"\"; // expect:
var a = ; // Error at ';': Expect expression.
// [line 7] Error at 'x': applies to everyone
// [other line 8] Error at 'y': some other implementation
";
    let exp = parse_expectations(source)?;

    if exp.stdout != ["1", ""] {
        return Err(format!("stdout expectations parsed as {:?}", exp.stdout).into());
    }
    let lines: Vec<usize> = exp.compile_errors.iter().map(|e| e.line).collect();
    if lines != [3, 7] {
        return Err(format!("compile error lines parsed as {lines:?}").into());
    }
    if exp.exit_code() != EXIT_COMPILE_ERROR {
        return Err("compile errors should imply exit code 65".into());
    }

    if subcommand_for("scanning/numbers.lox") != "tokenize"
        || subcommand_for("if/else.lox") != DEFAULT_SUBCOMMAND
    {
        return Err("subcommand routing is wrong".into());
    }

    let mixed = "// Error at ';': x\n// expect runtime error: y\n";
    if parse_expectations(mixed).is_ok() {
        return Err("mixed compile/runtime expectations must be rejected".into());
    }

    // Which error tests can run before the resolver exists?
    let syntax = parse_expectations("var a = ; // Error at ';': Expect expression.\n")?;
    let resolver =
        parse_expectations("return 1; // Error at 'return': Can't return from top-level code.\n")?;
    let runtime =
        parse_expectations("1 + nil; // expect runtime error: Operands must be numbers.\n")?;
    if !expects_only_syntax_errors(&syntax)
        || expects_only_syntax_errors(&resolver)
        || expects_only_syntax_errors(&runtime)
        || expects_only_syntax_errors(&Expectations::default())
    {
        return Err("syntax-error vs resolver-error classification is wrong".into());
    }

    // Unexpected-error detection: exact lines only; trace lines and prose are
    // ignored.
    let stderr = "[line 1] Error at ';': Expect expression.\n[line 9] Error at 'x': extra\n[line \
                  9]\nsome unrelated text\n";
    let extra = unexpected_error_lines(&syntax, stderr);
    if extra != ["[line 9] Error at 'x': extra"] {
        return Err(format!("unexpected-error lines computed as {extra:?}").into());
    }
    Ok(())
}
