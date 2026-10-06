use alloc::borrow::Cow;
use core::fmt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Token<'de> {
    pub kind: TokenKind,
    pub origin: &'de str,
    pub offset: usize,
    pub line: usize,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TokenKind {
    /// `(`
    LeftParen,
    /// `)`
    RightParen,
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `-`
    Minus,
    /// `+`
    Plus,
    /// `;`
    Semicolon,
    /// `*`
    Star,
    /// `/`
    Slash,

    /// `!`
    Bang,
    /// `!=`
    BangEqual,
    /// `=`
    Equal,
    /// `==`
    EqualEqual,
    /// `>`
    Greater,
    /// `>=`
    GreaterEqual,
    /// `<`
    Less,
    /// `<=`
    LessEqual,

    String,
    Ident,
    Number(f64),

    /// `and`
    And,
    /// `or`
    Or,
    /// `if`
    If,
    /// `else`
    Else,
    /// `true`
    True,
    /// `false`
    False,
    /// `fun`
    Fun,
    /// `class`
    Class,
    /// `nil`
    Nil,
    /// `print`
    Print,
    /// `return`
    Return,
    /// `this`
    This,
    /// `super`
    Super,
    /// `var`
    Var,
    /// `for`
    For,
    /// `while`
    While,
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let origin = self.origin;

        match self.kind {
            TokenKind::LeftParen => write!(f, "LEFT_PAREN {origin} null"),
            TokenKind::RightParen => write!(f, "RIGHT_PAREN {origin} null"),
            TokenKind::LeftBrace => write!(f, "LEFT_BRACE {origin} null"),
            TokenKind::RightBrace => write!(f, "RIGHT_BRACE {origin} null"),
            TokenKind::Comma => write!(f, "COMMA {origin} null"),
            TokenKind::Dot => write!(f, "DOT {origin} null"),
            TokenKind::Minus => write!(f, "MINUS {origin} null"),
            TokenKind::Plus => write!(f, "PLUS {origin} null"),
            TokenKind::Semicolon => write!(f, "SEMICOLON {origin} null"),
            TokenKind::Slash => write!(f, "SLASH {origin} null"),
            TokenKind::Star => write!(f, "STAR {origin} null"),

            TokenKind::Bang => write!(f, "BANG {origin} null"),
            TokenKind::BangEqual => write!(f, "BANG_EQUAL {origin} null"),
            TokenKind::Equal => write!(f, "EQUAL {origin} null"),
            TokenKind::EqualEqual => write!(f, "EQUAL_EQUAL {origin} null"),
            TokenKind::Greater => write!(f, "GREATER {origin} null"),
            TokenKind::GreaterEqual => write!(f, "GREATER_EQUAL {origin} null"),
            TokenKind::Less => write!(f, "LESS {origin} null"),
            TokenKind::LessEqual => write!(f, "LESS_EQUAL {origin} null"),

            TokenKind::Identifier => write!(f, "IDENTIFIER {origin} null"),
            TokenKind::String => {
                write!(f, "STRING {origin} {}", Cow::Borrowed(origin.trim_matches('"')))
            }
            // tests require that integers are printed as N.0
            TokenKind::Number(v) if v.fract() == 0.0 => write!(f, "NUMBER {origin} {v}.0"),
            TokenKind::Number(v) => write!(f, "NUMBER {origin} {v}"),

            TokenKind::And => write!(f, "AND {origin} null"),
            TokenKind::Or => write!(f, "OR {origin} null"),
            TokenKind::If => write!(f, "IF {origin} null"),
            TokenKind::Else => write!(f, "ELSE {origin} null"),
            TokenKind::True => write!(f, "TRUE {origin} null"),
            TokenKind::False => write!(f, "FALSE {origin} null"),
            TokenKind::For => write!(f, "FOR {origin} null"),
            TokenKind::While => write!(f, "WHILE {origin} null"),
            TokenKind::Class => write!(f, "CLASS {origin} null"),
            TokenKind::Fun => write!(f, "FUN {origin} null"),
            TokenKind::Print => write!(f, "PRINT {origin} null"),
            TokenKind::Return => write!(f, "RETURN {origin} null"),
            TokenKind::This => write!(f, "THIS {origin} null"),
            TokenKind::Super => write!(f, "SUPER {origin} null"),
            TokenKind::Var => write!(f, "VAR {origin} null"),
            TokenKind::Nil => write!(f, "NIL {origin} null"),
        }
    }
}
