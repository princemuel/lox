#![allow(clippy::print_stdout, reason = "this project uses logs")]
#![allow(clippy::print_stderr, reason = "this project uses logs")]
#![allow(clippy::use_debug, reason = "this project uses logs")]

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use loxxi::prelude::*;
use miette::{Context as _, IntoDiagnostic as _};

fn main() -> miette::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Tokenize { filename } => {
            let mut is_cc_err = false;

            let source = std::fs::read_to_string(&filename)
                .into_diagnostic()
                .wrap_err_with(|| format!("reading '{}' failed", filename.display()))?;

            for token in Lexer::new(&source) {
                let token = match token {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("{e:?}");
                        if let Some(ex) = e.downcast_ref::<SingleTokenError>() {
                            is_cc_err = true;
                            eprintln!(
                                "[line {}] Error: Unexpected character: {}",
                                ex.line(),
                                ex.token
                            );
                        } else if let Some(ex) = e.downcast_ref::<StringTerminationError>() {
                            is_cc_err = true;
                            eprintln!("[line {}] Error: Unterminated string.", ex.line());
                        }
                        continue;
                    }
                };

                println!("{token}");
            }

            println!("EOF  null");

            if is_cc_err {
                std::process::exit(65);
            }
        }
    }

    Ok(())
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
