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

/// Performs tilde expansion.
pub fn expand(_name: &str, env: &Env) -> Vec<AttrChar> {
    match env.variables.get("HOME") {
        Some(Variable {
            value: Value::Scalar(value),
            ..
        }) => value,
        _ => "~",
    }
    .chars()
    .map(|c| AttrChar {
        value: c,
        origin: Origin::HardExpansion,
        is_quoted: false,
        is_quoting: false,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::variable::Scope;
    use yash_env::variable::Variable;

    #[test]
    fn empty_name_with_scalar_home() {
        let mut env = Env::new_virtual();
        env.variables
            .assign(
                Scope::Global,
                "HOME".to_string(),
                Variable::new("/home/foobar"),
            )
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
            .assign(
                Scope::Global,
                "HOME".to_string(),
                Variable::new_empty_array(),
            )
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

    // TODO other forms of tilde expansion
}
