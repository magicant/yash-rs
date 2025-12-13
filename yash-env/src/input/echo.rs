// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! `Echo` definition

use super::{Context, Input, Result};
use crate::option::Option::Verbose;
use crate::option::State::On;
use crate::{Env, SharedSystem};
use std::cell::RefCell;

/// `Input` decorator that echoes the input.
///
/// This decorator adds the behavior of the [verbose](crate::option::Verbose)
/// shell option to the input. If the option is enabled, the input is echoed to
/// the standard error before it is returned to the caller. Otherwise, the input
/// is returned as is.
#[derive(Clone, Debug)]
#[doc(alias = "Verbose")]
#[must_use = "Echo does nothing unless used by a parser"]
pub struct Echo<'a, 'b, T> {
    inner: T,
    env: &'a RefCell<&'b mut Env>,
    system: SharedSystem,
}

impl<'a, 'b, T> Echo<'a, 'b, T> {
    /// Creates a new `Echo` decorator.
    ///
    /// The first argument is the inner `Input` that performs the actual input
    /// operation. The second argument is the shell environment that contains
    /// the shell option state. The third argument is the system interface to
    /// print to the standard error.
    pub fn new(inner: T, env: &'a RefCell<&'b mut Env>, system: SharedSystem) -> Self {
        Self { inner, env, system }
    }
}

impl<T> Input for Echo<'_, '_, T>
where
    T: Input,
{
    // The RefCell should be local to the calling read-eval loop, so it is safe
    // to keep the mutable borrow across await points.
    #[allow(clippy::await_holding_refcell_ref)]
    async fn next_line(&mut self, context: &Context) -> Result {
        let line = self.inner.next_line(context).await?;

        if self.env.borrow().options.get(Verbose) == On {
            self.system.print_error(&line).await;
        }

        Ok(line)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Memory;
    use super::*;
    use crate::system::r#virtual::VirtualSystem;
    use crate::tests::assert_stderr;
    use futures_util::FutureExt;
    use std::rc::Rc;

    #[test]
    fn verbose_off() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        let system = env.system.clone();
        let ref_env = RefCell::new(&mut env);
        let memory = Memory::new("echo test\n");
        let mut echo = Echo::new(memory, &ref_env, system);

        let line = echo
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "echo test\n");
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn verbose_on() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        env.options.set(Verbose, On);
        let system = env.system.clone();
        let ref_env = RefCell::new(&mut env);
        let memory = Memory::new("echo test\nfoo");
        let mut echo = Echo::new(memory, &ref_env, system);

        let line = echo
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "echo test\n");
        assert_stderr(&state, |stderr| assert_eq!(stderr, "echo test\n"));

        let line = echo
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .unwrap();
        assert_eq!(line, "foo");
        assert_stderr(&state, |stderr| assert_eq!(stderr, "echo test\nfoo"));
    }
}
