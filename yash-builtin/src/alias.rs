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
use std::pin::Pin;
use std::rc::Rc;
use yash_core::alias::*;
use yash_core::builtin::Result;
use yash_core::env::Env;
use yash_core::expansion::Field;

/// Implementation of the alias built-in.
pub fn alias_builtin(env: &mut dyn Env, args: Vec<Field>) -> Result {
    // TODO support options
    // TODO print alias definitions if there are no operands

    let mut args = args.into_iter();
    args.next(); // ignore the first argument, which is the command name

    if args.as_ref().is_empty() {
        for alias in env.aliases().as_ref() {
            // TODO should print via IoEnv rather than directly to stdout
            println!("{}={}", &alias.0.name, &alias.0.replacement);
        }
        return (0, None);
    }

    for Field { value, origin } in args {
        if let Some(eq_index) = value.find('=') {
            let name = value[..eq_index].to_owned();
            // TODO reject invalid name
            let replacement = value[eq_index + 1..].to_owned();
            Rc::make_mut(env.aliases_mut()).insert(HashEntry::new(
                name,
                replacement,
                false,
                origin,
            ));
        } else {
            // TODO print alias definition
        }
    }

    (0, None)
}

/// Implementation of the alias built-in.
///
/// This function calls [`alias_builtin`] and wraps the result in a `Future`.
pub fn alias_builtin_async(
    env: &mut dyn Env,
    args: Vec<Field>,
) -> Pin<Box<dyn Future<Output = Result>>> {
    Box::pin(ready(alias_builtin(env, args)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_core::env::AliasEnv;
    use yash_core::env::SimEnv;
    use yash_core::source::Location;
    use yash_core::source::Source;

    #[test]
    fn alias_builtin_defines_alias() {
        let mut env = SimEnv::new();
        let arg0 = Field::dummy("".to_string());
        let arg1 = Field::dummy("foo=bar baz".to_string());
        let args = vec![arg0, arg1];

        let result = alias_builtin(&mut env, args);
        assert_eq!(result, (0, None));

        let aliases = env.aliases().as_ref();
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
    fn alias_builtin_defines_many_aliases() {
        let mut env = SimEnv::new();
        let arg0 = Field::dummy("alias".to_string());
        let arg1 = Field::dummy("abc=xyz".to_string());
        let arg2 = Field::dummy("yes=no".to_string());
        let arg3 = Field::dummy("ls=ls --color".to_string());
        let args = vec![arg0, arg1, arg2, arg3];

        let result = alias_builtin(&mut env, args);
        assert_eq!(result, (0, None));

        let aliases = env.aliases().as_ref();
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
    fn alias_builtin_prints_all_aliases() {
        let mut env = SimEnv::new();
        let aliases = Rc::make_mut(env.aliases_mut());
        aliases.insert(HashEntry::new(
            "foo".to_string(),
            "bar".to_string(),
            false,
            Location::dummy("".to_string()),
        ));
        aliases.insert(HashEntry::new(
            "hello".to_string(),
            "world".to_string(),
            false,
            Location::dummy("".to_string()),
        ));
        // TODO alias_builtin should print to IoEnv rather than real standard output
    }
    // TODO test case with global aliases
}
