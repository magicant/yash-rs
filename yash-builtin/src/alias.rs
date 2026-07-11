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
//! This module implements the [`alias` built-in], which defines aliases or prints
//! alias definitions.
//!
//! [`alias` built-in]: https://magicant.github.io/yash-rs/builtins/alias.html

use crate::common::output;
use crate::common::report::merge_reports;
use crate::common::report::report_error;
use crate::common::report::report_failure;
use crate::common::syntax::Mode;
use crate::common::syntax::parse_arguments;
use std::mem::Discriminant;
use std::mem::discriminant;
use yash_env::Env;
use yash_env::builtin::Result;
use yash_env::semantics::Field;
use yash_env::system::Isatty;
use yash_env::system::concurrency::WriteAll;

/// Parsed command line arguments
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub struct Command {
    /// Operands to the alias built-in
    pub operands: Vec<Field>,
}

pub mod semantics;

/// Groups the given errors by kind.
///
/// Errors of the same kind (e.g. two [`NonPortableAliasName`] errors) end up
/// in the same group regardless of where they occur among the operands, so
/// that [`main`] can report each kind as its own message rather than merging
/// unrelated kinds of errors under one misleading shared title. The groups
/// are returned in the order in which each kind first appears in `errors`.
///
/// [`NonPortableAliasName`]: semantics::Error::NonPortableAliasName
fn group_errors_by_kind(
    errors: &[semantics::Error],
) -> Vec<(Discriminant<semantics::Error>, Vec<&semantics::Error>)> {
    let mut groups: Vec<(_, Vec<_>)> = Vec::new();
    for error in errors {
        let kind = discriminant(error);
        match groups.iter_mut().find(|(k, _)| *k == kind) {
            Some((_, group)) => group.push(error),
            None => groups.push((kind, vec![error])),
        }
    }
    groups
}

/// Entry point for executing the `alias` built-in
pub async fn main<S>(env: &mut Env<S>, args: Vec<Field>) -> Result
where
    S: Isatty + WriteAll,
{
    let mode = Mode::with_env(env);
    // TODO support options
    match parse_arguments(&[], mode, args) {
        Ok((_options, operands)) => {
            let command = Command { operands };
            let (output_string, errors) = command.execute(env).await;
            let mut result = output(env, &output_string).await;
            for (_, group) in group_errors_by_kind(&errors) {
                if let Some(report) = merge_reports(group) {
                    result = result.max(report_failure(env, report).await);
                }
            }
            result
        }

        Err(e) => report_error(env, &e).await,
    }
}

#[allow(
    clippy::bool_assert_comparison,
    reason = "to make the expected values clearer"
)]
#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt as _;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::option::Option::Portable;
    use yash_env::option::State::On;
    use yash_env::semantics::ExitStatus;
    use yash_env::source::Source;
    use yash_env::system::Concurrent;
    use yash_env::test_helper::assert_stderr;

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
        assert_eq!(*alias.origin.code.source, Source::Unknown);
        assert_eq!(alias.origin.range, 0..11);
    }

    #[test]
    fn builtin_errors_on_non_portable_alias_name() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, On);
        let args = Field::dummies(["a.b=x"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result, Result::new(ExitStatus::FAILURE));
        assert!(!env.aliases.contains("a.b"));
        assert_stderr(&state, |stderr| {
            assert!(stderr.contains("not portable"), "stderr = {stderr:?}");
            assert!(stderr.contains("a.b"), "stderr = {stderr:?}");
        });
    }

    #[test]
    fn builtin_mixed_non_portable_name_and_other_error() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        env.options.set(Portable, On);
        let args = Field::dummies(["a.b=x", "missing"]);

        let result = main(&mut env, args).now_or_never().unwrap();

        assert_eq!(result, Result::new(ExitStatus::FAILURE));
        assert!(!env.aliases.contains("a.b"));
        assert_stderr(&state, |stderr| {
            // The two unrelated kinds of errors are reported as separate
            // messages, each with a title that accurately describes it,
            // rather than being merged under one misleading shared title.
            assert!(
                stderr.contains("cannot define alias with non-portable name"),
                "stderr = {stderr:?}"
            );
            assert!(
                stderr.contains("cannot print alias definition"),
                "stderr = {stderr:?}"
            );
            assert!(stderr.contains("not portable"), "stderr = {stderr:?}");
            assert!(stderr.contains("not found"), "stderr = {stderr:?}");
        });
    }

    #[test]
    fn group_errors_by_kind_merges_non_adjacent_errors_of_the_same_kind() {
        let a_b = semantics::Error::NonPortableAliasName {
            name: Field::dummy("a.b"),
        };
        let missing = semantics::Error::NonExistentAlias {
            name: Field::dummy("missing"),
        };
        let c_d = semantics::Error::NonPortableAliasName {
            name: Field::dummy("c.d"),
        };
        let errors = vec![a_b.clone(), missing.clone(), c_d.clone()];

        let groups = group_errors_by_kind(&errors);

        assert_eq!(
            groups,
            [
                (discriminant(&a_b), vec![&a_b, &c_d]),
                (discriminant(&missing), vec![&missing])
            ]
        );
    }

    #[test]
    fn group_errors_by_kind_orders_groups_by_first_occurrence() {
        let missing = semantics::Error::NonExistentAlias {
            name: Field::dummy("missing"),
        };
        let a_b = semantics::Error::NonPortableAliasName {
            name: Field::dummy("a.b"),
        };
        let errors = vec![missing.clone(), a_b.clone()];

        let groups = group_errors_by_kind(&errors);

        assert_eq!(
            groups,
            [
                (discriminant(&missing), vec![&missing]),
                (discriminant(&a_b), vec![&a_b])
            ]
        );
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
        assert_eq!(*abc.origin.code.source, Source::Unknown);
        assert_eq!(abc.origin.range, 0..7);

        let yes = env.aliases.get("yes").unwrap().0.as_ref();
        assert_eq!(yes.name, "yes");
        assert_eq!(yes.replacement, "no");
        assert_eq!(yes.global, false);
        assert_eq!(*yes.origin.code.value.borrow(), "yes=no");
        assert_eq!(yes.origin.code.start_line_number.get(), 1);
        assert_eq!(*yes.origin.code.source, Source::Unknown);
        assert_eq!(yes.origin.range, 0..6);

        let ls = env.aliases.get("ls").unwrap().0.as_ref();
        assert_eq!(ls.name, "ls");
        assert_eq!(ls.replacement, "ls --color");
        assert_eq!(ls.global, false);
        assert_eq!(*ls.origin.code.value.borrow(), "ls=ls --color");
        assert_eq!(ls.origin.code.start_line_number.get(), 1);
        assert_eq!(*ls.origin.code.source, Source::Unknown);
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
