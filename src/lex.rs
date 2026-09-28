use core::iter::FusedIterator;

use crate::error::{
    Eof,
    Error,
    ParseNumberError,
    SingleTokenError,
    StringTerminationError,
    UnexpectedTokenError,
};
use crate::token::{Token, TokenKind};
pub struct Lexer<'de> {
    rest: &'de str,
    cursor: usize,
    source: &'de str,
    peeked: Option<Result<Token<'de>, Error>>,
}

impl<'de> Lexer<'de> {
    #[must_use]
    pub const fn new(input: &'de str) -> Self {
        Self { cursor: 0, source: input, rest: input, peeked: None }
    }

    pub fn expect(&mut self, expected: TokenKind, message: &str) -> Result<Token<'de>, Error> {
        self.expect_where(|next| next.kind == expected, message)
    }

    #[expect(clippy::missing_errors_doc)]
    pub fn expect_where(
        &mut self,
        mut predicate: impl FnMut(&Token<'de>) -> bool,
        message: &str,
    ) -> Result<Token<'de>, Error> {
        match self.next() {
            Some(Ok(token)) if predicate(&token) => Ok(token),
            Some(Ok(token)) => Err(UnexpectedTokenError {
                src: self.source.to_owned(),
                message: message.to_owned(),
                found: format!("{token:?}"),
                span_start: token.offset,
                span_len: token.origin.len(),
            }
            .into()),
            Some(Err(e)) => Err(e),
            None => Err(Eof.into()),
        }
    }

    pub fn peek(&mut self) -> Option<&Result<Token<'de>, Error>> {
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

impl<'de> Iterator for Lexer<'de> {
    type Item = Result<Token<'de>, Error>;

    #[expect(clippy::too_many_lines)]
    /// once the iterator returns `Error`, it will only return `None`
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

            let just = move |kind| Some(Ok(Token { kind, offset: ch_at, origin: ch_str }));

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

                c if c.is_whitespace() => continue,
                c => {
                    return Some(Err(SingleTokenError::new(
                        self.source.to_owned(),
                        c,
                        self.cursor - c.len_utf8(),
                    )
                    .into()));
                }
            };

            break match started {
                Started::String => {
                    if let Some(end) = self.rest.find('"') {
                        let literal = &ch_onwards[..end + 1 + 1];
                        self.cursor += end + 1;
                        self.rest = &self.rest[end + 1..];
                        Some(Ok(Token { origin: literal, offset: ch_at, kind: TokenKind::String }))
                    } else {
                        let err = StringTerminationError::new(
                            self.source.to_owned(),
                            self.cursor - ch.len_utf8(),
                        );

                        // swallow the remainder of input as being a string
                        self.cursor += self.rest.len();
                        self.rest = &self.rest[self.rest.len()..];

                        return Some(Err(err.into()));
                    }
                }

                Started::Slash => {
                    if self.rest.starts_with('/') {
                        let line_end = self.rest.find('\n').unwrap_or_else(|| self.rest.len());
                        self.cursor += line_end;
                        self.rest = &self.rest[line_end..];
                        continue;
                    }

                    Some(Ok(Token { origin: ch_str, offset: ch_at, kind: TokenKind::Slash }))
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

                    return Some(Ok(Token { origin: literal, offset: ch_at, kind }));
                }

                Started::Number => {
                    let first_non_digit = ch_onwards
                        .find(|ch| !matches!(ch, '.' | '0'..='9'))
                        .unwrap_or(ch_onwards.len());

                    let mut literal = &ch_onwards[..first_non_digit];
                    let mut dotted = literal.splitn(3, '.');

                    match (dotted.next(), dotted.next(), dotted.next()) {
                        (Some(a), Some(b), Some(_)) => {
                            literal = &literal[..=(a.len() + b.len())];
                        }
                        (Some(a), Some(""), None) => {
                            literal = &literal[..a.len()];
                        }
                        // leave literal as-is
                        _ => {}
                    }

                    let extra_bytes = literal.len() - ch.len_utf8();
                    self.cursor += extra_bytes;
                    self.rest = &self.rest[extra_bytes..];

                    let num = match literal.parse() {
                        Ok(num) => num,
                        Err(source) => {
                            return Some(Err(ParseNumberError {
                                literal: literal.to_owned(),
                                span_start: self.cursor - literal.len(),
                                source,
                            }
                            .into()));
                        }
                    };

                    return Some(Ok(Token {
                        origin: literal,
                        offset: ch_at,
                        kind: TokenKind::Number(num),
                    }));
                }

                Started::IfEqualElse(yes, no) => {
                    self.rest = self.rest.trim_start();

                    let trimmed = ch_onwards.len() - self.rest.len() - 1;
                    self.cursor += trimmed;

                    if self.rest.starts_with('=') {
                        let span = &ch_onwards[..=(ch.len_utf8() + trimmed)];
                        self.rest = &self.rest[1..];
                        self.cursor += 1;

                        Some(Ok(Token { origin: span, offset: ch_at, kind: yes }))
                    } else {
                        Some(Ok(Token { origin: ch_str, offset: ch_at, kind: no }))
                    }
                }
            };
        }
    }
}

impl FusedIterator for Lexer<'_> {}
