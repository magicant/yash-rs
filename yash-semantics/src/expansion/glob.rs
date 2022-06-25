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

//! Pathname expansion
//!
//! Pathname expansion (a.k.a. globbing) scans directories and produces
//! pathnames matching the input field.
//!
//! # Pattern syntax
//!
//! An input field is split by `/`, and each component is parsed as a pattern
//! that may contain the following non-literal elements:
//!
//! - `?`
//! - `*`
//! - Bracket expression (a set of characters enclosed in brackets)
//!
//! Refer to the [`yash-fnmatch`](yash_fnmatch) crate for pattern syntax and
//! semantics details.
//!
//! # Directory scanning
//!
//! The expansion scans directories corresponding to components containing any
//! non-literal elements above. The scan requires read permission for the
//! directory. For components that have only literal characters, no scan is
//! performed. Search permissions for all ancestor directories are needed to
//! check if the file exists referred to by the resulting pathname.
//!
//! # Results
//!
//! Pathname expansion returns pathnames that have matched the input pattern,
//! sorted alphabetically. Any errors are silently ignored. If directory
//! scanning produces no pathnames, the input pattern is returned intact. (TODO:
//! the null-glob option)
//!
//! If the input field contains no non-literal elements subject to pattern
//! matching at all, the result is the input intact.

use super::attr::AttrChar;
use super::attr::AttrField;
use super::attr::Origin;
use std::ffi::CString;
use std::iter::Once;
use std::marker::PhantomData;
use yash_env::semantics::Field;
use yash_env::system::Mode;
use yash_env::system::OFlag;
use yash_env::Env;
use yash_env::System;
use yash_fnmatch::Config;
use yash_fnmatch::Pattern;
use yash_fnmatch::PatternChar;
use yash_syntax::source::Location;

#[derive(Debug)]
enum Inner {
    One(Once<Field>),
    Many(std::vec::IntoIter<Field>),
}

impl From<Field> for Inner {
    fn from(field: Field) -> Self {
        Inner::One(std::iter::once(field))
    }
}

/// Iterator that provides results of parameter expansion
///
/// This iterator is created with the [`glob`] function.
#[derive(Debug)]
pub struct Glob<'a> {
    /// Dummy to allow retaining a mutable reference to `Env` in the future
    ///
    /// The current [`glob`] implementation pre-computes all results before
    /// returning a `Glob`. The future implementation may optimize by using a
    /// [generator], which will need a real reference to `Env`.
    ///
    /// [generator]: https://github.com/rust-lang/rust/issues/43122
    env: PhantomData<&'a mut Env>,

    inner: Inner,
}

impl From<Inner> for Glob<'_> {
    fn from(inner: Inner) -> Self {
        Glob {
            env: PhantomData,
            inner,
        }
    }
}

impl Iterator for Glob<'_> {
    type Item = Field;
    fn next(&mut self) -> Option<Field> {
        match &mut self.inner {
            Inner::One(once) => once.next(),
            Inner::Many(many) => many.next(),
        }
    }
}

/// Converts a field to a glob pattern.
fn to_pattern(field: &[AttrChar]) -> Option<Pattern> {
    let chars = field.iter().filter_map(|c| {
        if c.is_quoting {
            None
        } else if c.is_quoted || c.origin == Origin::HardExpansion {
            Some(PatternChar::Literal(c.value))
        } else {
            Some(PatternChar::Normal(c.value))
        }
    });
    let mut config = Config::default();
    config.anchor_begin = true;
    config.anchor_end = true;
    config.literal_period = true;
    Pattern::parse_with_config(chars, config).ok()
}

fn remove_quotes_and_strip(chars: &[AttrChar]) -> impl Iterator<Item = char> + '_ {
    use super::attr_strip::Strip;
    use super::quote_removal::skip_quotes;
    skip_quotes(chars.iter().copied()).strip()
}

#[derive(Debug)]
struct SearchEnv<'e> {
    env: &'e mut Env,
    prefix: String,
    origin: Location,
    results: Vec<Field>,
}

impl SearchEnv<'_> {
    /// Recursively searches directories for matching pathnames.
    fn search_dir(&mut self, suffix: &[AttrChar]) {
        let (this, new_suffix) = match suffix.iter().position(|c| c.value == '/') {
            None => (suffix, None),
            Some(index) => (&suffix[..index], Some(&suffix[index + 1..])),
        };

        match to_pattern(this).map(Pattern::into_literal) {
            None => {
                self.push_component(new_suffix, |prefix| {
                    prefix.extend(remove_quotes_and_strip(this))
                });
            }
            Some(Ok(literal)) => {
                self.push_component(new_suffix, |prefix| prefix.push_str(&literal));
            }
            Some(Err(pattern)) => {
                let dir_path = if self.prefix.is_empty() {
                    // TODO CString::new(".")
                    CString::new("/").unwrap()
                } else {
                    // TODO What if prefix contains a nul byte?
                    CString::new(self.prefix.as_str()).unwrap()
                };

                if let Ok(mut dir) = self.env.system.opendir(&dir_path) {
                    while let Ok(Some(entry)) = dir.next() {
                        // TODO Handle name.as_str error
                        let name = entry.name.to_str().unwrap();
                        if pattern.is_match(name) {
                            self.push_component(new_suffix, |prefix| prefix.push_str(name));
                        }
                    }
                }
            }
        }
    }

    fn file_exists(&mut self) -> bool {
        // TODO Use "stat" rather than "open" and "close". O_PATH is
        // Linux-specific and non-portable.
        let path = match CString::new(self.prefix.as_str()) {
            Ok(path) => path,
            Err(_) => return false,
        };
        let fd = self.env.system.open(&path, OFlag::O_PATH, Mode::empty());
        let file_exists = fd.is_ok();
        if let Ok(fd) = fd {
            let _ = self.env.system.close(fd);
        }
        file_exists
    }

    /// Pushes a pathname component to `prefix` and start processing the next suffix
    /// component.
    fn push_component<F>(&mut self, suffix: Option<&[AttrChar]>, push: F)
    where
        F: FnOnce(&mut String),
    {
        let old_prefix_len = self.prefix.len();
        push(&mut self.prefix);

        match suffix {
            None => {
                // TODO Don't need to check the file existence if the last path
                // component comes from a directory entry
                if self.file_exists() {
                    self.results.push(Field {
                        value: self.prefix.clone(),
                        origin: self.origin.clone(),
                    });
                }
            }
            Some(suffix) => {
                self.prefix.push('/');
                self.search_dir(suffix);
            }
        }

        self.prefix.truncate(old_prefix_len);
    }
}

/// Performs parameter expansion.
///
/// This function returns an iterator that yields fields resulting from the
/// expansion.
pub fn glob(env: &mut Env, field: AttrField) -> Glob {
    // TODO Quick check for *, ?, [ containment

    let mut search_env = SearchEnv {
        env,
        prefix: String::with_capacity(1024 /*nix::libc::PATH_MAX*/),
        origin: field.origin,
        results: Vec::new(),
    };
    search_env.search_dir(&field.chars);

    let mut results = search_env.results;
    Glob::from(if results.is_empty() {
        let field = AttrField {
            chars: field.chars,
            origin: search_env.origin,
        };
        Inner::from(field.remove_quotes_and_strip())
    } else {
        results.sort_unstable_by(|a, b| a.value.cmp(&b.value));
        Inner::Many(results.into_iter())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expansion::AttrChar;
    use crate::expansion::Origin;
    use std::path::Path;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_syntax::source::Location;

    fn dummy_attr_field(s: &str) -> AttrField {
        let chars = s
            .chars()
            .map(|c| AttrChar {
                value: c,
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            })
            .collect();
        let origin = Location::dummy("");
        AttrField { chars, origin }
    }

    fn env_with_dummy_files<I, P>(paths: I) -> Env
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        for path in paths {
            state.file_system.save(path, Rc::default()).unwrap();
        }
        drop(state);
        Env::with_system(Box::new(system))
    }

    #[test]
    fn literal_field() {
        let mut env = Env::new_virtual();
        let f = dummy_attr_field("abc");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "abc");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn quoting_characters_are_removed() {
        let mut env = Env::new_virtual();
        let mut f = dummy_attr_field("aXbcYde");
        f.chars[1].is_quoting = true;
        f.chars[4].is_quoting = true;
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "abcde");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn quoted_characters_do_not_expand() {
        let mut env = env_with_dummy_files(["foo.exe"]);
        let mut f = dummy_attr_field("foo.*");
        f.chars[4].is_quoted = true;
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "foo.*");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn characters_from_hard_expansion_do_not_expand() {
        let mut env = env_with_dummy_files(["foo.exe"]);
        let mut f = dummy_attr_field("foo.*");
        f.chars[4].origin = Origin::HardExpansion;
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "foo.*");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn single_component_pattern_no_match() {
        let mut env = env_with_dummy_files(["foo.exe"]);
        let f = dummy_attr_field("*.txt");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "*.txt");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn single_component_pattern_single_match() {
        let mut env = env_with_dummy_files(["foo.exe", "foo.txt"]);
        let f = dummy_attr_field("*.txt");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "foo.txt");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn single_component_pattern_many_matches() {
        let mut env = env_with_dummy_files(["foo.exe", "foo.txt"]);
        let f = dummy_attr_field("foo.*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "foo.exe");
        assert_eq!(i.next().unwrap().value, "foo.txt");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn absolute_path_single_component_pattern_many_matches() {
        let mut env = env_with_dummy_files(["/foo.exe", "/foo.txt"]);
        let f = dummy_attr_field("/foo.*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "/foo.exe");
        assert_eq!(i.next().unwrap().value, "/foo.txt");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn multi_component_pattern_ending_with_pattern() {
        let mut env = env_with_dummy_files([
            "a/a/a/a", "a/a/a/b", "a/a/a/no", "a/a/b/a", "a/b/a/a", "a/b/a/b", "a/b/a/no",
            "a/no/a/a", "b/a/a/a",
        ]);
        let f = dummy_attr_field("a/?/a/?");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "a/a/a/a");
        assert_eq!(i.next().unwrap().value, "a/a/a/b");
        assert_eq!(i.next().unwrap().value, "a/b/a/a");
        assert_eq!(i.next().unwrap().value, "a/b/a/b");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn multi_component_pattern_ending_with_literal() {
        let mut env = env_with_dummy_files([
            "/a/a/a/a",
            "/a/a/a/b",
            "/a/a/b/a",
            "/a/a/no/a",
            "/a/b/a/a",
            "/b/a/a/a",
            "/b/a/b/b",
            "/b/a/c",
            "/b/a/no",
            "/c/a",
            "/no/a",
        ]);
        let f = dummy_attr_field("/?/a/?/a");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "/a/a/a/a");
        assert_eq!(i.next().unwrap().value, "/a/a/b/a");
        assert_eq!(i.next().unwrap().value, "/b/a/a/a");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn multi_component_pattern_ending_with_slash() {
        let mut env = env_with_dummy_files(["a/a/_", "a/b/_", "a/c"]);
        let f = dummy_attr_field("a/*/");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "a/a/");
        assert_eq!(i.next().unwrap().value, "a/b/");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn multi_component_pattern_with_adjacent_slashes() {
        let mut env = env_with_dummy_files(["a/b", "b/a"]);
        let f = dummy_attr_field("?//?");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "a//b");
        assert_eq!(i.next().unwrap().value, "b//a");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn invalid_pattern_remains_intact() {
        let mut env = env_with_dummy_files(["foo.txt"]);
        let f = dummy_attr_field("*[[:wrong:]]*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "*[[:wrong:]]*");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn slash_between_brackets() {
        let mut env = env_with_dummy_files(["abd", "a/d"]);
        let f = dummy_attr_field("a[b/c]d");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().value, "a[b/c]d");
        assert_eq!(i.next(), None);
    }
}
