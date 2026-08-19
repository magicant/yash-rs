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

//! True built-in
//!
//! This module implements the [`true` built-in], which does nothing, successfully.
//!
//! [`true` built-in]: https://magicant.github.io/yash-rs/builtins/true.html

use crate::Result;
use crate::common::no_arg::warn_if_any_argument;
use yash_env::Env;
use yash_env::semantics::Field;
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;

/// Executes the `true` built-in.
///
/// This is the main entry point for the `true` built-in.
///
/// The built-in ignores its arguments, but warns about them while the
/// `portable` shell option is on. The warning does not affect the exit status.
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> Result
where
    S: Isatty + WriteAll,
{
    warn_if_any_argument(env, &args).await;
    Result::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::option::Option::Portable;
    use yash_env::option::State::On;
    use yash_env::system::Concurrent;
    use yash_env::test_helper::assert_stderr;

    #[test]
    fn returns_success_with_arguments_under_portable_option() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, On);
        let args = Field::dummies(["foo"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        // The argument is only warned about; it does not affect the result.
        assert_eq!(result, Result::default());
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("non-portable argument"), "{stderr:?}");
        });
    }
}
