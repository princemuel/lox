use thiserror::Error;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum LexError {
    #[error(transparent)]
    SingleToken(#[from] SingleTokenError),
    #[error(transparent)]
    StringTermination(#[from] StringTerminationError),
    #[error(transparent)]
    ParseNumber(#[from] ParseNumberError),
}

#[derive(Debug, Error)]
#[error("Unexpected EOF")]
pub struct Eof;

#[derive(Debug, Error)]
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

#[derive(Debug, Error)]
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

#[derive(Debug, Error)]
#[error("Invalid number literal '{literal}' at byte {span_start}: {source}")]
pub struct ParseNumberError {
    pub literal: String,
    pub span_start: usize,
    #[source]
    pub source: std::num::ParseFloatError,
}
