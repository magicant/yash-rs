// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use super::ContextType;
use super::VariableSet;
use crate::Env;
use std::ops::Deref;
use std::ops::DerefMut;

/// RAII-style guard for temporarily retaining a variable context.
///
/// The guard object is created by [`VariableSet::push_context`].
#[derive(Debug)]
#[must_use = "You must retain ContextGuard to keep the context alive"]
pub struct ContextGuard<'a> {
    stack: &'a mut VariableSet,
}

impl VariableSet {
    /// Pushes a new empty context to this variable set.
    ///
    /// This function returns a scope guard that will pop the context when dropped.
    #[inline]
    pub fn push_context(&mut self, context_type: ContextType) -> ContextGuard<'_> {
        self.push_context_impl(context_type);
        ContextGuard { stack: self }
    }

    /// Pops the topmost context from the variable set.
    #[inline]
    pub fn pop_context(guard: ContextGuard<'_>) {
        drop(guard)
    }
}

/// When the guard is dropped, the context that was pushed when creating the
/// guard is [popped](VariableSet::pop_context).
impl std::ops::Drop for ContextGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        self.stack.pop_context_impl()
    }
}

impl std::ops::Deref for ContextGuard<'_> {
    type Target = VariableSet;
    #[inline]
    fn deref(&self) -> &VariableSet {
        self.stack
    }
}

impl std::ops::DerefMut for ContextGuard<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut VariableSet {
        self.stack
    }
}

/// RAII-style guard that makes sure a context is popped properly
///
/// The guard object is created by [`Env::push_context`].
#[derive(Debug)]
#[must_use = "The context is popped when the guard is dropped"]
pub struct EnvContextGuard<'a> {
    env: &'a mut Env,
}

impl Env {
    /// Pushes a new context to the variable set.
    ///
    /// This function is equivalent to
    /// `self.variables.push_context(context_type)`, but returns an
    /// `EnvContextGuard` that allows re-borrowing the `Env`.
    #[inline]
    pub fn push_context(&mut self, context_type: ContextType) -> EnvContextGuard<'_> {
        self.variables.push_context_impl(context_type);
        EnvContextGuard { env: self }
    }

    /// Pops the topmost context from the variable set.
    #[inline]
    pub fn pop_context(guard: EnvContextGuard<'_>) {
        drop(guard)
    }
}

/// When the guard is dropped, the context that was pushed when creating the
/// guard is [popped](VariableSet::pop_context).
impl Drop for EnvContextGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        self.env.variables.pop_context_impl()
    }
}

impl Deref for EnvContextGuard<'_> {
    type Target = Env;
    #[inline]
    fn deref(&self) -> &Env {
        self.env
    }
}

impl DerefMut for EnvContextGuard<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Env {
        self.env
    }
}

#[cfg(test)]
mod tests {
    use super::super::Scope;
    use super::super::Value;
    use super::*;

    #[test]
    fn scope_guard() {
        let mut env = Env::new_virtual();
        let mut guard = env.variables.push_context(ContextType::Regular);
        guard
            .get_or_new("foo", Scope::Global)
            .assign("", None)
            .unwrap();
        guard
            .get_or_new("bar", Scope::Local)
            .assign("", None)
            .unwrap();
        VariableSet::pop_context(guard);

        let variable = env.variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("")));
        assert_eq!(env.variables.get("bar"), None);
    }

    #[test]
    fn env_scope_guard() {
        let mut env = Env::new_virtual();
        let mut guard = env.push_context(ContextType::Regular);
        guard
            .variables
            .get_or_new("foo", Scope::Global)
            .assign("", None)
            .unwrap();
        guard
            .variables
            .get_or_new("bar", Scope::Local)
            .assign("", None)
            .unwrap();
        Env::pop_context(guard);

        let variable = env.variables.get("foo").unwrap();
        assert_eq!(variable.value, Some(Value::scalar("")));
        assert_eq!(env.variables.get("bar"), None);
    }
}
