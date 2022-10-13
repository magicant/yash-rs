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

//! Parameter expansion trim semantics

use super::Env;
use super::Error;
use crate::expansion::attr::AttrChar;
use crate::expansion::initial::expand;
use yash_env::variable::Value::{self, Array, Scalar};
use yash_fnmatch::Config;
use yash_fnmatch::Pattern;
use yash_fnmatch::PatternChar;
use yash_syntax::syntax::Trim;
use yash_syntax::syntax::TrimLength::{Longest, Shortest};
use yash_syntax::syntax::TrimSide::{Prefix, Suffix};

// TODO Merge implementation with command_impl/compound_command/case.rs
/// Converts unquoted backslashes to quoting characters.
///
/// Sets the `is_quoting` flag of unquoted backslashes and the `is_quoted` flag
/// of their following characters.
fn apply_escapes(chars: &mut [AttrChar]) {
    for j in 1..chars.len() {
        let i = j - 1;
        if chars[i].value == '\\' && !chars[i].is_quoting && !chars[i].is_quoted {
            chars[i].is_quoting = true;
            chars[j].is_quoted = true;
        }
    }
}

// TODO Merge implementation with command_impl/compound_command/case.rs
fn to_pattern_chars(chars: &[AttrChar]) -> impl Iterator<Item = PatternChar> + Clone + '_ {
    chars.iter().filter_map(|c| {
        if c.is_quoting {
            None
        } else if c.is_quoted {
            Some(PatternChar::Literal(c.value))
        } else {
            Some(PatternChar::Normal(c.value))
        }
    })
}

/// Applies the trim modifier to the value.
pub async fn apply(env: &mut Env<'_>, trim: &Trim, value: &mut Value) -> Result<(), Error> {
    let expansion = expand(env, &trim.pattern).await?;
    let mut pattern = expansion.ifs_join(&env.inner.variables);
    apply_escapes(&mut pattern);

    let mut config = Config::default();
    match trim.side {
        Prefix => {}
        /*TODO anchor_begin */
        Suffix => config.anchor_end = true,
    }
    match trim.length {
        Shortest => config.shortest_match = true,
        Longest => (),
    }
    let pattern = match Pattern::parse_with_config(to_pattern_chars(&pattern), config) {
        Ok(parse) => parse,
        Err(_error) => {
            // Treat the broken pattern as a valid pattern that does not match anything
            return Ok(());
        }
    };

    match value {
        Scalar(value) => {
            if let Some(range) = pattern.find(value) {
                value.drain(range);
            }
        }
        Array(array) => {
            for value in array {
                if let Some(range) = pattern.find(value) {
                    value.drain(range);
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;

    #[test]
    fn shortest_prefix_with_scalar() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let trim = Trim {
            side: Prefix,
            length: Shortest,
            pattern: "*2".parse().unwrap(),
        };
        let mut value = Value::scalar("123123123");
        let result = apply(&mut env, &trim, &mut value).now_or_never().unwrap();
        assert_eq!(result, Ok(()));
        assert_eq!(value, Value::scalar("3123123"));
    }

    #[test]
    fn shortest_prefix_with_array() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let trim = Trim {
            side: Prefix,
            length: Shortest,
            pattern: "*2".parse().unwrap(),
        };
        let mut value = Value::array(["0", "12321", "112211"]);
        let result = apply(&mut env, &trim, &mut value).now_or_never().unwrap();
        assert_eq!(result, Ok(()));
        assert_eq!(value, Value::array(["0", "321", "211"]));
    }

    #[test]
    fn longest_prefix() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let trim = Trim {
            side: Prefix,
            length: Longest,
            pattern: "*2".parse().unwrap(),
        };
        let mut value = Value::scalar("123123123");
        let result = apply(&mut env, &trim, &mut value).now_or_never().unwrap();
        assert_eq!(result, Ok(()));
        assert_eq!(value, Value::scalar("3"));
    }

    #[test]
    #[ignore] // TODO
    fn shortest_suffix() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let trim = Trim {
            side: Suffix,
            length: Shortest,
            pattern: "2*".parse().unwrap(),
        };
        let mut value = Value::scalar("123123123");
        let result = apply(&mut env, &trim, &mut value).now_or_never().unwrap();
        assert_eq!(result, Ok(()));
        assert_eq!(value, Value::scalar("1231231"));
    }

    #[test]
    fn longest_suffix() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let trim = Trim {
            side: Suffix,
            length: Longest,
            pattern: "2*".parse().unwrap(),
        };
        let mut value = Value::scalar("123123123");
        let result = apply(&mut env, &trim, &mut value).now_or_never().unwrap();
        assert_eq!(result, Ok(()));
        assert_eq!(value, Value::scalar("1"));
    }
}
