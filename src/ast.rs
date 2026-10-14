//! The Abstract Syntax Tree produced by the parser.

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;

use crate::token::Token;

pub struct Ast;

#[derive(Clone, Debug, PartialEq)]
pub enum Literal<'de> {
    Nil,
    Bool(bool),
    Number(f64),
    String(Cow<'de, str>),
}
impl fmt::Display for Literal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("nil"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::String(s) => f.write_str(s),
            Self::Number(n) if n.fract() == 0.0 => write!(f, "{n}.0"),
            Self::Number(n) => write!(f, "{n}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnaryOp {
    Negate,
    Not,
}
impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Negate => "-",
            Self::Not => "!",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}
impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Add => "+",
            Self::Subtract => "-",
            Self::Multiply => "*",
            Self::Divide => "/",
            Self::Equal => "==",
            Self::NotEqual => "!=",
            Self::Less => "<",
            Self::LessEqual => "<=",
            Self::Greater => ">",
            Self::GreaterEqual => ">=",
        })
    }
}

/// `and` / `or` are separate from [`BinaryOp`] because they short-circuit i.e.
/// the right operand is not always evaluated
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}
impl fmt::Display for LogicalOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::And => "and",
            Self::Or => "or",
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr<'de> {
    Literal(Literal<'de>),
    Variable(Token<'de>),
    This(Token<'de>),
    Super { keyword: Token<'de>, method: Token<'de> },
    Grouping(Box<Self>),
    Unary { op: UnaryOp, token: Token<'de>, right: Box<Self> },
    Binary { left: Box<Self>, op: BinaryOp, right: Box<Self>, token: Token<'de> },
    Logical { left: Box<Self>, op: LogicalOp, right: Box<Self> },
    Assign { name: Token<'de>, value: Box<Self> },
    Call { paren: Token<'de>, callee: Box<Self>, args: Vec<Self> },
    Get { name: Token<'de>, object: Box<Self> },
    Set { name: Token<'de>, object: Box<Self>, value: Box<Self> },
}
impl fmt::Display for Expr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(literal) => write!(f, "{literal}"),
            Self::Variable(name) => f.write_str(name.origin),
            Self::This(_) => f.write_str("this"),
            Self::Super { method, .. } => write!(f, "(. super {})", method.origin),
            Self::Grouping(inner) => write!(f, "(group {inner})"),
            Self::Unary { op, right, .. } => write!(f, "({op} {right})"),
            Self::Binary { left, op, right, .. } => write!(f, "({op} {left} {right})"),
            Self::Logical { left, op, right } => write!(f, "({op} {left} {right})"),
            Self::Assign { name, value } => write!(f, "(= {} {value})", name.origin),
            Self::Call { callee, args, .. } => {
                write!(f, "({callee}")?;
                args.iter().try_for_each(|arg| write!(f, " {arg}"))?;
                f.write_str(")")
            }
            Self::Get { object, name } => write!(f, "(. {object} {})", name.origin),
            Self::Set { object, name, value } => {
                write!(f, "(= (. {object} {}) {value})", name.origin)
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function<'de> {
    pub name: Token<'de>,
    pub params: Vec<Token<'de>>,
    pub body: Vec<Stmt<'de>>,
}
impl fmt::Display for Function<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "(def {} (", self.name.origin)?;

        for (idx, param) in self.params.iter().enumerate() {
            if idx > 0 {
                f.write_str(" ")?;
            }

            f.write_str(param.origin)?;
        }

        f.write_str(")")?;
        self.body.iter().try_for_each(|item| write!(f, " {item}"))?;
        f.write_str(")")
    }
}

/// The statement class
///
/// There is no `For` variant: the parser desugars `for` into `while` + blocks
#[derive(Clone, Debug, PartialEq)]
pub enum Stmt<'de> {
    Expression(Expr<'de>),
    Print(Expr<'de>),
    Var { name: Token<'de>, initializer: Option<Expr<'de>> },
    Block(Vec<Self>),
    If { condition: Expr<'de>, then_branch: Box<Self>, else_branch: Option<Box<Self>> },
    While { condition: Expr<'de>, body: Box<Self> },
    Function(Function<'de>),
    Return { keyword: Token<'de>, value: Option<Expr<'de>> },
    Class { name: Token<'de>, superclass: Option<Expr<'de>>, methods: Vec<Function<'de>> },
}
impl fmt::Display for Stmt<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Expression(e) => write!(f, "(expr {e})"),
            Self::Print(e) => write!(f, "(print {e})"),
            Self::Var { name, initializer: Some(init) } => {
                write!(f, "(var {} {init})", name.origin)
            }
            Self::Var { name, initializer: None } => write!(f, "(var {})", name.origin),
            Self::Block(stmts) => {
                f.write_str("(block")?;

                stmts.iter().try_for_each(|stmt| write!(f, " {stmt}"))?;

                f.write_str(")")
            }
            Self::If { condition, then_branch, else_branch: Some(fallback) } => {
                write!(f, "(if {condition} {then_branch} {fallback})")
            }
            Self::If { condition, then_branch, else_branch: None } => {
                write!(f, "(if {condition} {then_branch})")
            }
            Self::While { condition, body } => write!(f, "(while {condition} {body})"),
            Self::Function(fun) => write!(f, "{fun}"),
            Self::Return { value: Some(v), .. } => write!(f, "(return {v})"),
            Self::Return { value: None, .. } => f.write_str("(return)"),
            Self::Class { name, superclass, methods } => {
                write!(f, "(class {}", name.origin)?;

                if let Some(class) = superclass {
                    write!(f, " < {class}")?;
                }

                methods
                    .iter()
                    .try_for_each(|method| write!(f, " {method}"))?;

                f.write_str(")")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operator {
    Minus,
    Plus,
    Star,
    BangEqual,
    EqualEqual,
    LessEqual,
    GreaterEqual,
    Less,
    Greater,
    Slash,
    Bang,
    And,
    Or,
    Call,
    For,
    Class,
    Print,
    Return,
    Field,
    Var,
    While,
    Group,
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::Minus => "-",
            Self::Plus => "",
            Self::Star => "*",
            Self::BangEqual => "!=",
            Self::EqualEqual => "==",
            Self::LessEqual => "<=",
            Self::GreaterEqual => ">=",
            Self::Less => "<",
            Self::Greater => ">",
            Self::Slash => "/",
            Self::Bang => "!",
            Self::And => "and",
            Self::Or => "or",
            Self::For => "for",
            Self::While => "while",
            Self::Class => "class",
            Self::Call => "call",
            Self::Print => "print",
            Self::Return => "return",
            Self::Field => ".",
            Self::Group => "group",
            Self::Var => "var",
        })
    }
}
