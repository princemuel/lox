use alloc::borrow::ToOwned as _;
use alloc::format;
use alloc::string::String;
use core::fmt;
use core::num::ParseFloatError;

use thiserror::Error as ThisError;

/// A lexical error.
#[non_exhaustive]
#[derive(Clone, Debug, ThisError)]
pub enum GlobalError {
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

impl From<GlobalError> for SyntaxError {
    fn from(err: GlobalError) -> Self {
        let (line, message) = match err {
            GlobalError::SingleToken(e) => (e.line, "Unexpected character.".to_owned()),
            GlobalError::StringTermination(e) => (e.line, "Unterminated string.".to_owned()),
            GlobalError::ParseNumber(e) => (e.line, format!("Invalid number '{}'.", e.literal)),
        };
        Self { line, location: Location::Unspecified, message }
    }
}
