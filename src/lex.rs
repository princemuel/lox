use miette::{Diagnostic, Error, LabeledSpan, SourceSpan};

use crate::token::{Token, TokenKind};
pub struct Lexer<'de> {
    byte: usize,
    source: &'de str,
    rest: &'de str,
}

impl<'de> Lexer<'de> {
    pub const fn new(input: &'de str) -> Self { Self { byte: 0, source: input, rest: input } }
}

impl<'de> Iterator for Lexer<'de> {
    type Item = Result<Token<'de>, Error>;

    /// once the iterator returns `Error`, it will only return `None`
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut chars = self.rest.chars();

            let ch = chars.next()?;
            let char_at = self.byte;
            let char_str = &self.rest[..ch.len_utf8()];
            let char_onwards = self.rest;

            self.rest = chars.as_str();
            self.byte += ch.len_utf8();

            enum Started {
                Slash,
                String,
                Number,
                Ident,
                IfEqualElse(TokenKind, TokenKind),
            }

            let just = move |kind| Some(Ok(Token { kind, origin: char_str }));

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
                ch => return Some(Err(format!("unexpected token '{ch}' in input").into())),
            };

            break match started {
                Started::String => todo!(),
                Started::Number => todo!(),
                Started::Ident => todo!(),
                Started::IfEqualElse(yes, no) => {
                    self.rest = self.rest.trim_start();

                    let trimmed = char_onwards.len() - self.rest.len() - 1;
                    self.byte += trimmed;

                    if self.rest.starts_with('=') {
                        let span = &char_onwards[..=(ch.len_utf8() + trimmed)];
                        self.rest = &self.rest[1..];
                        self.byte += 1;

                        Some(Ok(Token { origin: span, kind: yes }))
                    } else {
                        Some(Ok(Token { origin: char_str, kind: no }))
                    }
                }
            };
        }
    }
}
