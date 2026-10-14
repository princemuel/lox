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
}

enum Started {
    Slash,
    String,
    Number,
    Ident,
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
            let mut chars = self.rest.chars();

            let ch = chars.next()?;
            let ch_at = self.cursor;
            let ch_str = &self.rest[..ch.len_utf8()];
            let ch_onwards = self.rest;

            self.rest = chars.as_str();
            self.cursor += ch.len_utf8();

            let line = self.line;
            let just = move |kind| Some(Ok(Token { kind, offset: ch_at, origin: ch_str, line }));

            let started = match ch {
                '(' => return just(TokenKind::LeftParen),
                ')' => return just(TokenKind::RightParen),
                '{' => return just(TokenKind::LeftBrace),
                '}' => return just(TokenKind::RightBrace),
                ',' => return just(TokenKind::Comma),
                '.' => return just(TokenKind::Dot),
                '-' => return just(TokenKind::Minus),
                '+' => return just(TokenKind::Plus),
                ';' => return just(TokenKind::Semicolon),
                '*' => return just(TokenKind::Star),

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
                Started::String => {
                    if let Some(end) = self.rest.find('"') {
                        let literal = &ch_onwards[..=(end + 1)];
                        self.cursor += end + 1;
                        self.rest = &self.rest[end + 1..];
                        self.line += literal.matches('\n').count();

                        Some(Ok(Token {
                            origin: literal,
                            offset: ch_at,
                            kind: TokenKind::String,
                            line: self.line,
                        }))
                    } else {
                        // swallow the remainder of input as being a string
                        self.line += self.rest.matches('\n').count();
                        let err = StringTerminationError::new(ch_at, self.line);

                        self.cursor += self.rest.len();
                        self.rest = &self.rest[self.rest.len()..];

                        return Some(Err(err.into()));
                    }
                }

                Started::Slash => {
                    if self.rest.starts_with('/') {
                        let line_end = self.rest.find('\n').unwrap_or(self.rest.len());
                        self.cursor += line_end;
                        self.rest = &self.rest[line_end..];
                        continue;
                    }

                    Some(Ok(Token { origin: ch_str, offset: ch_at, kind: TokenKind::Slash, line }))
                }

                Started::Ident => {
                    let first_non_digit = ch_onwards
                        .find(|c| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_'))
                        .unwrap_or(ch_onwards.len());

                    let literal = &ch_onwards[..first_non_digit];
                    let extra_bytes = literal.len() - ch.len_utf8();

                    self.cursor += extra_bytes;
                    self.rest = &self.rest[extra_bytes..];

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

                    return Some(Ok(Token { origin: literal, offset: ch_at, kind, line }));
                }

                Started::Number => {
                    // digits, then an optional `.` that must be followed by a
                    // digit
                    let digits = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
                    let mut len = digits(ch_onwards);
                    if let Some(fractional_part) = ch_onwards[len..].strip_prefix('.') {
                        let n = digits(fractional_part);
                        if n > 0 {
                            len += 1 + n;
                        }
                    }

                    let literal = &ch_onwards[..len];

                    let extra_bytes = literal.len() - ch.len_utf8();
                    self.cursor += extra_bytes;
                    self.rest = &self.rest[extra_bytes..];

                    let num = match literal.parse() {
                        Ok(num) => num,
                        Err(source) => {
                            let err =
                                ParseNumberError::new(literal.to_owned(), ch_at, line, source);

                            return Some(Err(err.into()));
                        }
                    };

                    return Some(Ok(Token {
                        origin: literal,
                        offset: ch_at,
                        kind: TokenKind::Number(num),
                        line,
                    }));
                }

                Started::IfEqualElse(r#true, r#false) => {
                    if self.rest.starts_with('=') {
                        let span = &ch_onwards[..=ch.len_utf8()];
                        self.rest = &self.rest[1..];
                        self.cursor += 1;

                        Some(Ok(Token { origin: span, offset: ch_at, kind: r#true, line }))
                    } else {
                        Some(Ok(Token { origin: ch_str, offset: ch_at, kind: r#false, line }))
                    }
                }
            };
        }
    }
}

impl FusedIterator for Scanner<'_> {}
