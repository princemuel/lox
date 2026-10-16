#![expect(clippy::print_stdout, reason = "this project uses stdout logs")]
#![expect(clippy::print_stderr, reason = "this project uses stderr logs")]

use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::{fs, process};

use lexopt::prelude::*;
use lox::prelude::*;

fn main() -> process::ExitCode {
    let args = match Args::parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::Usage.into();
        }
    };

    // Parsing and running recurse as deeply as the program nests, so do it all
    // on a thread with a roomy stack rather than the small one `main` gets.
    let worker = with_big_stack(move || match args.command {
        Commands::Tokenize { filename } => tokenize(&filename),
        Commands::Parse { filename } => parse(&filename),
        Commands::Evaluate { filename } => evaluate(&filename),
        Commands::Run { filename } => run(&filename),
    });

    worker.unwrap_or_else(|err| {
        eprintln!("Error: could not start the interpreter thread: {err}");
        ExitCode::OsError.into()
    })
}

fn read_source(filename: &Path) -> Result<String, process::ExitCode> {
    fs::read_to_string(filename).map_err(|err| {
        eprintln!("Error: reading '{}' failed: {err}", filename.display());
        ExitCode::NoInput.into()
    })
}

fn error_status_from_code(had_error: bool) -> process::ExitCode {
    if had_error { ExitCode::InvalidInput } else { ExitCode::Success }.into()
}

fn run(filename: &Path) -> process::ExitCode {
    let source = match read_source(filename) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let program = match Parser::new(&source).parse() {
        Ok(program) => program,
        Err(errors) => {
            report(&errors);
            return error_status_from_code(true);
        }
    };

    let mut interpreter = Interpreter::new(BufWriter::new(io::stdout().lock()));
    let outcome = interpreter.interpret(&program);
    finish(&mut interpreter, outcome)
}

fn parse(filename: &Path) -> process::ExitCode {
    let source = match read_source(filename) {
        Ok(s) => s,
        Err(code) => return code,
    };

    match Parser::new(&source).parse_expression() {
        Ok(token_tree) => {
            println!("{token_tree}");
            error_status_from_code(false)
        }
        Err(errors) => {
            report(&errors);
            error_status_from_code(true)
        }
    }
}

fn tokenize(filename: &Path) -> process::ExitCode {
    let source = match read_source(filename) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut had_error = false;

    for token in Scanner::new(&source) {
        match token {
            Ok(token) => {
                // Ignoring write errors (e.g. broken pipe) is fine for a CLI.
                let _unused = writeln!(out, "{token}");
            }
            Err(err) => {
                had_error = true;
                eprintln!("{err}");
            }
        }
    }

    let _unused = writeln!(out, "EOF  null");
    let _unused = out.flush();

    error_status_from_code(had_error)
}

/// Parses one expression from FILE, evaluates it and prints the result.
fn evaluate(filename: &Path) -> process::ExitCode {
    let source = match read_source(filename) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let expr = match Parser::new(&source).parse_expression() {
        Ok(expr) => expr,
        Err(errors) => {
            report(&errors);
            return error_status_from_code(true);
        }
    };

    let mut interpreter = Interpreter::new(BufWriter::new(io::stdout().lock()));
    let outcome = interpreter
        .evaluate(&expr)
        .and_then(|value| interpreter.print(&value));
    finish(&mut interpreter, outcome)
}

/// Turns how a run ended into the process's exit status, reporting any failure.
fn finish<W: Write>(
    interpreter: &mut Interpreter<'_, W>,
    outcome: Result<(), Failure>,
) -> process::ExitCode {
    // Flush first, so everything the program printed is out before any error
    // text.
    let flushed = interpreter.flush();

    let result = match outcome {
        Ok(()) => flushed.map_or_else(
            |err| {
                if err.kind() != io::ErrorKind::BrokenPipe {
                    eprintln!("Error: writing output failed: {err}");
                }
                ExitCode::IoError
            },
            |()| ExitCode::Success,
        ),
        Err(Failure::Output(err)) => {
            if err.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("Error: writing output failed: {err}");
            }
            ExitCode::IoError
        }
        Err(Failure::Runtime(err)) => {
            eprintln!("{err}");
            ExitCode::Software
        }
    };

    result.into()
}

/// One line per problem, on stderr, in the book's format.
fn report(errors: &[SyntaxError]) {
    for error in errors {
        eprintln!("{error}");
    }
}

#[repr(u8)]
#[derive(Clone, Copy)]
enum ExitCode {
    Success = 0,
    Usage = 64,        // command line usage error
    InvalidInput = 65, // bad input data
    NoInput = 66,      // input file didn't exist or wasn't readable
    Software = 70,     // the program failed while running (a runtime error)
    OsError = 71,      // the operating system refused something (no thread)
    IoError = 74,      // could not write the output
}

impl From<ExitCode> for process::ExitCode {
    fn from(code: ExitCode) -> Self { Self::from(code as u8) }
}

#[derive(Debug)]
struct Args {
    command: Commands,
}

#[derive(Debug)]
enum Commands {
    Tokenize { filename: PathBuf },
    Parse { filename: PathBuf },
    Evaluate { filename: PathBuf },
    Run { filename: PathBuf },
}

impl Args {
    fn parse() -> Result<Self, lexopt::Error> {
        let mut parser = lexopt::Parser::from_env();

        let mut filename = None;
        let mut subcommand = None;

        while let Some(arg) = parser.next()? {
            match arg {
                Short('h') | Long("help") => show_help(subcommand.as_deref())?,
                Short('V') | Long("version") => {
                    println!("lox {}", env!("CARGO_PKG_VERSION"));
                    process::exit(0);
                }
                Value(val) if subcommand.is_none() => {
                    subcommand = Some(val.string()?);
                }
                Value(val) if subcommand.as_deref() == Some("help") => {
                    show_help(Some(&val.string()?))?;
                }
                Value(val) if filename.is_none() => {
                    filename = Some(val.into());
                }
                _ => return Err(arg.unexpected()),
            }
        }

        let subcommand = subcommand.ok_or("missing command (tokenize|parse|evaluate|run)")?;

        if subcommand == "help" {
            show_help(None)?;
        }

        let filename = filename.ok_or("missing filename")?;

        let command = match subcommand.as_str() {
            "tokenize" => Commands::Tokenize { filename },
            "parse" => Commands::Parse { filename },
            "evaluate" => Commands::Evaluate { filename },
            "run" => Commands::Run { filename },
            other => {
                return Err(lexopt::Error::Custom(
                    format!("unknown command '{other}' (expected tokenize|parse|evaluate|run)")
                        .into(),
                ));
            }
        };

        Ok(Self { command })
    }
}

/// Prints help for `target` (or top-level help) and exits 0.
/// It returns an error only for an unrecognized subcommand.
fn show_help(target: Option<&str>) -> Result<!, lexopt::Error> {
    let text = match target {
        Some("tokenize") => include_str!("../templates/tokenize.txt"),
        Some("parse") => include_str!("../templates/parse.txt"),
        Some("evaluate") => include_str!("../templates/evaluate.txt"),
        Some("run") => include_str!("../templates/run.txt"),
        Some("help") | None => include_str!("../templates/help.txt"),
        Some(other) => {
            return Err(lexopt::Error::Custom(format!("unrecognized subcommand '{other}'").into()));
        }
    };
    print!("{text}");
    process::exit(0)
}
