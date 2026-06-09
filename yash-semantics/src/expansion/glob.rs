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
//! - `\`
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
use std::ops::ControlFlow::{self, Break, Continue};
use yash_env::Env;
use yash_env::option::State::Off;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::system::concurrency::{Select, WaitForSignals};
use yash_env::system::{AT_FDCWD, Dir as _, Fstat, Open, Signals};
use yash_fnmatch::Config;
use yash_fnmatch::Pattern;
use yash_fnmatch::PatternChar;
use yash_syntax::source::Location;

/// Error value indicating that pathname expansion was interrupted
///
/// This error is returned when a SIGINT signal is received while scanning
/// directories in an interactive shell with SIGINT defaulted. The contained
/// [`ExitStatus`] is derived from the SIGINT signal.
#[derive(Debug, Eq, PartialEq)]
pub struct Interrupted(pub ExitStatus);

impl From<Interrupted> for ExitStatus {
    fn from(intr: Interrupted) -> ExitStatus {
        intr.0
    }
}

#[derive(Debug)]
enum Inner {
    One(Once<Result<Field, Interrupted>>),
    Many(std::vec::IntoIter<Field>),
}

impl From<Field> for Inner {
    fn from(field: Field) -> Self {
        Inner::One(std::iter::once(Ok(field)))
    }
}

impl From<Interrupted> for Inner {
    fn from(intr: Interrupted) -> Self {
        Inner::One(std::iter::once(Err(intr)))
    }
}

impl From<Result<Field, Interrupted>> for Inner {
    fn from(result: Result<Field, Interrupted>) -> Self {
        Inner::One(std::iter::once(result))
    }
}

/// Iterator that provides results of pathname expansion
///
/// This iterator is created with the [`glob`] function.
#[derive(Debug)]
pub struct Glob<'a, S> {
    /// Dummy to allow retaining a mutable reference to `Env` in the future
    ///
    /// The current [`glob`] implementation pre-computes all results before
    /// returning a `Glob`. The future implementation may optimize by using a
    /// [generator], which will need a real reference to `Env`.
    ///
    /// [generator]: https://github.com/rust-lang/rust/issues/43122
    env: PhantomData<&'a mut Env<S>>,

    inner: Inner,
}

impl<S> From<Inner> for Glob<'_, S> {
    fn from(inner: Inner) -> Self {
        Glob {
            env: PhantomData,
            inner,
        }
    }
}

impl<S: Fstat + Open + Select + Signals + WaitForSignals> Iterator for Glob<'_, S> {
    type Item = Result<Field, Interrupted>;
    fn next(&mut self) -> Option<Result<Field, Interrupted>> {
        match &mut self.inner {
            Inner::One(once) => once.next(),
            Inner::Many(many) => many.next().map(Ok),
        }
    }
}

impl<S> Glob<'_, S> {
    /// An escape hatch to decompose the `Glob` iterator.
    ///
    /// This method will be removed when `Glob` is re-implemented with a
    /// generator.
    pub(super) fn try_into_vec_iter(
        self,
    ) -> Result<std::vec::IntoIter<Field>, Once<Result<Field, Interrupted>>> {
        match self.inner {
            Inner::One(once) => Err(once),
            Inner::Many(many) => Ok(many),
        }
    }
}

/// Converts a field to a glob pattern.
fn to_pattern(field: &[AttrChar]) -> Option<Pattern> {
    #[derive(Clone, Debug)]
    struct Chars<'a> {
        inner: std::slice::Iter<'a, AttrChar>,
        next_quoted: bool,
    }
    impl Iterator for Chars<'_> {
        type Item = PatternChar;
        fn next(&mut self) -> Option<PatternChar> {
            for c in &mut self.inner {
                let quoted = std::mem::replace(&mut self.next_quoted, false);
                if c.is_quoting {
                    continue;
                } else if quoted || c.is_quoted || c.origin == Origin::HardExpansion {
                    return Some(PatternChar::Literal(c.value));
                } else {
                    self.next_quoted = c.value == '\\';
                    return Some(PatternChar::Normal(c.value));
                }
            }
            None
        }
    }

    let chars = Chars {
        inner: field.iter(),
        next_quoted: false,
    };
    let mut config = Config::default();
    config.anchor_begin = true;
    config.anchor_end = true;
    config.literal_period = true;
    Pattern::parse_with_config(chars, config).ok()
}

fn remove_quotes_and_strip(chars: &[AttrChar]) -> impl Iterator<Item = char> + '_ {
    use super::attr_strip::Strip as _;
    use super::quote_removal::skip_quotes;
    skip_quotes(chars.iter().copied()).strip()
}

#[derive(Debug)]
struct SearchEnv<'e, S> {
    env: &'e mut Env<S>,
    interruptible: bool,
    prefix: String,
    origin: Location,
    results: Vec<Field>,
}

impl<S: Fstat + Open + Select + Signals + WaitForSignals> SearchEnv<'_, S> {
    /// Recursively searches directories for matching pathnames.
    fn search_dir(&mut self, suffix: &[AttrChar]) -> ControlFlow<Interrupted> {
        let (this, new_suffix) = match suffix.iter().position(|c| c.value == '/') {
            None => (suffix, None),
            Some(index) => (&suffix[..index], Some(&suffix[index + 1..])),
        };

        match to_pattern(this).map(Pattern::into_literal) {
            None => {
                self.push_component(new_suffix, false, |prefix| {
                    prefix.extend(remove_quotes_and_strip(this))
                })?;
            }
            Some(Ok(literal)) => {
                self.push_component(new_suffix, false, |prefix| prefix.push_str(&literal))?;
            }
            Some(Err(pattern)) => {
                let dir_path = if self.prefix.is_empty() {
                    c".".to_owned()
                } else if let Ok(dir_path) = CString::new(self.prefix.as_str()) {
                    dir_path
                } else {
                    return Continue(());
                };

                if let Ok(mut dir) = self.env.system.opendir(&dir_path) {
                    while let Ok(Some(entry)) = dir.next() {
                        if self.interruptible
                            && self
                                .env
                                .poll_signals()
                                .is_some_and(|sigs| sigs.contains(&S::SIGINT))
                        {
                            return Break(Interrupted(ExitStatus::from(S::SIGINT)));
                        }
                        if let Some(name) = entry.name.to_str()
                            && name != "."
                            && name != ".."
                            && pattern.is_match(name)
                        {
                            self.push_component(new_suffix, true, |prefix| prefix.push_str(name))?;
                        }
                    }
                }
            }
        }
        Continue(())
    }

    fn file_exists(&mut self) -> bool {
        let Ok(path) = CString::new(self.prefix.as_str()) else {
            return false;
        };
        self.env
            .system
            .fstatat(AT_FDCWD, &path, /* follow symlinks */ true)
            .is_ok()
    }

    /// Pushes a pathname component to `prefix` and starts processing the next
    /// suffix component.
    ///
    /// If `suffix` is `None`, the pathname is checked for existence and added
    /// to the results if it exists. If `file_exists` is `true`, the pathname is
    /// assumed to exist.
    ///
    /// `push` is a closure that appends the suffix to the prefix.
    fn push_component<F>(
        &mut self,
        suffix: Option<&[AttrChar]>,
        file_exists: bool,
        push: F,
    ) -> ControlFlow<Interrupted>
    where
        F: FnOnce(&mut String),
    {
        let old_prefix_len = self.prefix.len();
        push(&mut self.prefix);

        let result = match suffix {
            None => {
                if file_exists || self.file_exists() {
                    self.results.push(Field {
                        value: self.prefix.clone(),
                        origin: self.origin.clone(),
                    });
                }
                Continue(())
            }
            Some(suffix) => {
                self.prefix.push('/');
                self.search_dir(suffix)
            }
        };

        self.prefix.truncate(old_prefix_len);
        result
    }
}

/// Performs pathname expansion.
///
/// This function returns a [`Glob`] iterator yielding the expanded fields.
/// If a SIGINT signal is received while scanning directories in an interactive
/// shell with SIGINT defaulted, the iterator yields `Err(interrupted)` where
/// `interrupted` carries the exit status representing the signal. After
/// interruption, the iterator may or may not yield more results.
///
/// If the `Glob` option is `Off` in `env.options`, the expansion is skipped.
pub fn glob<S: Fstat + Open + Select + Signals + WaitForSignals>(
    env: &mut Env<S>,
    field: AttrField,
) -> Glob<'_, S> {
    if env.options.get(yash_env::option::Option::Glob) == Off {
        return Glob::from(Inner::from(field.remove_quotes_and_strip()));
    }

    // TODO Quick check for *, ?, [ containment

    let interruptible = env.is_interactive() && env.sigint_has_default_action();
    let mut search_env = SearchEnv {
        env,
        interruptible,
        prefix: String::with_capacity(1024 /*nix::libc::PATH_MAX*/),
        origin: field.origin,
        results: Vec::new(),
    };
    if let Break(interrupted) = search_env.search_dir(&field.chars) {
        return Glob::from(Inner::from(interrupted));
    }

    let mut results = search_env.results;
    let inner = if results.is_empty() {
        let field = AttrField {
            chars: field.chars,
            origin: search_env.origin,
        };
        Inner::from(field.remove_quotes_and_strip())
    } else {
        results.sort_unstable_by(|a, b| a.value.cmp(&b.value));
        Inner::Many(results.into_iter())
    };
    Glob::from(inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expansion::AttrChar;
    use crate::expansion::Origin;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::path::Path;
    use yash_env::str::UnixStr;
    use yash_env::system::Concurrent;
    use yash_env::system::Mode;
    use yash_env::system::r#virtual::SIGINT;
    use yash_env::trap::Action;
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

    fn env_with_dummy_files<I, P>(paths: I) -> Env<Rc<Concurrent<VirtualSystem>>>
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
        Env::with_system(Rc::new(Concurrent::new(system)))
    }

    #[test]
    fn literal_field() {
        let mut env = Env::new_virtual();
        let f = dummy_attr_field("abc");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "abc");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn backslash_escapes_next_char() {
        let mut env = env_with_dummy_files(["a", r"\a"]);
        // The backslash escapes the '?', so this is not a pattern.
        let f = dummy_attr_field(r"\?");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, r"\?");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn quoting_characters_are_removed() {
        let mut env = Env::new_virtual();
        let mut f = dummy_attr_field("aXbcYde");
        f.chars[1].is_quoting = true;
        f.chars[4].is_quoting = true;
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "abcde");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn quoted_characters_do_not_expand() {
        let mut env = env_with_dummy_files(["foo.exe"]);
        let mut f = dummy_attr_field("foo.*");
        f.chars[4].is_quoted = true;
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "foo.*");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn characters_from_hard_expansion_do_not_expand() {
        let mut env = env_with_dummy_files(["foo.exe"]);
        let mut f = dummy_attr_field("foo.*");
        f.chars[4].origin = Origin::HardExpansion;
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "foo.*");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn single_component_pattern_no_match() {
        let mut env = env_with_dummy_files(["foo.exe"]);
        let f = dummy_attr_field("*.txt");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "*.txt");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn single_component_pattern_single_match() {
        let mut env = env_with_dummy_files(["foo.exe", "foo.txt"]);
        let f = dummy_attr_field("*.txt");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "foo.txt");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn single_component_pattern_many_matches() {
        let mut env = env_with_dummy_files(["foo.exe", "foo.txt"]);
        let f = dummy_attr_field("foo.*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "foo.exe");
        assert_eq!(i.next().unwrap().unwrap().value, "foo.txt");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn no_pattern_matches_dot_or_dot_dot() {
        let mut env = env_with_dummy_files([".foo"]);
        let f = dummy_attr_field(".*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, ".foo");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn absolute_path_single_component_pattern_many_matches() {
        let mut env = env_with_dummy_files(["/foo.exe", "/foo.txt"]);
        let f = dummy_attr_field("/foo.*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "/foo.exe");
        assert_eq!(i.next().unwrap().unwrap().value, "/foo.txt");
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
        assert_eq!(i.next().unwrap().unwrap().value, "a/a/a/a");
        assert_eq!(i.next().unwrap().unwrap().value, "a/a/a/b");
        assert_eq!(i.next().unwrap().unwrap().value, "a/b/a/a");
        assert_eq!(i.next().unwrap().unwrap().value, "a/b/a/b");
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
        assert_eq!(i.next().unwrap().unwrap().value, "/a/a/a/a");
        assert_eq!(i.next().unwrap().unwrap().value, "/a/a/b/a");
        assert_eq!(i.next().unwrap().unwrap().value, "/b/a/a/a");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn multi_component_pattern_ending_with_slash() {
        let mut env = env_with_dummy_files(["a/a/_", "a/b/_", "a/c"]);
        let f = dummy_attr_field("a/*/");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "a/a/");
        assert_eq!(i.next().unwrap().unwrap().value, "a/b/");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn multi_component_pattern_with_adjacent_slashes() {
        let mut env = env_with_dummy_files(["a/b", "b/a"]);
        let f = dummy_attr_field("?//?");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "a//b");
        assert_eq!(i.next().unwrap().unwrap().value, "b//a");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn no_search_permission_needed_if_last_component_is_pattern() {
        let system = VirtualSystem::new();
        {
            let mut state = system.state.borrow_mut();
            state
                .file_system
                .save("foo/bar", Default::default())
                .unwrap();
            let dir = state.file_system.get("foo").unwrap();
            dir.borrow_mut().permissions = Mode::ALL_READ | Mode::ALL_WRITE;
        }
        let mut env = Env::with_system(Rc::new(Concurrent::new(system)));
        let f = dummy_attr_field("foo/*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "foo/bar");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn invalid_pattern_remains_intact() {
        let mut env = env_with_dummy_files(["foo.txt"]);
        let f = dummy_attr_field("*[[:wrong:]]*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "*[[:wrong:]]*");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn slash_between_brackets() {
        let mut env = env_with_dummy_files(["abd", "a/d"]);
        let f = dummy_attr_field("a[b/c]d");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "a[b/c]d");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn nul_byte_in_literal_followed_by_pattern() {
        let mut env = env_with_dummy_files(["x", "y/y"]);
        let f = dummy_attr_field("\0/*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "\0/*");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn broken_utf8_byte_in_directory_entry_name() {
        let mut env = env_with_dummy_files([UnixStr::from_bytes(b"foo/\xFF")]);
        let f = dummy_attr_field("foo/*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "foo/*");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn noglob_option() {
        let mut env = env_with_dummy_files(["foo.exe"]);
        env.options.set(yash_env::option::Option::Glob, Off);
        let f = dummy_attr_field("foo.*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "foo.*");
        assert_eq!(i.next(), None);
    }

    fn raise_sigint(system: &VirtualSystem) {
        let _ = system.current_process_mut().raise_signal(SIGINT);
    }

    #[test]
    fn sigint_interrupts_glob_in_interactive_shell() {
        use futures_util::FutureExt as _;
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system.clone())));
        env.options.set(
            yash_env::option::Option::Interactive,
            yash_env::option::State::On,
        );
        env.traps
            .enable_internal_dispositions_for_terminators(&env.system)
            .now_or_never()
            .unwrap()
            .unwrap();
        {
            let mut state = system.state.borrow_mut();
            state.file_system.save("foo", Rc::default()).unwrap();
        }
        raise_sigint(&system);

        let f = dummy_attr_field("*");
        let mut i = glob(&mut env, f);
        let Err(interrupted) = i.next().unwrap() else {
            panic!("expected Err(Interrupted)");
        };
        assert_eq!(interrupted.0, ExitStatus::from(SIGINT));
        assert_eq!(i.next(), None);
    }

    #[test]
    fn sigint_does_not_interrupt_glob_in_non_interactive_shell() {
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system.clone())));
        {
            let mut state = system.state.borrow_mut();
            state
                .file_system
                .save("testdir/foo", Rc::default())
                .unwrap();
        }
        raise_sigint(&system);

        let f = dummy_attr_field("testdir/*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "testdir/foo");
        assert_eq!(i.next(), None);
    }

    #[test]
    fn sigint_does_not_interrupt_glob_when_sigint_is_trapped() {
        use futures_util::FutureExt as _;
        let system = VirtualSystem::new();
        let mut env = Env::with_system(Rc::new(Concurrent::new(system.clone())));
        env.options.set(
            yash_env::option::Option::Interactive,
            yash_env::option::State::On,
        );
        env.traps
            .set_action(
                &env.system,
                SIGINT,
                Action::Command("echo trapped".into()),
                Location::dummy(""),
                false,
            )
            .now_or_never()
            .unwrap()
            .unwrap();
        {
            let mut state = system.state.borrow_mut();
            state
                .file_system
                .save("testdir/foo", Rc::default())
                .unwrap();
        }
        raise_sigint(&system);

        let f = dummy_attr_field("testdir/*");
        let mut i = glob(&mut env, f);
        assert_eq!(i.next().unwrap().unwrap().value, "testdir/foo");
        assert_eq!(i.next(), None);
    }
}
