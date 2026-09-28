use thiserror::Error as ThisError;

#[non_exhaustive]
#[derive(Debug, ThisError)]
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
        context: &'static str,
        #[source]
        source: Box<Self>,
    },
}

impl Error {
    #[must_use]
    pub fn context(self, context: &'static str) -> Self {
        Self::Context { context, source: Box::new(self) }
    }
}

pub trait ResultExt<T> {
    fn context(self, context: &'static str) -> Result<T, Error>;
}

impl<T, E: Into<Error>> ResultExt<T> for Result<T, E> {
    fn context(self, context: &'static str) -> Result<T, Error> {
        self.map_err(|e| e.into().context(context))
    }
}

#[derive(Debug, ThisError)]
#[error("{message} at byte {span_start}")]
pub struct UnexpectedTokenError {
    pub src: String,
    pub message: String,
    pub found: String, // Debug rendering of the offending token
    pub span_start: usize,
    pub span_len: usize,
}

impl UnexpectedTokenError {
    #[must_use]
    pub fn line(&self) -> usize { self.src[..=self.span_start].lines().count() }
}

#[derive(Debug, ThisError)]
#[error("Unexpected EOF")]
pub struct Eof;

#[derive(Debug, ThisError)]
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
    pub fn line(&self) -> usize { self.src[..=self.span_start].lines().count() }
}

#[derive(Debug, ThisError)]
#[error("Unterminated string starting at byte {span_start}")]
pub struct StringTerminationError {
    pub src: String,
    pub span_start: usize,
}

impl StringTerminationError {
    #[must_use]
    pub const fn new(src: String, span_start: usize) -> Self { Self { src, span_start } }

    #[must_use]
    pub fn line(&self) -> usize { self.src[..=self.span_start].lines().count() }
}

#[derive(Debug, ThisError)]
#[error("Invalid number literal '{literal}' at byte {span_start}: {source}")]
pub struct ParseNumberError {
    pub literal: String,
    pub span_start: usize,
    #[source]
    pub source: core::num::ParseFloatError,
}
