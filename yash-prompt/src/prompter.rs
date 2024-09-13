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

use std::cell::RefCell;
use yash_env::input::{Context, Input, Result};
use yash_env::variable::{VariableSet, PS1, PS2};
use yash_env::Env;

/// [`Input`] decorator that shows a command prompt
///
/// This decorator expands and shows the command prompt before the input is read
/// by the inner `Input`.
#[derive(Clone, Debug)]
#[must_use = "Prompter does nothing unless used by a parser"]
pub struct Prompter<'a, 'b, T> {
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

impl<'a, 'b, T> Input for Prompter<'a, 'b, T>
where
    T: Input,
{
    #[allow(clippy::await_holding_refcell_ref)]
    async fn next_line(&mut self, context: &Context) -> Result {
        print_prompt(&mut self.env.borrow_mut(), context).await;
        self.inner.next_line(context).await
    }
}

async fn print_prompt(env: &mut Env, context: &Context) {
    // Obtain the prompt string
    let prompt = fetch_posix(&env.variables, context);

    // Perform parameter expansion in the prompt string
    let expanded_prompt = super::expand_posix(env, &prompt, context.is_first_line()).await;

    // Print the prompt to the standard error
    env.system.print_error(&expanded_prompt).await;
}

/// Fetches the command prompt string from the variable set.
///
/// The return value is the raw value taken from the `PS1` or `PS2` variable
/// in the set. [`Context::is_first_line`] determines which variable is used.
/// An empty string is returned if the variable is not found.
///
/// The returned prompt string should be expanded before being shown to the
/// user.
///
/// This function does not consider yash-specific prompt variables.
pub fn fetch_posix(variables: &VariableSet, context: &Context) -> String {
    let var = if context.is_first_line() { PS1 } else { PS2 };
    variables.get_scalar(var).unwrap_or_default().to_owned()
}

// TODO pub fn fetch_ex: yash-specific prompt variables

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

    #[test]
    fn ps1_variable_defines_main_prompt() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        define_variable(&mut env, PS1, "my_custom_prompt !! >");
        let ref_env = RefCell::new(&mut env);
        let mut prompter = Prompter::new(Memory::new(""), &ref_env);

        prompter
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .ok();
        assert_stderr(&state, |stderr| assert_eq!(stderr, "my_custom_prompt ! >"));
        // Note that "!!" is expanded to "!" in the prompt string.
    }

    #[test]
    fn ps2_variable_defines_continuation_prompt() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        define_variable(&mut env, PS2, "continuation ! >");
        let ref_env = RefCell::new(&mut env);
        let mut prompter = Prompter::new(Memory::new(""), &ref_env);
        let mut context = Context::default();
        context.set_is_first_line(false);

        prompter.next_line(&context).now_or_never().unwrap().ok();
        assert_stderr(&state, |stderr| assert_eq!(stderr, "continuation ! >"));
        // Note that "!" is not expanded in the prompt string.
    }

    #[test]
    fn parameter_expansion_in_prompt_string() {
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(system);
        define_variable(&mut env, PS1, "$X $ ");
        define_variable(&mut env, "X", "foo");
        let ref_env = RefCell::new(&mut env);
        let mut prompter = Prompter::new(Memory::new(""), &ref_env);

        prompter
            .next_line(&Context::default())
            .now_or_never()
            .unwrap()
            .ok();
        assert_stderr(&state, |stderr| assert_eq!(stderr, "foo $ "));
    }
}
