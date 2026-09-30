//! Parser: recursive descent for statements, Pratt for expressions.
//!
//! Grammar:
//!
//! ```text
//! program     → declaration* EOF
//! declaration → classDecl | funDecl | varDecl | statement
//! classDecl   → "class" IDENT ( "<" IDENT )? "{" function* "}"
//! funDecl     → "fun" function
//! function    → IDENT "(" parameters? ")" block
//! varDecl     → "var" IDENT ( "=" expression )? ";"
//! statement   → exprStmt | forStmt | ifStmt | printStmt | returnStmt | whileStmt | block
//! forStmt     → "for" "(" ( varDecl | exprStmt | ";" ) expression? ";" expression? ")" statement
//! ifStmt      → "if" "(" expression ")" statement ( "else" statement )?
//! returnStmt  → "return" expression? ";"
//! whileStmt   → "while" "(" expression ")" statement
//! block       → "{" declaration* "}"
//! ```

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::{format, vec};

use crate::ast::{BinaryOp, Expr, Function, Literal, LogicalOp, Stmt, UnaryOp};
use crate::error::{Eof, Error, UnexpectedTokenError};
use crate::lex::Lexer;
use crate::token::{Token, TokenKind};

/// Lox caps parameters and arguments at 255.
const MAX_ARITY: usize = 255;

/// Prefix `!` and `-` bind tighter than every infix operator, and
/// looser than call/property access (so `-a.b` is `-(a.b)`).
const UNARY_BP: u8 = 15;

#[derive(Clone, Copy)]
struct BindingPower {
    left: u8,
    right: u8,
}
impl BindingPower {
    const fn new(left: u8, right: u8) -> Self { Self { left, right } }
}

/// What an infix-position token means. Call and `.` are "postfix": they only
/// have a left operand, so their right binding power is unused.
#[derive(Clone, Copy)]
enum Infix {
    Assign,
    Logical(LogicalOp),
    Binary(BinaryOp),
    Call,
    Get,
}
impl Infix {
    /// `(meaning, left binding power, right binding power)`.
    ///
    /// # Guide
    ///
    /// A *bigger* number binds *tighter*.
    ///
    /// `lhs < rhs` is left-associative: (`1 - 2 - 3` is `(1 - 2) - 3`)
    ///
    /// `lhs > rhs` is right-associative: (`a = b = c` is `a = (b = c)`)
    const fn from_token(kind: TokenKind) -> Option<(Self, BindingPower)> {
        Some(match kind {
            TokenKind::Equal => (Self::Assign, BindingPower::new(2, 1)),
            TokenKind::Or => (Self::Logical(LogicalOp::Or), BindingPower::new(3, 4)),
            TokenKind::And => (Self::Logical(LogicalOp::And), BindingPower::new(5, 6)),
            TokenKind::EqualEqual => (Self::Binary(BinaryOp::Equal), BindingPower::new(7, 8)),
            TokenKind::BangEqual => (Self::Binary(BinaryOp::NotEqual), BindingPower::new(7, 8)),
            TokenKind::Less => (Self::Binary(BinaryOp::Less), BindingPower::new(9, 10)),
            TokenKind::LessEqual => (Self::Binary(BinaryOp::LessEqual), BindingPower::new(9, 10)),
            TokenKind::Greater => (Self::Binary(BinaryOp::Greater), BindingPower::new(9, 10)),
            TokenKind::GreaterEqual => {
                (Self::Binary(BinaryOp::GreaterEqual), BindingPower::new(9, 10))
            }
            TokenKind::Plus => (Self::Binary(BinaryOp::Add), BindingPower::new(11, 12)),
            TokenKind::Minus => (Self::Binary(BinaryOp::Subtract), BindingPower::new(11, 12)),
            TokenKind::Star => (Self::Binary(BinaryOp::Multiply), BindingPower::new(13, 14)),
            TokenKind::Slash => (Self::Binary(BinaryOp::Divide), BindingPower::new(13, 14)),
            TokenKind::LeftParen => (Self::Call, BindingPower::new(17, 0)),
            TokenKind::Dot => (Self::Get, BindingPower::new(17, 0)),
            _ => return None,
        })
    }
}

pub struct Parser<'de> {
    source: &'de str,
    lexer: Lexer<'de>,
}
impl<'de> Parser<'de> {
    #[must_use]
    pub const fn new(input: &'de str) -> Self { Self { source: input, lexer: Lexer::new(input) } }

    /// Parses a whole program: declarations until end of input.
    pub fn parse_program(mut self) -> Result<Vec<Stmt<'de>>, Error> {
        let mut program = Vec::new();
        while self.peek()?.is_some() {
            program.push(self.declaration()?);
        }
        Ok(program)
    }

    /// Parses exactly one expression (the `parse` subcommand).
    pub fn parse_expression(mut self) -> Result<Expr<'de>, Error> {
        let expr = self.expression()?;
        match self.peek()? {
            Some(extra) => Err(self.error_at(extra, "Expect end of expression.")),
            None => Ok(expr),
        }
    }

    /// Looks at the next token without consuming it. `Ok(None)` means `EOF`.
    fn peek(&mut self) -> Result<Option<Token<'de>>, Error> {
        match self.lexer.peek() {
            None => Ok(None),
            Some(Ok(token)) => Ok(Some(*token)),
            Some(Err(err)) => Err(err.clone()),
        }
    }

    /// Consumes and returns the next token.
    fn advance(&mut self) -> Result<Option<Token<'de>>, Error> { self.lexer.next().transpose() }

    /// Consumes the next token, discarding it
    fn bump(&mut self) -> Result<(), Error> { self.advance().map(|_| ()) }

    fn check(&mut self, kind: TokenKind) -> Result<bool, Error> {
        Ok(self.peek()?.is_some_and(|token| token.kind == kind))
    }

    /// Consumes the next token only if it is `kind`
    fn eat_if(&mut self, kind: TokenKind) -> Result<Option<Token<'de>>, Error> {
        if self.check(kind)? { self.advance() } else { Ok(None) }
    }

    /// Like [`Parser::consume`] for when the token itself isn't needed (`)`,
    /// `;`, ...).
    fn expect(&mut self, kind: TokenKind, msg: &str) -> Result<(), Error> {
        self.lexer.eat(kind, msg)
    }

    /// Consumes a token that *must* be `kind`, or fails with `msg`.
    fn consume(&mut self, kind: TokenKind, msg: &str) -> Result<Token<'de>, Error> {
        self.lexer.expect(kind, msg)
    }

    fn error_at(&self, token: Token<'de>, msg: &str) -> Error {
        UnexpectedTokenError::new(self.source, msg, token, token.offset, token.origin.len()).into()
    }

    /// Error located at whatever token comes next (or end of input).
    fn error_here(&mut self, msg: &str) -> Error {
        match self.peek() {
            Ok(Some(token)) => self.error_at(token, msg),
            Ok(None) => Eof.into(),
            Err(err) => err,
        }
    }

    fn declaration(&mut self) -> Result<Stmt<'de>, Error> {
        if self.eat_if(TokenKind::Class)?.is_some() {
            self.class_declaration()
        } else if self.eat_if(TokenKind::Fun)?.is_some() {
            self.function("function").map(Stmt::Function)
        } else if self.eat_if(TokenKind::Var)?.is_some() {
            self.var_declaration()
        } else {
            self.statement()
        }
    }

    fn class_declaration(&mut self) -> Result<Stmt<'de>, Error> {
        let name = self.consume(TokenKind::Ident, "Expect class name.")?;

        let superclass = if self.eat_if(TokenKind::Less)?.is_some() {
            let parent = self.consume(TokenKind::Ident, "Expect superclass name.")?;
            Some(Expr::Variable(parent))
        } else {
            None
        };

        self.expect(TokenKind::LeftBrace, "Expect '{' before class body.")?;

        let mut methods = Vec::new();
        while !self.check(TokenKind::RightBrace)? && self.peek()?.is_some() {
            methods.push(self.function("method")?);
        }

        self.expect(TokenKind::RightBrace, "Expect '}' after class body.")?;

        Ok(Stmt::Class { name, superclass, methods })
    }

    /// `kind` is "function" or "method"; it only affects error messages.
    fn function(&mut self, kind: &str) -> Result<Function<'de>, Error> {
        let name = self.consume(TokenKind::Ident, &format!("Expect {kind} name."))?;
        self.expect(TokenKind::LeftParen, &format!("Expect '(' after {kind} name."))?;

        let mut params = Vec::new();
        if !self.check(TokenKind::RightParen)? {
            loop {
                if params.len() >= MAX_ARITY {
                    return Err(self.error_here("Can't have more than 255 parameters."));
                }
                params.push(self.consume(TokenKind::Ident, "Expect parameter name.")?);

                if self.eat_if(TokenKind::Comma)?.is_none() {
                    break;
                }
            }
        }

        self.expect(TokenKind::RightParen, "Expect ')' after parameters.")?;

        self.expect(TokenKind::LeftBrace, &format!("Expect '{{' before {kind} body."))?;
        let body = self.block()?;
        Ok(Function { name, params, body })
    }

    /// `var` has already been consumed.
    fn var_declaration(&mut self) -> Result<Stmt<'de>, Error> {
        let name = self.consume(TokenKind::Ident, "Expect variable name.")?;

        let initializer =
            if self.eat_if(TokenKind::Equal)?.is_some() { Some(self.expression()?) } else { None };

        self.expect(TokenKind::Semicolon, "Expect ';' after variable declaration.")?;
        Ok(Stmt::Var { name, initializer })
    }

    fn statement(&mut self) -> Result<Stmt<'de>, Error> {
        let Some(token) = self.peek()? else {
            return self.expression_statement(); // reports the unexpected end of input
        };

        match token.kind {
            TokenKind::For => {
                self.bump()?;
                self.for_statement()
            }
            TokenKind::If => {
                self.bump()?;
                self.if_statement()
            }
            TokenKind::Print => {
                self.bump()?;
                let value = self.expression()?;
                self.expect(TokenKind::Semicolon, "Expect ';' after value.")?;
                Ok(Stmt::Print(value))
            }
            TokenKind::Return => {
                self.bump()?;
                self.return_statement(token)
            }
            TokenKind::While => {
                self.bump()?;
                self.while_statement()
            }
            TokenKind::LeftBrace => {
                self.bump()?;
                self.block().map(Stmt::Block)
            }
            _ => self.expression_statement(),
        }
    }

    fn expression_statement(&mut self) -> Result<Stmt<'de>, Error> {
        let expr = self.expression()?;
        self.expect(TokenKind::Semicolon, "Expect ';' after expression.")?;
        Ok(Stmt::Expression(expr))
    }

    /// `{` has already been consumed.
    fn block(&mut self) -> Result<Vec<Stmt<'de>>, Error> {
        let mut statements = Vec::new();
        while !self.check(TokenKind::RightBrace)? && self.peek()?.is_some() {
            statements.push(self.declaration()?);
        }
        self.expect(TokenKind::RightBrace, "Expect '}' after block.")?;
        Ok(statements)
    }

    /// `if` has already been consumed. The branches are *statements*, not
    /// blocks, which is what makes `else if` and brace-less bodies just work:
    /// `else if (…) …` is an `else` whose statement happens to be an `if`.
    fn if_statement(&mut self) -> Result<Stmt<'de>, Error> {
        self.expect(TokenKind::LeftParen, "Expect '(' after 'if'.")?;
        let condition = self.expression()?;
        self.expect(TokenKind::RightParen, "Expect ')' after if condition.")?;

        let then_branch = Box::new(self.statement()?);
        // A dangling `else` binds to the nearest `if`, because we grab it here.
        let else_branch = if self.eat_if(TokenKind::Else)?.is_some() {
            Some(Box::new(self.statement()?))
        } else {
            None
        };

        Ok(Stmt::If { condition, then_branch, else_branch })
    }

    fn while_statement(&mut self) -> Result<Stmt<'de>, Error> {
        self.expect(TokenKind::LeftParen, "Expect '(' after 'while'.")?;
        let condition = self.expression()?;
        self.expect(TokenKind::RightParen, "Expect ')' after condition.")?;
        let body = Box::new(self.statement()?);
        Ok(Stmt::While { condition, body })
    }

    /// `return` has already been consumed (passed in for its line number).
    fn return_statement(&mut self, keyword: Token<'de>) -> Result<Stmt<'de>, Error> {
        let value = if self.check(TokenKind::Semicolon)? { None } else { Some(self.expression()?) };
        self.expect(TokenKind::Semicolon, "Expect ';' after return value.")?;
        Ok(Stmt::Return { keyword, value })
    }

    /// `for` has already been consumed.
    ///
    /// Desugars to smaller pieces so nothing downstream needs a `for` node:
    ///
    /// ```text
    /// for (init; cond; inc) body  =>  { init; while (cond) { body; inc; } }
    /// ```
    fn for_statement(&mut self) -> Result<Stmt<'de>, Error> {
        self.expect(TokenKind::LeftParen, "Expect '(' after 'for'.")?;

        let initializer = if self.eat_if(TokenKind::Semicolon)?.is_some() {
            None
        } else if self.eat_if(TokenKind::Var)?.is_some() {
            Some(self.var_declaration()?)
        } else {
            Some(self.expression_statement()?)
        };

        let condition =
            if self.check(TokenKind::Semicolon)? { None } else { Some(self.expression()?) };
        self.expect(TokenKind::Semicolon, "Expect ';' after loop condition.")?;

        let increment =
            if self.check(TokenKind::RightParen)? { None } else { Some(self.expression()?) };
        self.expect(TokenKind::RightParen, "Expect ')' after for clauses.")?;

        let mut body = self.statement()?;

        if let Some(increment) = increment {
            body = Stmt::Block(vec![body, Stmt::Expression(increment)]);
        }

        // An omitted condition means "loop forever".
        let condition = condition.unwrap_or(Expr::Literal(Literal::Bool(true)));
        body = Stmt::While { condition, body: Box::new(body) };

        if let Some(initializer) = initializer {
            body = Stmt::Block(vec![initializer, body]);
        }

        Ok(body)
    }

    fn expression(&mut self) -> Result<Expr<'de>, Error> { self.expression_within(0) }

    /// Pratt parser. Parses an expression, but stops in front of any operator
    /// that binds *looser* than `min_bp`. Callers pass the right binding power
    /// of the operator they just consumed: that single number is what decides
    /// whether `1 - 2 - 3` groups left and `a = b = c` groups right.
    fn expression_within(&mut self, min_bp: u8) -> Result<Expr<'de>, Error> {
        let Some(token) = self.advance()? else {
            return Err(Eof.into());
        };

        // Prefix Position: what can *start* an expression.
        let mut lhs = match token.kind {
            TokenKind::Number(n) => Expr::Literal(Literal::Number(n)),
            TokenKind::String => {
                Expr::Literal(Literal::String(Cow::Borrowed(token.origin.trim_matches('"'))))
            }
            TokenKind::True => Expr::Literal(Literal::Bool(true)),
            TokenKind::False => Expr::Literal(Literal::Bool(false)),
            TokenKind::Nil => Expr::Literal(Literal::Nil),
            TokenKind::Ident => Expr::Variable(token),
            TokenKind::This => Expr::This(token),
            TokenKind::Super => {
                self.expect(TokenKind::Dot, "Expect '.' after 'super'.")?;
                let method = self.consume(TokenKind::Ident, "Expect superclass method name.")?;
                Expr::Super { keyword: token, method }
            }
            TokenKind::LeftParen => {
                let inner = self.expression_within(0)?;
                self.expect(TokenKind::RightParen, "Expect ')' after expression.")?;
                Expr::Grouping(Box::new(inner))
            }
            TokenKind::Bang | TokenKind::Minus => {
                let op = if token.kind == TokenKind::Bang { UnaryOp::Not } else { UnaryOp::Negate };
                let right = self.expression_within(UNARY_BP)?;
                Expr::Unary { op, token, right: Box::new(right) }
            }
            _ => return Err(self.error_at(token, "Expect expression.")),
        };

        // Infix/Postfix Position: keep extending `lhs` while the next operator
        // binds at least as tightly as our caller allows.

        while let Some(op_token) = self.peek()? {
            // Not an operator (`;`, `)`, `,`, ...): this expression is
            // finished, and whoever called us decides whether that
            // token is legal.
            let Some((infix_op, binding)) = Infix::from_token(op_token.kind) else { break };
            if binding.left < min_bp {
                break;
            }
            self.bump()?;

            lhs = match infix_op {
                Infix::Call => self.finish_call(lhs)?,
                Infix::Get => {
                    let name = self.consume(TokenKind::Ident, "Expect property name after '.'.")?;
                    Expr::Get { object: Box::new(lhs), name }
                }

                Infix::Assign => {
                    let value = Box::new(self.expression_within(binding.right)?);
                    match lhs {
                        Expr::Variable(name) => Expr::Assign { name, value },
                        Expr::Get { object, name } => Expr::Set { object, name, value },
                        _ => return Err(self.error_at(op_token, "Invalid assignment target.")),
                    }
                }

                Infix::Logical(op) => {
                    let right = self.expression_within(binding.right)?;
                    Expr::Logical { left: Box::new(lhs), op, right: Box::new(right) }
                }

                Infix::Binary(op) => {
                    let right = self.expression_within(binding.right)?;
                    Expr::Binary {
                        left: Box::new(lhs),
                        op,
                        token: op_token,
                        right: Box::new(right),
                    }
                }
            };
        }

        Ok(lhs)
    }

    /// `(` has already been consumed.
    fn finish_call(&mut self, callee: Expr<'de>) -> Result<Expr<'de>, Error> {
        let mut args = Vec::new();
        if !self.check(TokenKind::RightParen)? {
            loop {
                if args.len() >= MAX_ARITY {
                    return Err(self.error_here("Can't have more than 255 arguments."));
                }
                args.push(self.expression()?);
                if self.eat_if(TokenKind::Comma)?.is_none() {
                    break;
                }
            }
        }

        let paren = self.consume(TokenKind::RightParen, "Expect ')' after arguments.")?;
        Ok(Expr::Call { callee: Box::new(callee), paren, args })
    }
}
