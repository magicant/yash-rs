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

//! Alias built-in.
//!
//! The **`alias`** built-in defines [aliases] or prints alias definitions.
//!
//! [aliases]: yash_syntax::alias
//!
//! # Synopsis
//!
//! ```sh
//! alias [name[=value]…]
//! ```
//!
//! # Description
//!
//! The alias built-in defines aliases or prints alias definitions as specified
//! by the operands. If there are no operands, the alias built-in prints all
//! alias definitions. The printed definitions are in the form of assignments
//! with proper quoting that can be used as operands to the alias built-in to
//! restore the definitions.
//!
//! # Options
//!
//! None. (TODO: Non-POSIX options)
//!
//! # Operands
//!
//! Each operand must be of the form `name=value` or `name`. The first form
//! defines an alias named *name* that expands to *value*. The second form
//! prints the definition of the alias named *name*.
//!
//! # Errors
//!
//! It is an error if an operand without `=` names a non-existent alias.
//!
//! # Exit status
//!
//! Zero unless an error occurs.
//!
//! # Portability
//!
//! The alias built-in is specified in POSIX.
//!
//! Some shells have a set of predefined aliases that are printed even if you
//! don't define any explicitly.

use crate::common::output;
use crate::common::report_error;
use crate::common::report_failure;
use crate::common::syntax::parse_arguments;
use crate::common::syntax::Mode;
use crate::common::to_single_message;
use yash_env::builtin::Result;
use yash_env::semantics::Field;
use yash_env::Env;

/// Parsed command line arguments
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Command {
    /// Operands to the alias built-in
    pub operands: Vec<Field>,
}

pub mod semantics;

/// Entry point for executing the `alias` built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> Result {
    let mode = Mode::with_env(env);
    // TODO support options
    match parse_arguments(&[], mode, args) {
        Ok((_options, operands)) => {
            let command = Command { operands };
            let (result, errors) = command.execute(env).await;
            let mut result = output(env, &result).await;
            if let Some(message) = to_single_message(&errors) {
                result = result.max(report_failure(env, message).await);
            }
            result
        }

        Err(e) => report_error(env, &e).await,
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use yash_env::semantics::ExitStatus;
    use yash_syntax::source::Source;

    #[test]
    fn builtin_defines_alias() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["foo=bar baz"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        assert_eq!(env.aliases.len(), 1);

        let alias = env.aliases.get("foo").unwrap().0.as_ref();
        assert_eq!(alias.name, "foo");
        assert_eq!(alias.replacement, "bar baz");
        assert_eq!(alias.global, false);
        assert_eq!(*alias.origin.code.value.borrow(), "foo=bar baz");
        assert_eq!(alias.origin.code.start_line_number.get(), 1);
        assert_eq!(alias.origin.code.source, Source::Unknown);
        assert_eq!(alias.origin.range, 0..11);
    }

    #[test]
    fn builtin_defines_many_aliases() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["abc=xyz", "yes=no", "ls=ls --color"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        assert_eq!(env.aliases.len(), 3);

        let abc = env.aliases.get("abc").unwrap().0.as_ref();
        assert_eq!(abc.name, "abc");
        assert_eq!(abc.replacement, "xyz");
        assert_eq!(abc.global, false);
        assert_eq!(*abc.origin.code.value.borrow(), "abc=xyz");
        assert_eq!(abc.origin.code.start_line_number.get(), 1);
        assert_eq!(abc.origin.code.source, Source::Unknown);
        assert_eq!(abc.origin.range, 0..7);

        let yes = env.aliases.get("yes").unwrap().0.as_ref();
        assert_eq!(yes.name, "yes");
        assert_eq!(yes.replacement, "no");
        assert_eq!(yes.global, false);
        assert_eq!(*yes.origin.code.value.borrow(), "yes=no");
        assert_eq!(yes.origin.code.start_line_number.get(), 1);
        assert_eq!(yes.origin.code.source, Source::Unknown);
        assert_eq!(yes.origin.range, 0..6);

        let ls = env.aliases.get("ls").unwrap().0.as_ref();
        assert_eq!(ls.name, "ls");
        assert_eq!(ls.replacement, "ls --color");
        assert_eq!(ls.global, false);
        assert_eq!(*ls.origin.code.value.borrow(), "ls=ls --color");
        assert_eq!(ls.origin.code.start_line_number.get(), 1);
        assert_eq!(ls.origin.code.source, Source::Unknown);
        assert_eq!(ls.origin.range, 0..13);
    }

    #[test]
    fn builtin_replaces_alias() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["foo=1"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        let args = Field::dummies(["foo=2"]);

        let result = main(&mut env, args).now_or_never().unwrap();
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        assert_eq!(env.aliases.len(), 1);

        let alias = env.aliases.get("foo").unwrap().0.as_ref();
        assert_eq!(alias.name, "foo");
        assert_eq!(alias.replacement, "2");
        assert_eq!(alias.global, false);
        // TODO Test with the global option
        // assert_eq!(alias.global, true);
    }

    // TODO test case with global aliases
}
