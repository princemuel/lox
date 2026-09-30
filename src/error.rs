use alloc::borrow::{Cow, ToOwned as _};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;

use thiserror::Error as ThisError;

#[non_exhaustive]
#[derive(Clone, Debug, ThisError)]
pub enum Error {
    #[error(transparent)]
    SingleToken(#[from] SingleTokenError),
    #[error(transparent)]
    StringTermination(#[from] StringTerminationError),
    #[error(transparent)]
    ParseNumber(#[from] ParseNumberError),
    #[error(transparent)]
    UnexpectedToken(#[from] UnexpectedTokenError),
    #[error(transparent)]
    EndOfFile(#[from] Eof),

    #[error("{context}")]
    Context {
        context: Cow<'static, str>,
        #[source]
        source: Box<Self>,
    },
}

impl Error {
    #[must_use]
    pub fn context(self, context: impl Into<Cow<'static, str>>) -> Self {
        Self::Context { context: context.into(), source: Box::new(self) }
    }
}

pub trait ResultExt<T> {
    /// # Errors
    ///
    /// Returns `Err` with the original error wrapped in [`Error::Context`] when
    /// `self` is an error.
    fn context(self, context: impl Into<Cow<'static, str>>) -> Result<T, Error>;

    /// Lazily adds context to an error.
    ///
    /// The context-producing closure is only evaluated when the result is
    /// an error.
    ///
    /// # Errors
    ///
    /// Returns the original error wrapped with the context produced by `f`.
    fn with_context<C: Into<Cow<'static, str>>>(self, f: impl FnOnce() -> C) -> Result<T, Error>;
}

impl<T, E: Into<Error>> ResultExt<T> for Result<T, E> {
    fn context(self, context: impl Into<Cow<'static, str>>) -> Result<T, Error> {
        self.map_err(|e| e.into().context(context))
    }

    fn with_context<C: Into<Cow<'static, str>>>(self, f: impl FnOnce() -> C) -> Result<T, Error> {
        self.map_err(|e| e.into().context(f()))
    }
}

#[derive(Clone, Debug, ThisError)]
#[error("{message} at byte {span_start}")]
pub struct UnexpectedTokenError {
    pub src: String,
    pub message: String,
    pub found: String,
    pub span_start: usize,
    pub span_len: usize,
}

impl UnexpectedTokenError {
    #[must_use]
    pub fn new(
        src: &str,
        msg: &str,
        found: impl core::fmt::Debug,
        offset: usize,
        len: usize,
    ) -> Self {
        Self {
            src: src.to_owned(),
            message: msg.to_owned(),
            found: format!("{found:?}"),
            span_start: offset,
            span_len: len,
        }
    }

    #[must_use]
    pub fn line(&self) -> usize { line_of(&self.src, self.span_start) }
}

#[derive(Clone, Debug, ThisError)]
#[error("Unexpected EOF")]
pub struct Eof;

#[derive(Clone, Debug, ThisError)]
#[error("Unexpected token '{token}' at byte {span_start}")]
pub struct SingleTokenError {
    pub src: String,
    pub token: char,
    pub span_start: usize,
}

impl SingleTokenError {
    #[must_use]
    pub const fn new(src: String, token: char, span_start: usize) -> Self {
        Self { src, token, span_start }
    }

    #[must_use]
    pub fn line(&self) -> usize { line_of(&self.src, self.span_start) }
}

#[derive(Clone, Debug, ThisError)]
#[error("Unterminated string starting at byte {span_start}")]
pub struct StringTerminationError {
    pub src: String,
    pub span_start: usize,
}

impl StringTerminationError {
    #[must_use]
    pub const fn new(src: String, span_start: usize) -> Self { Self { src, span_start } }

    #[must_use]
    pub fn line(&self) -> usize { line_of(&self.src, self.span_start) }
}

#[derive(Clone, Debug, ThisError)]
#[error("Invalid number literal '{literal}' at byte {span_start}: {source}")]
pub struct ParseNumberError {
    pub literal: String,
    pub span_start: usize,
    #[source]
    pub source: core::num::ParseFloatError,
}

/// 1-based line of a byte offset (counts `\n`s before it, like jlox/clox).
/// Never panics: `offset` is always a char boundary, and we only look at bytes.
#[expect(clippy::naive_bytecount)]
fn line_of(src: &str, offset: usize) -> usize {
    src.as_bytes()[..offset.min(src.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}
