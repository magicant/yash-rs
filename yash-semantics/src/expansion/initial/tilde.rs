// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Tilde expansion semantics

use crate::expansion::attr::AttrChar;
use crate::expansion::attr::Origin;
use yash_env::variable::Value;
use yash_env::variable::Variable;
use yash_env::Env;
use yash_env::System;

fn into_attr_chars<I>(i: I) -> Vec<AttrChar>
where
    I: IntoIterator<Item = char>,
{
    i.into_iter()
        .map(|c| AttrChar {
            value: c,
            origin: Origin::HardExpansion,
            is_quoted: false,
            is_quoting: false,
        })
        .collect()
}

/// Performs tilde expansion.
pub fn expand(name: &str, env: &Env) -> Vec<AttrChar> {
    if name.is_empty() {
        let result = match env.variables.get("HOME") {
            Some(Variable {
                value: Some(Value::Scalar(value)),
                ..
            }) => value,
            _ => "~",
        };
        into_attr_chars(result.chars())
    } else {
        if let Ok(Some(path)) = env.system.getpwnam_dir(name) {
            if let Ok(path) = path.into_os_string().into_string() {
                return into_attr_chars(path.chars());
            }
        }
        into_attr_chars(std::iter::once('~').chain(name.chars()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use yash_env::variable::Scope;
    use yash_env::VirtualSystem;

    #[test]
    fn empty_name_with_scalar_home() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new("HOME", Scope::Global)
            .assign("/home/foobar", None)
            .unwrap();

        let expansion = expand("", &env);
        let value: String = expansion.iter().copied().map(|c| c.value).collect();
        assert_eq!(value, "/home/foobar");
        for c in expansion {
            assert!(!c.is_quoted);
            assert!(!c.is_quoting);
            assert_eq!(c.origin, Origin::HardExpansion);
        }
    }

    #[test]
    fn empty_name_with_undefined_home() {
        let env = Env::new_virtual();
        assert_eq!(
            expand("", &env),
            [AttrChar {
                value: '~',
                origin: Origin::HardExpansion,
                is_quoted: false,
                is_quoting: false
            }]
        );
    }

    #[test]
    fn empty_name_with_array_home() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new("HOME", Scope::Global)
            .assign(Value::Array(vec![]), None)
            .unwrap();

        assert_eq!(
            expand("", &env),
            [AttrChar {
                value: '~',
                origin: Origin::HardExpansion,
                is_quoted: false,
                is_quoting: false
            }]
        );
    }

    #[test]
    fn existing_user_home_directory() {
        let system = Box::new(VirtualSystem::new());
        system
            .state
            .borrow_mut()
            .home_dirs
            .insert("love".to_string(), PathBuf::from("/usr/home/love"));
        let env = Env::with_system(system);

        let expansion = expand("love", &env);
        let value: String = expansion.iter().copied().map(|c| c.value).collect();
        assert_eq!(value, "/usr/home/love");
        for c in expansion {
            assert!(!c.is_quoted);
            assert!(!c.is_quoting);
            assert_eq!(c.origin, Origin::HardExpansion);
        }
    }

    #[test]
    fn non_existing_user_home_directory() {
        let env = Env::new_virtual();

        let expansion = expand("love", &env);
        let value: String = expansion.iter().copied().map(|c| c.value).collect();
        assert_eq!(value, "~love");
        for c in expansion {
            assert!(!c.is_quoted);
            assert!(!c.is_quoting);
            assert_eq!(c.origin, Origin::HardExpansion);
        }
    }

    // TODO other forms of tilde expansion
}
