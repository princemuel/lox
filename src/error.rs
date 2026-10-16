#![expect(clippy::error_impl_error, reason = "easier to use")]
use core::fmt;
use core::num::ParseFloatError;
use std::io;

use thiserror::Error as ThisError;

use crate::token::Token;

/// A lexical error.
#[non_exhaustive]
#[derive(Clone, Debug, ThisError)]
pub enum Error {
    #[error(transparent)]
    SingleToken(#[from] SingleTokenError),

    #[error(transparent)]
    StringTermination(#[from] StringTerminationError),

    #[error(transparent)]
    ParseNumber(#[from] ParseNumberError),
}

#[derive(Clone, Debug, ThisError)]
#[error("[line {line}] Error: Unterminated string.")]
pub struct SingleTokenError {
    pub token: char,
    pub span_start: usize,
    pub line: usize,
}
impl SingleTokenError {
    #[must_use]
    pub const fn new(token: char, span_start: usize, line: usize) -> Self {
        Self { token, span_start, line }
    }
}

#[derive(Clone, Debug, ThisError)]
#[error("[line {line}] Error: Unterminated string.")]
pub struct StringTerminationError {
    pub span_start: usize,
    pub line: usize,
}
impl StringTerminationError {
    #[must_use]
    pub const fn new(span_start: usize, line: usize) -> Self { Self { span_start, line } }
}

#[derive(Clone, Debug, ThisError)]
#[error("[line {line}] Error: Invalid number literal '{literal}': {source}")]
pub struct ParseNumberError {
    pub literal: String,
    pub span_start: usize,
    pub line: usize,
    #[source]
    pub source: ParseFloatError,
}

impl ParseNumberError {
    #[must_use]
    pub const fn new(
        literal: String,
        span_start: usize,
        line: usize,
        source: ParseFloatError,
    ) -> Self {
        Self { literal, span_start, line, source }
    }
}

/// Where in the source a [`SyntaxError`] points.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    /// No particular token (lexical errors): `[line 3] Error: message`.
    Unspecified,
    /// At a token's text: `[line 3] Error at 'b': message`.
    Token(String),
    /// Ran out of input: `[line 3] Error at end: message`.
    End,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    pub line: usize,
    pub location: Location,
    pub message: String,
}

impl SyntaxError {
    #[must_use]
    pub const fn new(line: usize, location: Location, message: String) -> Self {
        Self { line, location, message }
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self { line, location, message } = self;
        match location {
            Location::Unspecified => write!(f, "[line {line}] Error: {message}"),
            Location::Token(lexeme) => write!(f, "[line {line}] Error at '{lexeme}': {message}"),
            Location::End => write!(f, "[line {line}] Error at end: {message}"),
        }
    }
}

impl core::error::Error for SyntaxError {}

impl From<Error> for SyntaxError {
    fn from(err: Error) -> Self {
        let (line, message) = match err {
            Error::SingleToken(e) => (e.line, "Unexpected character.".to_owned()),
            Error::StringTermination(e) => (e.line, "Unterminated string.".to_owned()),
            Error::ParseNumber(e) => (e.line, format!("Invalid number '{}'.", e.literal)),
        };
        Self { line, location: Location::Unspecified, message }
    }
}

/// A mistake found while running. Printed the way jlox does: the message, then
/// `[line N]` on its own line with exit status 70
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeError {
    pub line: usize,
    pub message: String,
}

impl RuntimeError {
    #[must_use]
    pub fn new(token: &Token<'_>, message: impl Into<String>) -> Self {
        Self { line: token.line, message: message.into() }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}\n[line {}]", self.message, self.line)
    }
}

impl core::error::Error for RuntimeError {}

/// Anything that stops a running program.
#[derive(Debug)]
pub enum Failure {
    /// The Lox program did something illegal.
    Runtime(RuntimeError),
    /// The interpreter could not write the program's output (closed pipe, full
    /// disk).
    Output(io::Error),
}

impl From<RuntimeError> for Failure {
    fn from(err: RuntimeError) -> Self { Self::Runtime(err) }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(e) => write!(f, "{e}"),
            Self::Output(e) => write!(f, "Error: writing output failed: {e}"),
        }
    }
}

impl core::error::Error for Failure {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::Runtime(e) => Some(e),
            Self::Output(e) => Some(e),
        }
    }
}
