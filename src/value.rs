//! A Value is a runtime value meaning what an expression evaluates to.

use alloc::rc::Rc;
use core::{fmt, ptr};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ast::{Function, Literal};
use crate::environment::Scope;

#[derive(Clone)]
pub enum Value<'de> {
    Nil,
    Bool(bool),
    Number(f64),
    String(Rc<str>),
    Function(Rc<LoxFunction<'de>>),
    Native(&'static NativeFunction),
}

impl Value<'_> {
    /// Only `nil` and `false` are falsey; everything else (including `0` and
    /// `""`) is truthy.
    #[must_use]
    pub const fn is_truthy(&self) -> bool { !matches!(self, Self::Nil | Self::Bool(false)) }
}

impl From<&Literal> for Value<'_> {
    fn from(literal: &Literal) -> Self {
        match literal {
            Literal::Nil => Self::Nil,
            Literal::Bool(b) => Self::Bool(*b),
            Literal::Number(n) => Self::Number(*n),
            Literal::String(s) => Self::String(Rc::clone(s)),
        }
    }
}

/// In lox, there are no implicit conversions and values of different kinds are
/// never equal. Lox functions are equal only to themselves.
///
/// Numbers compare like IEEE floats (so `NaN != NaN`, and that is deliberate);
impl PartialEq for Value<'_> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Nil, Self::Nil) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => Rc::ptr_eq(a, b),
            (Self::Native(a), Self::Native(b)) => ptr::eq(*a, *b),
            _ => false,
        }
    }
}

/// How `print` shows a value.
impl fmt::Display for Value<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("nil"),
            Self::Bool(b) => write!(f, "{b}"),
            Self::Number(n) => {
                if n.is_nan() {
                    f.write_str("NaN")
                } else if n.is_infinite() {
                    f.write_str(if *n > 0.0 { "Infinity" } else { "-Infinity" })
                } else {
                    write!(f, "{n}")
                }
            }
            Self::String(s) => f.write_str(s),
            Self::Function(function) => write!(f, "<fn {}>", function.name()),
            Self::Native(_) => f.write_str("<native fn>"),
        }
    }
}

// * NOTE: The debug printing is not derived as that would follow
// * a function's closure, which can lead back to the function itself resulting
// * in an endless loop.
impl fmt::Debug for Value<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => f.write_str("Nil"),
            Self::Bool(b) => write!(f, "Bool({b})"),
            Self::Number(n) => write!(f, "Number({n})"),
            Self::String(s) => write!(f, "String({s:?})"),
            Self::Function(function) => write!(f, "Function({})", function.name()),
            Self::Native(native) => write!(f, "Native({})", native.name),
        }
    }
}

/// A function declared in Lox: its code, plus the scope it was declared in.
///
/// When the scope is kept (making it a closure), the body can keep using the
/// variables around the declaration even after that code has finished running.
pub struct LoxFunction<'de> {
    pub declaration: Rc<Function<'de>>,
    pub closure: Scope<'de>,
}

impl LoxFunction<'_> {
    #[must_use]
    pub fn name(&self) -> &str { self.declaration.name.origin }

    #[must_use]
    pub fn arity(&self) -> usize { self.declaration.params.len() }
}

/// A function implemented in Rust and visible to Lox programs.
pub struct NativeFunction {
    pub name: &'static str,
    pub arity: usize,
    pub call: for<'de> fn(&[Value<'de>]) -> Value<'de>,
}

/// `clock()`: seconds since the Unix epoch, for timing programs.
pub static CLOCK: NativeFunction = NativeFunction { name: "clock", arity: 0, call: clock };

fn clock<'de>(_args: &[Value<'de>]) -> Value<'de> {
    // A clock set before 1970 reads as 0 rather than failing.
    Value::Number(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or_else(|_| 0.0, |d| d.as_secs_f64()),
    )
}
