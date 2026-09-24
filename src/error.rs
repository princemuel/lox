use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Debug, Diagnostic, Error)]
#[error("Unexpected EOF")]
pub struct Eof;

#[derive(Debug, Diagnostic, Error)]
#[error("Unexpected token '{token}'")]
pub struct SingleTokenError {
    #[source_code]
    pub src: String,

    pub token: char,

    #[label = "this input character"]
    pub err_span: SourceSpan,
}

impl SingleTokenError {
    pub fn line(&self) -> usize {
        let until_unrecongized = &self.src[..=self.err_span.offset()];
        until_unrecongized.lines().count()
    }
}

#[derive(Debug, Diagnostic, Error)]
#[error("Unterminated string")]
pub struct StringTerminationError {
    #[source_code]
    pub src: String,

    #[label = "this string literal"]
    pub err_span: SourceSpan,
}

impl StringTerminationError {
    pub fn line(&self) -> usize {
        let until_unrecongized = &self.src[..=self.err_span.offset()];
        until_unrecongized.lines().count()
    }
}
