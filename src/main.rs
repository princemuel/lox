#![allow(clippy::print_stdout, reason = "this project uses logs")]
#![allow(clippy::print_stderr, reason = "this project uses logs")]

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::{fs, process};

use lexopt::prelude::*;
use loxxi::prelude::*;

fn main() -> process::ExitCode {
    let args = match Args::parse() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {e}");
            return ExitCode::Usage.into();
        }
    };

    match args.command {
        Commands::Tokenize { filename } => tokenize(&filename),
        Commands::Parse { filename } => parse(&filename),
        Commands::Run { filename } => run(&filename),
    }
}

fn read_source(filename: &Path) -> Result<String, process::ExitCode> {
    fs::read_to_string(filename).map_err(|err| {
        eprintln!("Error: reading '{}' failed: {err}", filename.display());
        ExitCode::NoInput.into()
    })
}

fn status(had_error: bool) -> process::ExitCode {
    if had_error { ExitCode::InvalidInput } else { ExitCode::Success }.into()
}

fn run(filename: &Path) -> process::ExitCode {
    let source = match read_source(filename) {
        Ok(s) => s,
        Err(code) => return code,
    };

    match Parser::new(&source).parse() {
        Ok(value) => {
            println!("{value}");
            status(false)
        }
        Err(err) => {
            eprintln!("Error: {err}");
            status(true)
        }
    }
}

fn parse(filename: &Path) -> process::ExitCode {
    let source = match read_source(filename) {
        Ok(s) => s,
        Err(code) => return code,
    };

    match Parser::new(&source).parse_expression() {
        Ok(token_tree) => {
            println!("{token_tree}");
            status(false)
        }
        Err(err) => {
            // FIXME: match the error line format
            eprintln!("Error: {err}");
            status(true)
        }
    }
}

fn tokenize(filename: &Path) -> process::ExitCode {
    let source = match read_source(filename) {
        Ok(s) => s,
        Err(code) => return code,
    };

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut had_error = false;

    for token in Lexer::new(&source) {
        match token {
            Ok(token) => {
                // Ignoring write errors (e.g. broken pipe) is fine for a CLI.
                let _unused = writeln!(out, "{token}");
            }
            Err(err) => {
                had_error = true;

                match err {
                    Error::SingleToken(e) => {
                        eprintln!("[line {}] Error: Unexpected character: {}", e.line(), e.token);
                    }
                    Error::StringTermination(e) => {
                        eprintln!("[line {}] Error: Unterminated string.", e.line());
                    }
                    Error::ParseNumber(e) => {
                        eprintln!("Error: invalid number literal '{}': {}", e.literal, e.source);
                    }
                    other => eprintln!("Error: {other}"),
                }
            }
        }
    }

    let _unused = writeln!(out, "EOF  null");
    let _unused = out.flush();

    status(had_error)
}

#[repr(u8)]
#[derive(Clone, Copy)]
enum ExitCode {
    Success = 0,
    Usage = 64,        // command line usage error
    InvalidInput = 65, // bad input data
    NoInput = 66,      // input file didn't exist or wasn't readable
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
                    println!("loxxi {}", env!("CARGO_PKG_VERSION"));
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

        let subcommand = subcommand.ok_or("missing command (tokenize|parse|run)")?;

        if subcommand == "help" {
            show_help(None)?;
        }

        let filename = filename.ok_or("missing filename")?;

        let command = match subcommand.as_str() {
            "tokenize" => Commands::Tokenize { filename },
            "parse" => Commands::Parse { filename },
            "run" => Commands::Run { filename },
            other => {
                return Err(lexopt::Error::Custom(
                    format!("unknown command '{other}' (expected tokenize|parse|run)").into(),
                ));
            }
        };

        Ok(Self { command })
    }
}

const USAGE: &str = "\
Usage: loxxi <COMMAND>

Commands:
  tokenize  Print the tokens of FILE
  parse     Parse a single expression from FILE and print its tree
  run       Parse and run FILE
  help      Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
";

const TOKENIZE_USAGE: &str = "\
Print the tokens of FILE

Usage: loxxi tokenize <FILENAME>

Arguments:
  <FILENAME>

Options:
  -h, --help  Print help
";

const PARSE_USAGE: &str = "\
Parse a single expression from FILE and print its tree

Usage: loxxi parse <FILENAME>

Arguments:
  <FILENAME>

Options:
  -h, --help  Print help
";

const RUN_USAGE: &str = "\
Parse and run FILE

Usage: loxxi run <FILENAME>

Arguments:
  <FILENAME>

Options:
  -h, --help  Print help
";

/// Prints help for `target` (or top-level help) and exits 0.
/// It returns an error only for an unrecognized subcommand.
fn show_help(target: Option<&str>) -> Result<!, lexopt::Error> {
    let text = match target {
        Some("tokenize") => TOKENIZE_USAGE,
        Some("parse") => PARSE_USAGE,
        Some("run") => RUN_USAGE,
        Some("help") | None => USAGE,
        Some(other) => {
            return Err(lexopt::Error::Custom(format!("unrecognized subcommand '{other}'").into()));
        }
    };
    print!("{text}");
    process::exit(0)
}
