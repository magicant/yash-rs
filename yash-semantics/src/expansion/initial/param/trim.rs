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
use crate::expansion::attr::fnmatch::apply_escapes;
use crate::expansion::attr::fnmatch::to_pattern_chars;
use crate::expansion::initial::Expand as _;
use yash_env::variable::Value::{self, Array, Scalar};
use yash_fnmatch::Config;
use yash_fnmatch::Pattern;
use yash_syntax::syntax::Trim;
use yash_syntax::syntax::TrimLength::{Longest, Shortest};
use yash_syntax::syntax::TrimSide::{Prefix, Suffix};

fn trim_value(pattern: &Pattern, value: &mut String) {
    let config = pattern.config();
    let find = if config.anchor_end && config.shortest_match {
        Pattern::rfind
    } else {
        Pattern::find
    };
    if let Some(range) = find(pattern, value) {
        value.drain(range);
    }
}

/// Applies the trim modifier to the value.
pub async fn apply(env: &mut Env<'_>, trim: &Trim, value: &mut Value) -> Result<(), Error> {
    let expansion = trim.pattern.expand(env).await?;
    let mut pattern = expansion.ifs_join(&env.inner.variables);
    apply_escapes(&mut pattern);

    let mut config = Config::default();
    match trim.side {
        Prefix => config.anchor_begin = true,
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
        Scalar(value) => trim_value(&pattern, value),
        Array(array) => {
            for value in array {
                trim_value(&pattern, value);
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
    fn shortest_prefix_unmatched() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let trim = Trim {
            side: Prefix,
            length: Shortest,
            pattern: "2*".parse().unwrap(),
        };
        let mut value = Value::scalar("123123123");
        let result = apply(&mut env, &trim, &mut value).now_or_never().unwrap();
        assert_eq!(result, Ok(()));
        assert_eq!(value, Value::scalar("123123123"));
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

    #[test]
    fn longest_suffix_unmatched() {
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let trim = Trim {
            side: Suffix,
            length: Longest,
            pattern: "*2".parse().unwrap(),
        };
        let mut value = Value::scalar("123123123");
        let result = apply(&mut env, &trim, &mut value).now_or_never().unwrap();
        assert_eq!(result, Ok(()));
        assert_eq!(value, Value::scalar("123123123"));
    }
}
