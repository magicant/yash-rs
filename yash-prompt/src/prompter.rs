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

//! Defines the `Prompter` decorator.

use async_trait::async_trait;
use std::cell::RefCell;
use yash_env::input::{Context, Input, Result};
use yash_env::Env;

/// [`Input`] decorator that shows a command prompt.
///
/// This decorator expands and shows the command prompt before the input is read
/// by the inner `Input`.
#[derive(Clone, Debug)]
#[must_use = "Prompter does nothing unless used by a parser"]
pub struct Prompter<'a, 'b, T: 'a> {
    inner: T,
    env: &'a RefCell<&'b mut Env>,
}

impl<'a, 'b, T> Prompter<'a, 'b, T> {
    /// Creates a new `Prompter` decorator.
    ///
    /// The first argument is the inner `Input` that performs the actual input
    /// operation. The second argument is the shell environment that contains
    /// the prompt variable and the system interface to print to the standard
    /// error. It is wrapped in a `RefCell` so that it can be shared with other
    /// decorators and the parser.
    pub fn new(inner: T, env: &'a RefCell<&'b mut Env>) -> Self {
        Self { inner, env }
    }
}

#[async_trait(?Send)]
impl<'a, 'b, T> Input for Prompter<'a, 'b, T>
where
    T: Input + 'a,
{
    async fn next_line(&mut self, context: &Context) -> Result {
        {
            let mut system = self.env.borrow().system.clone();
            // TODO Honor $PS1 and $PS2
            system.print_error("$ ").await;
        }

        self.inner.next_line(context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::input::Memory;
    use yash_env::system::r#virtual::SystemState;
    use yash_env::system::r#virtual::VirtualSystem;
    use yash_env::variable::Value;
    use yash_env::variable::PS1;
    use yash_env::variable::PS1_INITIAL_VALUE_NON_ROOT;
    use yash_env_test_helper::assert_stderr;

    fn define_variable<N: Into<String>, V: Into<Value>>(env: &mut Env, name: N, value: V) {
        env.variables
            .get_or_new(name, yash_env::variable::Scope::Global)
            .assign(value, None)
            .unwrap();
    }

    #[test]
    fn prompter_reads_from_inner_input() {
        let mut env = Env::new_virtual();
        let ref_env = RefCell::new(&mut env);
        let mut prompter = Prompter::new(Memory::new("echo hello"), &ref_env);
        let result = prompter
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "echo hello");
    }

    #[test]
    fn prompter_shows_prompt_before_reading() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        define_variable(&mut env, PS1, PS1_INITIAL_VALUE_NON_ROOT);

        struct InputMock(Rc<RefCell<SystemState>>);
        #[async_trait::async_trait(?Send)]
        impl Input for InputMock {
            async fn next_line(&mut self, _: &Context) -> Result {
                // The Prompter is expected to have shown the prompt before
                // calling the inner input. Let's check that here.
                assert_stderr(&self.0, |stderr| {
                    assert_eq!(stderr, PS1_INITIAL_VALUE_NON_ROOT)
                });

                Ok("foo".to_string())
            }
        }

        let ref_env = RefCell::new(&mut env);
        let mut prompter = Prompter::new(InputMock(state), &ref_env);
        let result = prompter
            .next_line(&Context::default())
            .now_or_never()
            .unwrap();
        assert_eq!(result.unwrap(), "foo"); // Make sure the mock input is called.
    }

    // TODO ps1_variable_defines_default_prompt
    // TODO ps2_variable_defines_continuation_prompt
    // TODO variable_is_expanded_in_prompt
    // TODO Should we implement and test all of the above here?
}
