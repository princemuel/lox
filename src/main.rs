#![allow(clippy::print_stdout, reason = "this project uses logs")]
#![allow(clippy::print_stderr, reason = "this project uses logs")]
#![allow(clippy::use_debug, reason = "this project uses logs")]
#![feature(core_io)]

use core::io::Write as _;
use std::path::{Path, PathBuf};
use std::{fs, io, process};

use clap::{Parser, Subcommand};
use loxxi::error::LexError;
use loxxi::prelude::*;

fn main() -> process::ExitCode {
    let args = Args::parse();

    match args.command {
        Commands::Tokenize { filename } => tokenize(&filename),
    }
}

fn tokenize(filename: &Path) -> process::ExitCode {
    let source = match fs::read_to_string(filename) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("Error: reading '{}' failed: {err}", filename.display());
            return process::ExitCode::from(66); // EX_NOINPUT
        }
    };

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());
    let mut had_error = false;

    for token in Lexer::new(&source) {
        match token {
            Ok(token) => {
                // Buffered write; ignore broken-pipe style errors here is fine
                // for a CLI.
                let _ = writeln!(out, "{token}");
            }
            Err(err) => {
                had_error = true;
                report_lex_error(&err);
            }
        }
    }

    let _ = writeln!(out, "EOF  null");
    let _ = out.flush();

    if had_error { process::ExitCode::from(65) } else { process::ExitCode::SUCCESS }
}

fn report_lex_error(err: &LexError) {
    match err {
        LexError::SingleToken(e) => {
            eprintln!("[line {}] Error: Unexpected character: {}", e.line(), e.token);
        }
        LexError::StringTermination(e) => {
            eprintln!("[line {}] Error: Unterminated string.", e.line());
        }
        LexError::ParseNumber(e) => {
            eprintln!("Error: invalid number literal '{}': {}", e.literal, e.source);
        }
        _ => todo!(),
    }
}

#[derive(Debug, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Tokenize { filename: PathBuf },
}
