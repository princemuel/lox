//! Variable scopes.
//!
//! A scope (lexical scope) is a table of names to values plus a link to the
//! scope around it. Looking a name up walks outward until it is found, meaning
//! an inner scope sees its own names first, then its surroundings.

use alloc::rc::Rc;
use core::cell::RefCell;
use std::collections::HashMap;

use crate::value::Value;

/// A scope is *shared* (`Rc`) and *mutable* (`RefCell`) because several things
/// hold on to the same one: the running code, and every closure created inside
/// it. A closure that outlives its block keeps that block's variables alive.
///
/// FIXME: *scopes that declare functions are never freed*
///
/// A scope that *declares* a function holds that function, and the function
/// holds the scope as its closure. That is a reference cycle, which reference
/// counting can never free. Scopes that declare no function are freed normally.
///
/// For a program that runs and exits this is harmless, but a long loop that
/// declares a function on every iteration currently grows without bound
pub type Scope<'de> = Rc<RefCell<Environment<'de>>>;

pub struct Environment<'de> {
    values: HashMap<&'de str, Value<'de>>,
    enclosing: Option<Scope<'de>>,
}

impl<'de> Environment<'de> {
    /// The outermost scope: no enclosing scope to fall back on.
    #[must_use]
    pub fn global() -> Scope<'de> {
        Rc::new(RefCell::new(Self { values: HashMap::new(), enclosing: None }))
    }

    /// A new scope nested inside `enclosing` (a block body, a function call).
    #[must_use]
    pub fn child(enclosing: &Scope<'de>) -> Scope<'de> {
        Rc::new(RefCell::new(Self {
            values: HashMap::new(),
            enclosing: Some(Rc::clone(enclosing)),
        }))
    }

    /// Creates or replaces a binding in *this* scope: `var x = ...;`.
    pub fn define(&mut self, name: &'de str, value: Value<'de>) {
        let _previous = self.values.insert(name, value);
    }

    /// Looks `name` up here, then outward.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<Value<'de>> {
        match self.values.get(name) {
            Some(value) => Some(value.clone()),
            None => self.enclosing.as_ref()?.borrow().get(name),
        }
    }

    /// Updates the *existing* binding of `name`, here or outward: `x = ...;`.
    /// Returns `false` (and changes nothing) if no scope defines it.
    pub fn assign(&mut self, name: &str, value: Value<'de>) -> bool {
        if let Some(slot) = self.values.get_mut(name) {
            *slot = value;
            true
        } else if let Some(enclosing) = &self.enclosing {
            enclosing.borrow_mut().assign(name, value)
        } else {
            false
        }
    }
}
