use alloc::borrow::ToOwned as _;
use core::iter::FusedIterator;

use crate::error::{GlobalError, ParseNumberError, SingleTokenError, StringTerminationError};
use crate::token::{Token, TokenKind};

pub struct Scanner<'de> {
    rest: &'de str,
    cursor: usize,
    line: usize,
    peeked: Option<Result<Token<'de>, GlobalError>>,
}

impl<'de> Scanner<'de> {
    #[must_use]
    pub const fn new(input: &'de str) -> Self {
        Self { cursor: 0, line: 1, rest: input, peeked: None }
    }

    #[must_use]
    pub const fn line(&self) -> usize { self.line }

    pub fn peek(&mut self) -> Option<&Result<Token<'de>, GlobalError>> {
        if self.peeked.is_some() {
            return self.peeked.as_ref();
        }

        self.peeked = self.next();
        self.peeked.as_ref()
    }

    /// Consumes the next `bytes` bytes of input.
    fn advance(&mut self, bytes: usize) {
        self.cursor += bytes;
        self.rest = &self.rest[bytes..];
    }

    /// Finishes a string whose opening quote, found at `at`, was just consumed.
    /// `from_quote` is the input starting at that quote.
    fn string(&mut self, from_quote: &'de str, at: usize) -> Result<Token<'de>, GlobalError> {
        // `end` is the closing quote's offset, counted from just after the
        // opening one.
        if let Some(end) = self.rest.find('"') {
            let literal = &from_quote[..=(end + 1)];
            self.advance(end + 1);
            self.line += literal.matches('\n').count();

            Ok(Token { origin: literal, offset: at, kind: TokenKind::String, line: self.line })
        } else {
            // swallow the remainder of input as being a string
            self.line += self.rest.matches('\n').count();
            let err = StringTerminationError::new(at, self.line);
            self.advance(self.rest.len());

            Err(err.into())
        }
    }

    /// Finishes a number whose first digit, found at `at` on `line`, was just
    /// consumed. `from_digit` is the input starting at that digit.
    fn number(
        &mut self,
        from_digit: &'de str,
        at: usize,
        line: usize,
    ) -> Result<Token<'de>, GlobalError> {
        // digits, then an optional `.` that must be followed by a digit
        let digits = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let mut len = digits(from_digit);
        if let Some(fractional_part) = from_digit[len..].strip_prefix('.') {
            let n = digits(fractional_part);
            if n > 0 {
                len += 1 + n;
            }
        }

        let literal = &from_digit[..len];
        // the first digit (one byte) was already consumed
        self.advance(literal.len() - 1);

        match literal.parse() {
            Ok(num) => {
                Ok(Token { origin: literal, offset: at, kind: TokenKind::Number(num), line })
            }
            Err(source) => Err(ParseNumberError::new(literal.to_owned(), at, line, source).into()),
        }
    }
}

enum Started {
    Slash,
    String,
    Number,
    Ident,
    /// `<`, `>`, `!` and `=`: the first kind if an `=` follows, the second if
    /// not.
    IfEqualElse(TokenKind, TokenKind),
}

impl<'de> Iterator for Scanner<'de> {
    type Item = Result<Token<'de>, GlobalError>;

    #[expect(clippy::too_many_lines)]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.peeked.take() {
            return Some(next);
        }

        loop {
            let ch_onwards = self.rest;
            let ch = ch_onwards.chars().next()?;
            let ch_at = self.cursor;
            let ch_str = &ch_onwards[..ch.len_utf8()];
            self.advance(ch.len_utf8());

            let line = self.line;
            let emit = move |kind| Some(Ok(Token { kind, offset: ch_at, origin: ch_str, line }));

            let started = match ch {
                '(' => return emit(TokenKind::LeftParen),
                ')' => return emit(TokenKind::RightParen),
                '{' => return emit(TokenKind::LeftBrace),
                '}' => return emit(TokenKind::RightBrace),
                ',' => return emit(TokenKind::Comma),
                '.' => return emit(TokenKind::Dot),
                '-' => return emit(TokenKind::Minus),
                '+' => return emit(TokenKind::Plus),
                ';' => return emit(TokenKind::Semicolon),
                '*' => return emit(TokenKind::Star),

                '/' => Started::Slash,
                '<' => Started::IfEqualElse(TokenKind::LessEqual, TokenKind::Less),
                '>' => Started::IfEqualElse(TokenKind::GreaterEqual, TokenKind::Greater),
                '!' => Started::IfEqualElse(TokenKind::BangEqual, TokenKind::Bang),
                '=' => Started::IfEqualElse(TokenKind::EqualEqual, TokenKind::Equal),
                '"' => Started::String,
                '0'..='9' => Started::Number,
                'a'..='z' | 'A'..='Z' | '_' => Started::Ident,

                // whitespace in Lox is exactly these four (not Unicode `is_whitespace`).
                '\n' => {
                    self.line += 1;
                    continue;
                }
                ' ' | '\r' | '\t' => continue,

                c => return Some(Err(SingleTokenError::new(c, ch_at, line).into())),
            };

            break match started {
                Started::String => Some(self.string(ch_onwards, ch_at)),

                Started::Slash => {
                    if self.rest.starts_with('/') {
                        // a comment runs up to, but not including, the newline
                        let line_end = self.rest.find('\n').unwrap_or(self.rest.len());
                        self.advance(line_end);
                        continue;
                    }

                    emit(TokenKind::Slash)
                }

                Started::Ident => {
                    let end = ch_onwards
                        .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .unwrap_or(ch_onwards.len());

                    let literal = &ch_onwards[..end];
                    self.advance(literal.len() - ch.len_utf8());

                    let kind = match literal {
                        "and" => TokenKind::And,
                        "or" => TokenKind::Or,
                        "if" => TokenKind::If,
                        "else" => TokenKind::Else,
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "class" => TokenKind::Class,
                        "fun" => TokenKind::Fun,
                        "this" => TokenKind::This,
                        "super" => TokenKind::Super,
                        "var" => TokenKind::Var,
                        "nil" => TokenKind::Nil,
                        "return" => TokenKind::Return,
                        "print" => TokenKind::Print,
                        "for" => TokenKind::For,
                        "while" => TokenKind::While,
                        _ => TokenKind::Ident,
                    };

                    Some(Ok(Token { origin: literal, offset: ch_at, kind, line }))
                }

                Started::Number => Some(self.number(ch_onwards, ch_at, line)),

                Started::IfEqualElse(with_eq, without_eq) => {
                    if self.rest.starts_with('=') {
                        let span = &ch_onwards[..=ch.len_utf8()];
                        self.advance(1);

                        Some(Ok(Token { origin: span, offset: ch_at, kind: with_eq, line }))
                    } else {
                        emit(without_eq)
                    }
                }
            };
        }
    }
}

impl FusedIterator for Scanner<'_> {}
