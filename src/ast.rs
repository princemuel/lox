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
            // tests require that integers are printed as N.0
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
    Binary { left: Box<Self>, op: BinaryOp, token: Token<'de>, right: Box<Self> },
    Logical { left: Box<Self>, op: LogicalOp, right: Box<Self> },
    Assign { name: Token<'de>, value: Box<Self> },
    Call { callee: Box<Self>, paren: Token<'de>, args: Vec<Self> },
    Get { object: Box<Self>, name: Token<'de> },
    Set { object: Box<Self>, name: Token<'de>, value: Box<Self> },
}
impl fmt::Display for Expr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(lit) => write!(f, "{lit}"),
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
                write_each(f, args)?;
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
        write_each(f, &self.body)?;
        f.write_str(")")
    }
}

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
                write_each(f, stmts)?;
                f.write_str(")")
            }
            Self::If { condition, then_branch, else_branch: Some(no) } => {
                write!(f, "(if {condition} {then_branch} {no})")
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
                if let Some(sup) = superclass {
                    write!(f, " < {sup}")?;
                }
                write_each(f, methods)?;
                f.write_str(")")
            }
        }
    }
}

/// Writes ` a b c` (a leading space before every item).
fn write_each<T: fmt::Display>(f: &mut fmt::Formatter<'_>, items: &[T]) -> fmt::Result {
    items.iter().try_for_each(|item| write!(f, " {item}"))
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
