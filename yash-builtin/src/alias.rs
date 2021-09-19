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

use std::future::ready;
use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use std::rc::Rc;
use yash_env::builtin::Result;
use yash_env::exec::ExitStatus;
use yash_env::expansion::Field;
use yash_syntax::alias::{AliasSet, HashEntry};

/// Part of the shell execution environment the alias built-in depends on.
pub trait Env {
    /// Accesses the alias set in the environment.
    fn alias_set(&mut self) -> &mut Rc<AliasSet>;
    // TODO stdout, stderr
}

impl Env for yash_env::Env {
    fn alias_set(&mut self) -> &mut Rc<AliasSet> {
        &mut self.aliases
    }
}

/// Implementation of the alias built-in.
pub fn builtin_main_sync<E: Env>(env: &mut E, args: Vec<Field>) -> Result {
    // TODO support options
    // TODO print alias definitions if there are no operands

    let mut args = args.into_iter();
    args.next(); // ignore the first argument, which is the command name

    if args.as_ref().is_empty() {
        for alias in env.alias_set().as_ref() {
            // TODO should print via IoEnv rather than directly to stdout
            println!("{}={}", &alias.0.name, &alias.0.replacement);
        }
        return (ExitStatus::SUCCESS, Continue(()));
    }

    for Field { value, origin } in args {
        if let Some(eq_index) = value.find('=') {
            let name = value[..eq_index].to_owned();
            // TODO reject invalid name
            let replacement = value[eq_index + 1..].to_owned();
            let entry = HashEntry::new(name, replacement, false, origin);
            Rc::make_mut(&mut env.alias_set()).insert(entry);
        } else {
            // TODO print alias definition
        }
    }

    (ExitStatus::SUCCESS, Continue(()))
}

/// Implementation of the alias built-in.
///
/// This function calls [`builtin_main_sync`] and wraps the result in a `Future`.
pub fn builtin_main(
    env: &mut yash_env::Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result>>> {
    Box::pin(ready(builtin_main_sync(env, args)))
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use yash_syntax::source::Location;
    use yash_syntax::source::Source;

    #[derive(Default)]
    struct DummyEnv {
        aliases: Rc<AliasSet>,
    }

    impl Env for DummyEnv {
        fn alias_set(&mut self) -> &mut Rc<AliasSet> {
            &mut self.aliases
        }
    }

    #[test]
    fn builtin_defines_alias() {
        let mut env = DummyEnv::default();
        let arg0 = Field::dummy("");
        let arg1 = Field::dummy("foo=bar baz");
        let args = vec![arg0, arg1];

        let result = builtin_main_sync(&mut env, args);
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let aliases = env.aliases.as_ref();
        assert_eq!(aliases.len(), 1);

        let alias = aliases.get("foo").unwrap().0.as_ref();
        assert_eq!(alias.name, "foo");
        assert_eq!(alias.replacement, "bar baz");
        assert_eq!(alias.global, false);
        assert_eq!(alias.origin.line.value, "foo=bar baz");
        assert_eq!(alias.origin.line.number.get(), 1);
        assert_eq!(alias.origin.line.source, Source::Unknown);
        assert_eq!(alias.origin.column.get(), 1);
    }

    #[test]
    fn builtin_defines_many_aliases() {
        let mut env = DummyEnv::default();
        let arg0 = Field::dummy("alias");
        let arg1 = Field::dummy("abc=xyz");
        let arg2 = Field::dummy("yes=no");
        let arg3 = Field::dummy("ls=ls --color");
        let args = vec![arg0, arg1, arg2, arg3];

        let result = builtin_main_sync(&mut env, args);
        assert_eq!(result, (ExitStatus::SUCCESS, Continue(())));

        let aliases = env.aliases.as_ref();
        assert_eq!(aliases.len(), 3);

        let abc = aliases.get("abc").unwrap().0.as_ref();
        assert_eq!(abc.name, "abc");
        assert_eq!(abc.replacement, "xyz");
        assert_eq!(abc.global, false);
        assert_eq!(abc.origin.line.value, "abc=xyz");
        assert_eq!(abc.origin.line.number.get(), 1);
        assert_eq!(abc.origin.line.source, Source::Unknown);
        assert_eq!(abc.origin.column.get(), 1);

        let yes = aliases.get("yes").unwrap().0.as_ref();
        assert_eq!(yes.name, "yes");
        assert_eq!(yes.replacement, "no");
        assert_eq!(yes.global, false);
        assert_eq!(yes.origin.line.value, "yes=no");
        assert_eq!(yes.origin.line.number.get(), 1);
        assert_eq!(yes.origin.line.source, Source::Unknown);
        assert_eq!(yes.origin.column.get(), 1);

        let ls = aliases.get("ls").unwrap().0.as_ref();
        assert_eq!(ls.name, "ls");
        assert_eq!(ls.replacement, "ls --color");
        assert_eq!(ls.global, false);
        assert_eq!(ls.origin.line.value, "ls=ls --color");
        assert_eq!(ls.origin.line.number.get(), 1);
        assert_eq!(ls.origin.line.source, Source::Unknown);
        assert_eq!(ls.origin.column.get(), 1);
    }

    #[test]
    fn builtin_prints_all_aliases() {
        let mut env = DummyEnv::default();
        let aliases = Rc::make_mut(&mut env.aliases);
        aliases.insert(HashEntry::new(
            "foo".to_string(),
            "bar".to_string(),
            false,
            Location::dummy(""),
        ));
        aliases.insert(HashEntry::new(
            "hello".to_string(),
            "world".to_string(),
            false,
            Location::dummy(""),
        ));
        // TODO builtin should print to IoEnv rather than real standard output
    }
    // TODO test case with global aliases
}
