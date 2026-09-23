#![allow(clippy::print_stdout, reason = "this project uses logs")]

use core::error::Error;
use std::fs;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use loxxi::Lexer;
use miette::{Context as _, IntoDiagnostic as _};

pub type AnyError = Box<dyn Error + 'static>;

fn main() -> miette::Result<()> {
    let args = Args::parse();

    match args.command {
        Commands::Tokenize { filename } => {
            let source = fs::read_to_string(&filename)
                .into_diagnostic()
                .wrap_err_with(|| format!("reading '{}' failed", filename.display()))?;

            for token in Lexer::new(&source) {
                let token = token?;
                println!("{token}");
            }

            println!("EOF  null");
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
