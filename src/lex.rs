use core::iter::FusedIterator;

use miette::{Diagnostic, Error, LabeledSpan, SourceSpan, miette};

use crate::token::{Token, TokenKind};
pub struct Lexer<'de> {
    rest: &'de str,
    cursor: usize,
    source: &'de str,
}

impl<'de> Lexer<'de> {
    #[must_use]
    pub const fn new(input: &'de str) -> Self { Self { cursor: 0, source: input, rest: input } }
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

    /// once the iterator returns `Error`, it will only return `None`
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut chars = self.rest.chars();

            let ch = chars.next()?;
            let ch_at = self.cursor;
            let ch_str = &self.rest[..ch.len_utf8()];
            let ch_onwards = self.rest;

            self.rest = chars.as_str();
            self.cursor += ch.len_utf8();

            let just = move |kind| Some(Ok(Token { kind, origin: ch_str }));

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
                    return Some(Err(miette!(
                        labels = vec![LabeledSpan::at(
                            self.cursor - c.len_utf8()..self.cursor,
                            "this character"
                        )],
                        "unexpected token '{c}' in input"
                    )
                    .with_source_code(self.source.to_owned())));
                }
            };

            break match started {
                Started::String => todo!(),
                Started::Ident => {
                    let first_non_digit = ch_onwards
                        .find(|c| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_'))
                        .unwrap_or(ch_onwards.len());

                    let mut literal = &ch_onwards[..first_non_digit];
                    let extra_bytes = literal.len() - ch.len_utf8();

                    self.cursor += extra_bytes;
                    self.rest = &self.rest[extra_bytes..];

                    let kind = match literal {
                        "and" => TokenKind::And,
                        "class" => TokenKind::Class,
                        "else" => TokenKind::Else,
                        "false" => TokenKind::False,
                        "for" => TokenKind::For,
                        "fun" => TokenKind::Fun,
                        "if" => TokenKind::If,
                        "nil" => TokenKind::Nil,
                        "or" => TokenKind::Or,
                        "print" => TokenKind::Print,
                        "return" => TokenKind::Return,
                        "super" => TokenKind::Super,
                        "this" => TokenKind::This,
                        "true" => TokenKind::True,
                        "var" => TokenKind::Var,
                        "while" => TokenKind::While,
                        _ => TokenKind::Ident,
                    };

                    return Some(Ok(Token { origin: literal, kind }));
                }
                Started::Slash => todo!(),
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
                        Err(err) => {
                            return Some(Err(miette::miette! {
                                labels = vec![
                                    LabeledSpan::at(self.cursor - literal.len()..self.cursor, "this numeric literal"),
                                ],
                                "{err}",
                            }.with_source_code(self.source.to_owned())));
                        }
                    };

                    return Some(Ok(Token { origin: literal, kind: TokenKind::Number(num) }));
                }

                Started::IfEqualElse(yes, no) => {
                    self.rest = self.rest.trim_start();

                    let trimmed = ch_onwards.len() - self.rest.len() - 1;
                    self.cursor += trimmed;

                    if self.rest.starts_with('=') {
                        let span = &ch_onwards[..=(ch.len_utf8() + trimmed)];
                        self.rest = &self.rest[1..];
                        self.cursor += 1;

                        Some(Ok(Token { origin: span, kind: yes }))
                    } else {
                        Some(Ok(Token { origin: ch_str, kind: no }))
                    }
                }
            };
        }
    }
}

impl FusedIterator for Lexer<'_> {}
