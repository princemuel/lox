use alloc::rc::Rc;
use core::{hint, mem, ptr};
use std::io::{self, Write};
use std::{panic, thread};

use crate::ast::{BinaryOp, Expr, LogicalOp, Stmt, UnaryOp};
use crate::environment::{Environment, Scope};
use crate::error::{Failure, RuntimeError};
use crate::token::Token;
use crate::value::{CLOCK, LoxFunction, Value};

/// How deeply Lox calls may nest before the program fails with `Stack
/// overflow` thereby preventing a runaway recursion from eating memory.
const MAX_CALL_DEPTH: usize = 10_000;

pub const ONE_MEBIBYTE: usize = 1024 * 1024;

/// Native stack i.e reserved address space for the thread the interpreter runs
/// on.
pub const STACK_BYTES: usize = 256 * ONE_MEBIBYTE;

/// How much of [`STACK_BYTES`] function calls may use before `Stack overflow.`;
/// the rest is headroom for the code that runs between two calls (a deeply
/// nested expression, say).
///
/// This is the *safety* limit. Each Lox call costs a different amount of native
/// stack depending on how deeply nested the function's own code is (measured:
/// roughly 3 KiB to 20+ KiB), so counting calls alone can't promise to stop
/// before the stack runs out, and Rust aborts the whole process when it does.
const STACK_BUDGET: usize = STACK_BYTES - 32 * ONE_MEBIBYTE;

/// Where the native stack is right now, as an address. Only differences between
/// two readings on the same thread mean anything.
fn stack_position() -> usize {
    let marker = 0u8;
    ptr::from_ref(hint::black_box(&marker)).addr()
}

/// Runs `work` on a thread with a stack big enough for [`MAX_CALL_DEPTH`]
/// nested Lox calls (the default 2 MiB for spawned threads, or 8 MiB for
/// `main`, is not). A panic inside `work` is re-raised here.
///
/// # Errors
///
/// If the operating system cannot create the thread.
pub fn with_big_stack<T: Send>(work: impl FnOnce() -> T + Send) -> io::Result<T> {
    thread::scope(|scope| {
        let handle = thread::Builder::new()
            .name("lox".to_owned())
            .stack_size(STACK_BYTES)
            .spawn_scoped(scope, work)?;

        match handle.join() {
            Ok(value) => Ok(value),
            Err(error) => panic::resume_unwind(error),
        }
    })
}

/// What stops a *statement* early. One more case than [`Failure`]: `return`.
enum Unwind<'de> {
    Return(Value<'de>),
    Fail(Failure),
}

impl From<Failure> for Unwind<'_> {
    fn from(failure: Failure) -> Self { Self::Fail(failure) }
}

/// Runs programs, writing what they `print` to `out`.
pub struct Interpreter<'de, W> {
    out: W,
    /// The scope code currently runs in. Blocks and calls swap in a child scope
    /// and restore this one afterwards.
    env: Scope<'de>,
    /// How many Lox calls are in progress.
    depth: usize,
    /// Stack position when this interpreter was created; the stack grows down
    /// from here. (Create the interpreter inside [`with_big_stack`].)
    stack_base: usize,
}

impl<'de, W: Write> Interpreter<'de, W> {
    #[must_use]
    pub fn new(out: W) -> Self {
        let env = Environment::global();
        env.borrow_mut().define("clock", Value::Native(&CLOCK));
        Self { out, env, depth: 0, stack_base: stack_position() }
    }

    /// Runs a whole program.
    ///
    /// # Errors
    ///
    /// The [`Failure`] that stopped it. Output written before that stays
    /// written.
    pub fn interpret(&mut self, program: &[Stmt<'de>]) -> Result<(), Failure> {
        for stmt in program {
            match self.execute(stmt) {
                Ok(()) => {}
                // A `return` outside any function just ends the program. (Step 5's
                // resolver will reject that as a compile error before we get here.)
                Err(Unwind::Return(_)) => break,
                Err(Unwind::Fail(failure)) => return Err(failure),
            }
        }
        Ok(())
    }

    /// Writes `value` and a newline to the output, the way `print` shows it.
    ///
    /// # Errors
    ///
    /// [`Failure::Output`] if the output cannot be written.
    pub fn print(&mut self, value: &Value<'de>) -> Result<(), Failure> {
        writeln!(self.out, "{value}").map_err(Failure::Output)
    }

    /// Flushes buffered output; call this before exiting.
    ///
    /// # Errors
    ///
    /// If the output cannot be written.
    pub fn flush(&mut self) -> io::Result<()> { self.out.flush() }

    fn execute(&mut self, stmt: &Stmt<'de>) -> Result<(), Unwind<'de>> {
        match stmt {
            Stmt::Expression(expr) => {
                let _value = self.evaluate(expr)?;
                Ok(())
            }

            Stmt::Print(expr) => {
                let value = self.evaluate(expr)?;
                self.print(&value)?;
                Ok(())
            }

            Stmt::Var { name, initializer } => {
                let value = match initializer {
                    Some(expr) => self.evaluate(expr)?,
                    None => Value::Nil,
                };
                self.env.borrow_mut().define(name.origin, value);
                Ok(())
            }

            Stmt::Block(statements) => {
                self.execute_block(statements, Environment::child(&self.env))
            }

            Stmt::If { condition, then_branch, else_branch } => {
                if self.evaluate(condition)?.is_truthy() {
                    self.execute(then_branch)
                } else if let Some(else_branch) = else_branch {
                    self.execute(else_branch)
                } else {
                    Ok(())
                }
            }

            Stmt::While { condition, body } => {
                while self.evaluate(condition)?.is_truthy() {
                    self.execute(body)?;
                }
                Ok(())
            }

            Stmt::Function(declaration) => {
                let function = LoxFunction {
                    declaration: Rc::clone(declaration),
                    // The scope the function is *declared* in, not the one it will be called from.
                    closure: Rc::clone(&self.env),
                };
                self.env
                    .borrow_mut()
                    .define(declaration.name.origin, Value::Function(Rc::new(function)));
                Ok(())
            }

            Stmt::Return { value, .. } => {
                let value = match value {
                    Some(expr) => self.evaluate(expr)?,
                    None => Value::Nil,
                };
                Err(Unwind::Return(value))
            }

            Stmt::Class { name, .. } => Err(unsupported(name).into()),
        }
    }

    /// Runs `statements` in `scope`, then puts the previous scope back, even if
    /// a statement failed or returned.
    fn execute_block(
        &mut self,
        statements: &[Stmt<'de>],
        scope: Scope<'de>,
    ) -> Result<(), Unwind<'de>> {
        let previous = mem::replace(&mut self.env, scope);
        let result = statements.iter().try_for_each(|stmt| self.execute(stmt));
        self.env = previous;
        result
    }

    /// Evaluates one expression.
    ///
    /// # Errors
    ///
    /// The [`Failure`] if evaluation goes wrong.
    pub fn evaluate(&mut self, expr: &Expr<'de>) -> Result<Value<'de>, Failure> {
        match expr {
            Expr::Literal(literal) => Ok(Value::from(literal)),
            Expr::Grouping(inner) => self.evaluate(inner),
            Expr::Variable(name) => self
                .env
                .borrow()
                .get(name.origin)
                .ok_or_else(|| undefined_variable(name).into()),
            Expr::Assign { name, value } => {
                let value = self.evaluate(value)?;
                if self.env.borrow_mut().assign(name.origin, value.clone()) {
                    Ok(value)
                } else {
                    Err(undefined_variable(name).into())
                }
            }
            Expr::Unary { op, token, right } => {
                let right = self.evaluate(right)?;
                unary(*op, token, &right).map_err(Failure::from)
            }
            Expr::Binary { left, op, token, right } => {
                let left = self.evaluate(left)?;
                let right = self.evaluate(right)?;
                binary(*op, token, &left, &right).map_err(Failure::from)
            }
            Expr::Logical { left, op, right } => {
                let left = self.evaluate(left)?;
                // `or` stops at the first truthy operand, `and` at the first
                // falsey one; the result is that operand
                // itself, not a boolean.
                let done = match op {
                    LogicalOp::Or => left.is_truthy(),
                    LogicalOp::And => !left.is_truthy(),
                };
                if done { Ok(left) } else { self.evaluate(right) }
            }
            Expr::Call { callee, paren, args } => self.call(callee, paren, args),
            Expr::This(token) | Expr::Super { keyword: token, .. } => Err(unsupported(token)),
            Expr::Get { name, .. } | Expr::Set { name, .. } => Err(unsupported(name)),
        }
    }

    fn call(
        &mut self,
        callee: &Expr<'de>,
        paren: &Token<'de>,
        args: &[Expr<'de>],
    ) -> Result<Value<'de>, Failure> {
        let callee = self.evaluate(callee)?;

        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.evaluate(arg)?);
        }

        match callee {
            Value::Function(function) => {
                check_arity(paren, function.arity(), values.len())?;
                self.call_function(&function, values, paren)
            }
            Value::Native(native) => {
                check_arity(paren, native.arity, values.len())?;
                Ok((native.call)(&values))
            }
            _ => Err(RuntimeError::new(paren, "Can only call functions and classes.").into()),
        }
    }

    fn call_function(
        &mut self,
        function: &LoxFunction<'de>,
        args: Vec<Value<'de>>,
        paren: &Token<'de>,
    ) -> Result<Value<'de>, Failure> {
        // Stacks grow downward on every mainstream platform; if one ever grew
        // up, `saturating_sub` reads 0 and the call-depth limit alone applies.
        let stack_used = self.stack_base.saturating_sub(stack_position());
        if self.depth >= MAX_CALL_DEPTH || stack_used > STACK_BUDGET {
            return Err(RuntimeError::new(paren, "Stack overflow.").into());
        }

        // Parameters live in a fresh scope whose parent is the *closure*, so
        // the body sees the variables around where the function was
        // declared.
        let scope = Environment::child(&function.closure);
        {
            let mut scope = scope.borrow_mut();
            for (param, arg) in function.declaration.params.iter().zip(args) {
                scope.define(param.origin, arg);
            }
        }

        self.depth = self.depth.saturating_add(1);
        let result = self.execute_block(&function.declaration.body, scope);
        self.depth = self.depth.saturating_sub(1);

        match result {
            Ok(()) => Ok(Value::Nil), // fell off the end: returns nil
            Err(Unwind::Return(value)) => Ok(value),
            Err(Unwind::Fail(failure)) => Err(failure),
        }
    }
}

fn check_arity(paren: &Token<'_>, expected: usize, actual: usize) -> Result<(), RuntimeError> {
    if expected == actual {
        Ok(())
    } else {
        Err(RuntimeError::new(paren, format!("Expected {expected} arguments but got {actual}.")))
    }
}

fn undefined_variable(name: &Token<'_>) -> RuntimeError {
    RuntimeError::new(name, format!("Undefined variable '{}'.", name.origin))
}

/// Classes arrive in step 5; until then using one is an ordinary runtime error
/// rather than a crash.
fn unsupported(token: &Token<'_>) -> Failure {
    RuntimeError::new(token, "Classes are not supported yet.").into()
}

fn unary<'de>(
    op: UnaryOp,
    token: &Token<'_>,
    right: &Value<'de>,
) -> Result<Value<'de>, RuntimeError> {
    match (op, right) {
        (UnaryOp::Not, value) => Ok(Value::Bool(!value.is_truthy())),
        (UnaryOp::Negate, Value::Number(n)) => Ok(Value::Number(-n)),
        (UnaryOp::Negate, _) => Err(RuntimeError::new(token, "Operand must be a number.")),
    }
}

fn binary<'de>(
    op: BinaryOp,
    token: &Token<'_>,
    left: &Value<'de>,
    right: &Value<'de>,
) -> Result<Value<'de>, RuntimeError> {
    // Both operands of an arithmetic or comparison operator must be numbers.
    let numbers = |cb: fn(f64, f64) -> Value<'de>| match (left, right) {
        (Value::Number(a), Value::Number(b)) => Ok(cb(*a, *b)),
        _ => Err(RuntimeError::new(token, "Operands must be numbers.")),
    };

    match op {
        BinaryOp::Equal => Ok(Value::Bool(left == right)),
        BinaryOp::NotEqual => Ok(Value::Bool(left != right)),
        BinaryOp::Add => match (left, right) {
            (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
            (Value::String(a), Value::String(b)) => {
                let mut joined = String::with_capacity(a.len().saturating_add(b.len()));

                joined.push_str(a);
                joined.push_str(b);

                Ok(Value::String(Rc::from(joined)))
            }
            _ => Err(RuntimeError::new(token, "Operands must be two numbers or two strings.")),
        },
        BinaryOp::Subtract => numbers(|a, b| Value::Number(a - b)),
        BinaryOp::Multiply => numbers(|a, b| Value::Number(a * b)),
        BinaryOp::Divide => numbers(|a, b| Value::Number(a / b)),
        BinaryOp::Less => numbers(|a, b| Value::Bool(a < b)),
        BinaryOp::LessEqual => numbers(|a, b| Value::Bool(a <= b)),
        BinaryOp::Greater => numbers(|a, b| Value::Bool(a > b)),
        BinaryOp::GreaterEqual => numbers(|a, b| Value::Bool(a >= b)),
    }
}
