//! Parser: recursive descent for statements, Pratt for expressions.
//!
//! Grammar:
//!
//! ```text
//! Program     → Declaration* EOF
//! Declaration → ClassDecl | FunDecl | VarDecl | Statement
//! ClassDecl   → "class" IDENT ( "<" IDENT )? "{" function* "}"
//! FunDecl     → "fun" function
//! function    → IDENT "(" parameters? ")" Block
//! VarDecl     → "var" IDENT ( "=" expression )? ";"
//! Statement   → ExprStmt | ForStmt | IfStmt | PrintStmt | ReturnStmt | WhileStmt | Block
//! ForStmt     → "for" "(" ( VarDecl | ExprStmt | ";" ) expression? ";" expression? ")" statement
//! IfStmt      → "if" "(" expression ")" statement ( "else" statement )?
//! ReturnStmt  → "return" expression? ";"
//! WhileStmt   → "while" "(" expression ")" statement
//! Block       → "{" Declaration* "}"
//! ```

use alloc::borrow::ToOwned as _;
use alloc::rc::Rc;
use alloc::{format, vec};

use crate::ast::{BinaryOp, Expr, Function, Literal, LogicalOp, Stmt, UnaryOp};
use crate::error::{Location, SyntaxError};
use crate::scan::Scanner;
use crate::token::{Token, TokenKind};

/// What a grammar rule returns: its result, or the error that aborts the
/// current declaration (see the module docs).3
type ParseResult<T> = Result<T, SyntaxError>;

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
    /// `lhs < rhs` is left-associative: `1 - 2 - 3` is `(1 - 2) - 3`
    ///
    /// `lhs > rhs` is right-associative: `a = b = c`  is  `a = (b = c)`
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
    lexer: Scanner<'de>,
    previous: Option<TokenKind>,
    errors: Vec<SyntaxError>,
}

impl<'de> Parser<'de> {
    #[must_use]
    pub const fn new(input: &'de str) -> Self {
        Self { lexer: Scanner::new(input), previous: None, errors: Vec::new() }
    }

    /// Parses a whole program: declarations until end of input.
    ///
    /// # Errors
    ///
    /// Every lexical and syntax error found, in the order they were found. The
    /// parser recovers after each one, so a single call reports them all.
    pub fn parse(mut self) -> Result<Vec<Stmt<'de>>, Vec<SyntaxError>> {
        let mut program = Vec::new();

        while self.peek().is_some() {
            if let Some(stmt) = self.declaration() {
                program.push(stmt);
            }
        }

        if self.errors.is_empty() { Ok(program) } else { Err(self.errors) }
    }

    /// Parses exactly one expression (the `parse` subcommand).
    ///
    /// # Errors
    ///
    /// Every lexical and syntax error found; see [`Self::parse`].
    pub fn parse_expression(mut self) -> Result<Expr<'de>, Vec<SyntaxError>> {
        let result = self.expression().and_then(|expr| match self.peek() {
            Some(extra) => Err(Self::error_at(extra, "Expect end of expression.")),
            None => Ok(expr),
        });

        match result {
            Ok(expr) if self.errors.is_empty() => Ok(expr),
            Ok(_) => Err(self.errors),
            Err(err) => {
                self.errors.push(err);
                Err(self.errors)
            }
        }
    }

    fn peek(&mut self) -> Option<Token<'de>> {
        loop {
            match self.lexer.peek() {
                None => return None,
                Some(Ok(token)) => return Some(*token),
                // Take the error out of the peek slot, record it, look again.
                Some(Err(_)) => {
                    if let Some(Err(err)) = self.lexer.next() {
                        self.errors.push(err.into());
                    }
                }
            }
        }
    }

    /// Consumes and returns the next token.
    fn advance(&mut self) -> Option<Token<'de>> {
        let token = self.peek()?;
        let _consumed = self.lexer.next();

        self.previous = Some(token.kind);
        Some(token)
    }

    /// Consumes the next token, discarding it
    fn bump(&mut self) { let _discarded = self.advance(); }

    fn check(&mut self, kind: TokenKind) -> bool {
        self.peek().is_some_and(|token| token.kind == kind)
    }

    /// Consumes the next token if it is `kind`
    fn eat(&mut self, kind: TokenKind) -> bool {
        let found = self.check(kind);
        if found {
            self.bump();
        }
        found
    }

    /// Like [`Self::consume`] for when the token itself isn't needed (`)`,
    /// `;`, ...).
    fn expect(&mut self, kind: TokenKind, message: &str) -> ParseResult<()> {
        self.consume(kind, message).map(|_| ())
    }

    /// Consumes a token that *must* be `kind`, or fails with `message`.
    ///
    /// On failure nothing is consumed: the error points at the offending token
    /// and `synchronize` will be the one to skip it.
    fn consume(&mut self, kind: TokenKind, message: &str) -> ParseResult<Token<'de>> {
        match self.peek() {
            Some(token) if token.kind == kind => {
                self.bump();
                Ok(token)
            }
            _ => Err(self.error_here(message)),
        }
    }

    fn error_at(token: Token<'de>, message: &str) -> SyntaxError {
        SyntaxError::new(token.line, Location::Token(token.origin.to_owned()), message.to_owned())
    }

    /// Error located at whatever token comes next, or at the end of input.
    fn error_here(&mut self, message: &str) -> SyntaxError {
        match self.peek() {
            Some(token) => Self::error_at(token, message),
            None => SyntaxError::new(self.lexer.line(), Location::End, message.to_owned()),
        }
    }

    /// Records an error without aborting the current declaration.
    fn report(&mut self, error: SyntaxError) { self.errors.push(error); }

    /// After an error, skip tokens until something that
    /// plausibly starts a new statement (just past a `;`, or a keyword that
    /// begins a declaration), so one mistake doesn't cascade into an annoying
    /// wall of bogus follow-up errors.
    fn synchronize(&mut self) {
        // The offending token was never consumed; make sure we always advance.
        self.bump();

        while let Some(token) = self.peek() {
            if self.previous == Some(TokenKind::Semicolon) {
                return;
            }

            if matches!(
                token.kind,
                TokenKind::Class
                    | TokenKind::Fun
                    | TokenKind::Var
                    | TokenKind::For
                    | TokenKind::If
                    | TokenKind::While
                    | TokenKind::Print
                    | TokenKind::Return
            ) {
                return;
            }

            self.bump();
        }
    }

    /// One declaration, with error recovery: this is where a thrown error
    /// stops. On error, the mistake is recorded, the parser re-synchronizes,
    /// and `None` is returned in place of the statement.
    fn declaration(&mut self) -> Option<Stmt<'de>> {
        let result = if self.eat(TokenKind::Class) {
            self.class_declaration()
        } else if self.eat(TokenKind::Fun) {
            self.function("function")
                .map(|function| Stmt::Function(Rc::new(function)))
        } else if self.eat(TokenKind::Var) {
            self.var_declaration()
        } else {
            self.statement()
        };

        match result {
            Ok(stmt) => Some(stmt),
            Err(err) => {
                self.report(err);
                self.synchronize();
                None
            }
        }
    }

    fn class_declaration(&mut self) -> ParseResult<Stmt<'de>> {
        let name = self.consume(TokenKind::Ident, "Expect class name.")?;

        let superclass = if self.eat(TokenKind::Less) {
            let parent = self.consume(TokenKind::Ident, "Expect superclass name.")?;
            Some(Expr::Variable(parent))
        } else {
            None
        };

        self.expect(TokenKind::LeftBrace, "Expect '{' before class body.")?;

        let mut methods = Vec::new();
        while !self.check(TokenKind::RightBrace) && self.peek().is_some() {
            methods.push(self.function("method")?);
        }

        self.expect(TokenKind::RightBrace, "Expect '}' after class body.")?;

        Ok(Stmt::Class { name, superclass, methods })
    }

    /// `kind` is "function" or "method"; here, it is only used for error
    /// messages.
    fn function(&mut self, kind: &str) -> ParseResult<Function<'de>> {
        let name = self.consume(TokenKind::Ident, &format!("Expect {kind} name."))?;
        self.expect(TokenKind::LeftParen, &format!("Expect '(' after {kind} name."))?;

        let mut params = Vec::new();
        if !self.check(TokenKind::RightParen) {
            loop {
                if params.len() >= MAX_ARITY {
                    // Reported, not thrown: the tree is still fine, so keep
                    // going.
                    let err = self.error_here("Can't have more than 255 parameters.");
                    self.report(err);
                }

                params.push(self.consume(TokenKind::Ident, "Expect parameter name.")?);
                if !self.eat(TokenKind::Comma) {
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
    fn var_declaration(&mut self) -> ParseResult<Stmt<'de>> {
        let name = self.consume(TokenKind::Ident, "Expect variable name.")?;

        let initializer = if self.eat(TokenKind::Equal) { Some(self.expression()?) } else { None };

        self.expect(TokenKind::Semicolon, "Expect ';' after variable declaration.")?;
        Ok(Stmt::Var { name, initializer })
    }

    fn statement(&mut self) -> ParseResult<Stmt<'de>> {
        let Some(token) = self.peek() else {
            return self.expression_statement(); // reports the unexpected end of input
        };

        match token.kind {
            TokenKind::For => {
                self.bump();
                self.for_statement()
            }
            TokenKind::If => {
                self.bump();
                self.if_statement()
            }
            TokenKind::Print => {
                self.bump();
                let value = self.expression()?;
                self.expect(TokenKind::Semicolon, "Expect ';' after value.")?;
                Ok(Stmt::Print(value))
            }
            TokenKind::Return => {
                self.bump();
                self.return_statement(token)
            }
            TokenKind::While => {
                self.bump();
                self.while_statement()
            }
            TokenKind::LeftBrace => {
                self.bump();
                Ok(self.block().map(Stmt::Block)?)
            }
            _ => self.expression_statement(),
        }
    }

    fn expression_statement(&mut self) -> ParseResult<Stmt<'de>> {
        let expr = self.expression()?;
        self.expect(TokenKind::Semicolon, "Expect ';' after expression.")?;
        Ok(Stmt::Expression(expr))
    }

    /// `{` has already been consumed. Statements inside go through
    /// [`Self::declaration`], so a mistake in one doesn't discard its
    /// neighbours.
    fn block(&mut self) -> ParseResult<Vec<Stmt<'de>>> {
        let mut statements = Vec::new();

        while !self.check(TokenKind::RightBrace) && self.peek().is_some() {
            if let Some(stmt) = self.declaration() {
                statements.push(stmt);
            }
        }

        self.expect(TokenKind::RightBrace, "Expect '}' after block.")?;
        Ok(statements)
    }

    fn if_statement(&mut self) -> ParseResult<Stmt<'de>> {
        self.expect(TokenKind::LeftParen, "Expect '(' after 'if'.")?;
        let condition = self.expression()?;
        self.expect(TokenKind::RightParen, "Expect ')' after if condition.")?;

        let then_branch = Box::new(self.statement()?);
        // A dangling `else` binds to the nearest `if`, because we grab it here.
        let else_branch =
            if self.eat(TokenKind::Else) { Some(Box::new(self.statement()?)) } else { None };

        Ok(Stmt::If { condition, then_branch, else_branch })
    }

    fn while_statement(&mut self) -> ParseResult<Stmt<'de>> {
        self.expect(TokenKind::LeftParen, "Expect '(' after 'while'.")?;
        let condition = self.expression()?;
        self.expect(TokenKind::RightParen, "Expect ')' after condition.")?;
        let body = Box::new(self.statement()?);
        Ok(Stmt::While { condition, body })
    }

    /// `return` has already been consumed (passed in for its line number).
    fn return_statement(&mut self, keyword: Token<'de>) -> ParseResult<Stmt<'de>> {
        let value = if self.check(TokenKind::Semicolon) { None } else { Some(self.expression()?) };
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
    fn for_statement(&mut self) -> ParseResult<Stmt<'de>> {
        self.expect(TokenKind::LeftParen, "Expect '(' after 'for'.")?;

        let initializer = if self.eat(TokenKind::Semicolon) {
            None
        } else if self.eat(TokenKind::Var) {
            Some(self.var_declaration()?)
        } else {
            Some(self.expression_statement()?)
        };

        let condition =
            if self.check(TokenKind::Semicolon) { None } else { Some(self.expression()?) };
        self.expect(TokenKind::Semicolon, "Expect ';' after loop condition.")?;

        let increment =
            if self.check(TokenKind::RightParen) { None } else { Some(self.expression()?) };
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

    fn expression(&mut self) -> ParseResult<Expr<'de>> { self.expression_within(0) }

    /// Parses an expression via a pratt parser, but stops in front of any
    /// operator that binds *looser* than `min_bp`. Callers need to pass the
    /// right binding power of the operator they just consumed so that the
    /// interpreter can decide whether `1 - 2 - 3` groups left and `a = b =
    /// c` groups right.
    fn expression_within(&mut self, min_bp: u8) -> ParseResult<Expr<'de>> {
        let Some(token) = self.peek() else {
            return Err(self.error_here("Expect expression."));
        };

        // Prefix position: what can *start* an expression. The token is only
        // consumed once we know it can, so an error points at it and
        // `synchronize` is the one that skips it.
        if !can_start_expression(token.kind) {
            return Err(Self::error_at(token, "Expect expression."));
        }

        self.bump();

        // Prefix Position: what can *start* an expression.
        let mut lhs = match token.kind {
            TokenKind::Number(n) => Expr::Literal(Literal::Number(n)),

            TokenKind::String => {
                Expr::Literal(Literal::String(Rc::from(token.origin.trim_matches('"'))))
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
            _ => return Err(Self::error_at(token, "Expect expression.")),
        };

        // Infix/Postfix Position: keep extending `lhs` while the next operator
        // binds at least as tightly as our caller allows.

        while let Some(op_token) = self.peek() {
            // Not an operator (`;`, `)`, `,`, ...): this expression is
            // finished, and whoever called us decides whether that
            // token is legal.
            let Some((infix_op, binding)) = Infix::from_token(op_token.kind) else { break };
            if binding.left < min_bp {
                break;
            }
            self.bump();

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
                        other => {
                            // Reported, not thrown: keep the left side and go
                            // on.
                            self.report(Self::error_at(op_token, "Invalid assignment target."));
                            other
                        }
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
    fn finish_call(&mut self, callee: Expr<'de>) -> ParseResult<Expr<'de>> {
        let mut args = Vec::new();

        if !self.check(TokenKind::RightParen) {
            loop {
                if args.len() >= MAX_ARITY {
                    // Reported, not thrown: keep parsing the call normally.
                    let err = self.error_here("Can't have more than 255 arguments.");
                    self.report(err);
                }

                args.push(self.expression()?);
                if !self.eat(TokenKind::Comma) {
                    break;
                }
            }
        }

        let paren = self.consume(TokenKind::RightParen, "Expect ')' after arguments.")?;
        Ok(Expr::Call { callee: Box::new(callee), paren, args })
    }
}

/// Can a token of this kind begin an expression?
const fn can_start_expression(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Number(_)
            | TokenKind::String
            | TokenKind::True
            | TokenKind::False
            | TokenKind::Nil
            | TokenKind::Ident
            | TokenKind::This
            | TokenKind::Super
            | TokenKind::LeftParen
            | TokenKind::Bang
            | TokenKind::Minus
    )
}
