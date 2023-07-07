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
//! TODO Elaborate

use yash_env::builtin::Result;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_syntax::alias::HashEntry;

// TODO Split into syntax and semantics submodules

/// Entry point for executing the `alias` built-in
pub fn main(env: &mut Env, args: Vec<Field>) -> Result {
    // TODO support options
    // TODO print alias definitions if there are no operands

    if args.is_empty() {
        for alias in &env.aliases {
            // TODO should print via IoEnv rather than directly to stdout
            println!("{}={}", &alias.0.name, &alias.0.replacement);
        }
        return ExitStatus::SUCCESS.into();
    }

    for Field { value, origin } in args {
        if let Some(eq_index) = value.find('=') {
            let name = value[..eq_index].to_owned();
            // TODO reject invalid name
            let replacement = value[eq_index + 1..].to_owned();
            let entry = HashEntry::new(name, replacement, false, origin);
            env.aliases.replace(entry);
        } else {
            // TODO print alias definition
        }
    }

    ExitStatus::SUCCESS.into()
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use yash_syntax::source::Location;
    use yash_syntax::source::Source;

    #[test]
    fn builtin_defines_alias() {
        let mut env = Env::new_virtual();
        let args = Field::dummies(["foo=bar baz"]);

        let result = main(&mut env, args);
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

        let result = main(&mut env, args);
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

        let result = main(&mut env, args);
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        let args = Field::dummies(["foo=2"]);

        let result = main(&mut env, args);
        assert_eq!(result, Result::new(ExitStatus::SUCCESS));

        assert_eq!(env.aliases.len(), 1);

        let alias = env.aliases.get("foo").unwrap().0.as_ref();
        assert_eq!(alias.name, "foo");
        assert_eq!(alias.replacement, "2");
        assert_eq!(alias.global, false);
        // TODO Test with the global option
        // assert_eq!(alias.global, true);
    }

    #[test]
    fn builtin_prints_all_aliases() {
        let mut env = Env::new_virtual();
        env.aliases.insert(HashEntry::new(
            "foo".to_string(),
            "bar".to_string(),
            false,
            Location::dummy(""),
        ));
        env.aliases.insert(HashEntry::new(
            "hello".to_string(),
            "world".to_string(),
            false,
            Location::dummy(""),
        ));
        // TODO builtin should print to IoEnv rather than real standard output
    }
    // TODO test case with global aliases
}
