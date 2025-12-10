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
use std::borrow::Cow;
use std::ffi::CString;
use yash_env::Env;
use yash_env::System;
use yash_env::variable::HOME;

/// Computes the main result of tilde expansion.
fn expand_body<'n: 'r, 'e: 'r, 'r, S: System>(name: &'n str, env: &'e Env<S>) -> Cow<'r, str> {
    if name.is_empty() {
        return Cow::Borrowed(env.variables.get_scalar(HOME).unwrap_or("~"));
    }
    if let Ok(name) = CString::new(name) {
        if let Ok(Some(path)) = env.system.getpwnam_dir(&name) {
            if let Ok(path) = path.into_unix_string().into_string() {
                return Cow::Owned(path);
            }
        }
    }
    Cow::Owned(format!("~{name}"))
}

/// Produces the final result of tilde expansion.
fn finish(mut chars: &str, followed_by_slash: bool) -> Vec<AttrChar> {
    if followed_by_slash {
        if let Some(stripped) = chars.strip_suffix('/') {
            chars = stripped;
        }
    }

    let mut attr_chars: Vec<AttrChar> = chars
        .chars()
        .map(|c| AttrChar {
            value: c,
            origin: Origin::HardExpansion,
            is_quoted: false,
            is_quoting: false,
        })
        .collect();

    if attr_chars.is_empty() {
        // add a dummy quote to keep the result from removal in field splitting
        attr_chars.push(AttrChar {
            value: '"',
            origin: Origin::HardExpansion,
            is_quoted: false,
            is_quoting: true,
        });
    }

    attr_chars
}

/// Performs tilde expansion.
pub fn expand<S: System>(name: &str, followed_by_slash: bool, env: &Env<S>) -> Vec<AttrChar> {
    let chars = expand_body(name, env);
    finish(&chars, followed_by_slash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::VirtualSystem;
    use yash_env::path::PathBuf;
    use yash_env::variable::Scope;
    use yash_env::variable::Value;

    #[test]
    fn empty_name_with_scalar_home() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new(HOME, Scope::Global)
            .assign("/home/foobar", None)
            .unwrap();

        let expansion = expand("", false, &env);
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
            expand("", false, &env),
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
            .get_or_new(HOME, Scope::Global)
            .assign(Value::Array(vec![]), None)
            .unwrap();

        assert_eq!(
            expand("", false, &env),
            [AttrChar {
                value: '~',
                origin: Origin::HardExpansion,
                is_quoted: false,
                is_quoting: false
            }]
        );
    }

    #[test]
    fn empty_name_with_empty_home() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new(HOME, Scope::Global)
            .assign("", None)
            .unwrap();

        assert_eq!(
            expand("", false, &env),
            [AttrChar {
                value: '"',
                origin: Origin::HardExpansion,
                is_quoted: false,
                is_quoting: true
            }]
        );
    }

    #[test]
    fn existing_user_home_directory() {
        let system = VirtualSystem::new();
        system
            .state
            .borrow_mut()
            .home_dirs
            .insert("love".to_string(), PathBuf::from("/usr/home/love"));
        let env = Env::with_system(system);

        let expansion = expand("love", false, &env);
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

        let expansion = expand("love", false, &env);
        let value: String = expansion.iter().copied().map(|c| c.value).collect();
        assert_eq!(value, "~love");
        for c in expansion {
            assert!(!c.is_quoted);
            assert!(!c.is_quoting);
            assert_eq!(c.origin, Origin::HardExpansion);
        }
    }

    // TODO other forms of tilde expansion

    #[test]
    fn value_ending_with_slash_without_following_slash() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new(HOME, Scope::Global)
            .assign("/home/user/", None)
            .unwrap();

        let expansion = expand("", false, &env);
        let value: String = expansion.iter().copied().map(|c| c.value).collect();
        assert_eq!(value, "/home/user/");
    }

    #[test]
    fn value_not_ending_with_slash_with_following_slash() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new(HOME, Scope::Global)
            .assign("/home/user", None)
            .unwrap();

        let expansion = expand("", true, &env);
        let value: String = expansion.iter().copied().map(|c| c.value).collect();
        assert_eq!(value, "/home/user");
    }

    #[test]
    fn value_ending_with_slash_with_following_slash() {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new(HOME, Scope::Global)
            .assign("/home/user/", None)
            .unwrap();

        let expansion = expand("", true, &env);
        let value: String = expansion.iter().copied().map(|c| c.value).collect();
        assert_eq!(value, "/home/user");
    }
}
