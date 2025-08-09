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

//! Command search.
//!
//! The [command search](search) is part of the execution of a [simple
//! command](yash_syntax::syntax::SimpleCommand). It determines a command target
//! that is to be invoked. A [target](Target) can be a built-in utility,
//! function, or external utility.
//!
//! If the command name contains a slash, the target is always an external
//! utility. Otherwise, the shell searches the following candidates for the
//! target (in the order of priority):
//!
//! 1. [Special] built-ins
//! 1. Functions
//! 1. Other built-ins
//! 1. External utilities
//!
//! For a [substitutive](Substitutive) built-in or external utility to be chosen
//! as a target, a corresponding executable file must be present in a directory
//! specified in the `$PATH` variable.

use assert_matches::assert_matches;
use std::ffi::CStr;
use std::ffi::CString;
use std::rc::Rc;
use yash_env::Env;
use yash_env::System;
use yash_env::builtin::Builtin;
use yash_env::builtin::Type::{Elective, Extension, Mandatory, Special, Substitutive};
use yash_env::function::Function;
use yash_env::path::PathBuf;
use yash_env::variable::Expansion;
use yash_env::variable::PATH;

/// Target of a simple command execution
///
/// This is the result of the [command search](search).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Target {
    /// Built-in utility
    Builtin {
        /// Definition of the built-in
        builtin: Builtin,
        /// Path to the external utility that is shadowed by the substitutive
        /// built-in
        ///
        /// The path may not necessarily be absolute. If the `$PATH` variable
        /// contains a relative directory name and the external utility is found
        /// in that directory, the path will be relative.
        ///
        /// The path will be `None` if the built-in is not substitutive.
        path: Option<CString>,
    },

    /// Function
    Function(Rc<Function>),

    /// External utility
    External {
        /// Path to the external utility
        ///
        /// The path may not necessarily be absolute. If the `$PATH` variable
        /// contains a relative directory name and the external utility is found
        /// in that directory, the path will be relative.
        ///
        /// The path may not name an existing executable file, either. If the
        /// command name contains a slash, the name is immediately regarded as a
        /// path to an external utility, regardless of whether the named
        /// external utility actually exists.
        path: CString,
    },
}

impl From<Rc<Function>> for Target {
    #[inline]
    fn from(function: Rc<Function>) -> Target {
        Target::Function(function)
    }
}

impl From<Function> for Target {
    #[inline]
    fn from(function: Function) -> Target {
        Target::Function(function.into())
    }
}

// impl From<CString> for Target
// not implemented because of ambiguity between substitutive built-ins and
// external utilities

/// Part of the shell execution environment command path search depends on.
pub trait PathEnv {
    /// Accesses the `$PATH` variable in the environment.
    ///
    /// This function returns an `Expansion` rather than a reference to a
    /// variable value because the path may be dynamically computed in the
    /// function.
    #[must_use]
    fn path(&self) -> Expansion<'_>;

    /// Whether there is an executable file at the specified path.
    #[must_use]
    fn is_executable_file(&self, path: &CStr) -> bool;
    // TODO Cache the results of external utility search
}

/// Part of the shell execution environment command search depends on.
pub trait SearchEnv: PathEnv {
    /// Retrieves the built-in by name.
    #[must_use]
    fn builtin(&self, name: &str) -> Option<Builtin>;

    /// Retrieves the function by name.
    #[must_use]
    fn function(&self, name: &str) -> Option<&Rc<Function>>;
}

impl PathEnv for Env {
    /// Returns the value of the `$PATH` variable.
    ///
    /// This function assumes that the `$PATH` variable has no quirks. If the
    /// variable has a quirk, the function panics.
    fn path(&self) -> Expansion<'_> {
        self.variables
            .get(PATH)
            .and_then(|var| {
                assert_eq!(var.quirk, None, "PATH does not support quirks");
                var.value.as_ref()
            })
            .into()
    }

    fn is_executable_file(&self, path: &CStr) -> bool {
        self.system.is_executable_file(path)
    }
}

impl SearchEnv for Env {
    fn builtin(&self, name: &str) -> Option<Builtin> {
        self.builtins.get(name).copied()
    }

    #[inline]
    fn function(&self, name: &str) -> Option<&Rc<Function>> {
        self.functions.get(name)
    }
}

/// Performs command search.
///
/// This function requires a mutable reference to the environment because it may
/// need to update a cache of the results of external utility search (TODO:
/// which is not yet implemented). The function does not otherwise modify the
/// environment.
///
/// If the given name contains a slash, the function immediately returns an
/// external utility target, regardless of whether the named external utility
/// actually exists.
pub fn search<E: SearchEnv>(env: &mut E, name: &str) -> Option<Target> {
    if name.contains('/') {
        return if let Ok(path) = CString::new(name) {
            Some(Target::External { path })
        } else {
            None
        };
    }

    let builtin = env.builtin(name);
    if let Some(builtin) = builtin {
        if builtin.r#type == Special {
            let path = None;
            return Some(Target::Builtin { builtin, path });
        }
    }

    if let Some(function) = env.function(name) {
        return Some(Rc::clone(function).into());
    }

    if let Some(builtin) = builtin {
        if builtin.r#type != Substitutive {
            assert_matches!(builtin.r#type, Mandatory | Elective | Extension);
            let path = None;
            return Some(Target::Builtin { builtin, path });
        }
    }

    if let Some(path) = search_path(env, name) {
        if let Some(builtin) = builtin {
            assert_eq!(builtin.r#type, Substitutive);
            let path = Some(path);
            return Some(Target::Builtin { builtin, path });
        }
        return Some(Target::External { path });
    }

    None
}

/// Searches the `$PATH` for an executable file.
///
/// Returns the path to the executable if found. Note that the returned path may
/// not be absolute if the `$PATH` contains a relative path.
pub fn search_path<E: PathEnv>(env: &mut E, name: &str) -> Option<CString> {
    env.path()
        .split()
        .filter_map(|dir| {
            let candidate = PathBuf::from_iter([dir, name])
                .into_unix_string()
                .into_vec();
            CString::new(candidate).ok()
        })
        .find(|path| env.is_executable_file(path))
}

#[allow(clippy::field_reassign_with_default)]
#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::collections::HashMap;
    use std::collections::HashSet;
    use yash_env::function::FunctionSet;
    use yash_env::variable::Value;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::CompoundCommand;
    use yash_syntax::syntax::FullCompoundCommand;

    #[derive(Default)]
    struct DummyEnv {
        builtins: HashMap<&'static str, Builtin>,
        functions: FunctionSet,
        path: Expansion<'static>,
        executables: HashSet<String>,
    }

    impl PathEnv for DummyEnv {
        fn path(&self) -> Expansion<'_> {
            self.path.as_ref()
        }
        fn is_executable_file(&self, path: &CStr) -> bool {
            if let Ok(path) = path.to_str() {
                self.executables.contains(path)
            } else {
                false
            }
        }
    }

    impl SearchEnv for DummyEnv {
        fn builtin(&self, name: &str) -> Option<Builtin> {
            self.builtins.get(name).copied()
        }
        fn function(&self, name: &str) -> Option<&Rc<Function>> {
            self.functions.get(name)
        }
    }

    fn full_compound_command(s: &str) -> FullCompoundCommand {
        FullCompoundCommand {
            command: CompoundCommand::Grouping(s.parse().unwrap()),
            redirs: vec![],
        }
    }

    #[test]
    fn nothing_is_found_in_empty_env() {
        let mut env = DummyEnv::default();
        let target = search(&mut env, "foo");
        assert!(target.is_none(), "target = {target:?}");
    }

    #[test]
    fn nothing_is_found_with_name_unmatched() {
        let mut env = DummyEnv::default();
        env.builtins
            .insert("foo", Builtin::new(Special, |_, _| unreachable!()));
        let function = Function::new("foo", full_compound_command(""), Location::dummy(""));
        env.functions.define(function).unwrap();

        let target = search(&mut env, "bar");
        assert!(target.is_none(), "target = {target:?}");
    }

    #[test]
    fn special_builtin_is_found() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Special, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);

        assert_matches!(
            search(&mut env, "foo"),
            Some(Target::Builtin { builtin: result, path: None }) => {
                assert_eq!(result.r#type, builtin.r#type);
            }
        );
    }

    #[test]
    fn function_is_found_if_not_hidden_by_special_builtin() {
        let mut env = DummyEnv::default();
        let function = Rc::new(Function::new(
            "foo",
            full_compound_command("bar"),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Some(Target::Function(result)) => {
            assert_eq!(result, function);
        });
    }

    #[test]
    fn special_builtin_takes_priority_over_function() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Special, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        let function = Function::new(
            "foo",
            full_compound_command("bar"),
            Location::dummy("location"),
        );
        env.functions.define(function).unwrap();

        assert_matches!(
            search(&mut env, "foo"),
            Some(Target::Builtin { builtin: result, path: None }) => {
                assert_eq!(result.r#type, builtin.r#type);
            }
        );
    }

    #[test]
    fn mandatory_builtin_is_found_if_not_hidden_by_function() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Mandatory, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);

        assert_matches!(
            search(&mut env, "foo"),
            Some(Target::Builtin { builtin: result, path: None }) => {
                assert_eq!(result.r#type, builtin.r#type);
            }
        );
    }

    #[test]
    fn elective_builtin_is_found_if_not_hidden_by_function() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Elective, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);

        assert_matches!(
            search(&mut env, "foo"),
            Some(Target::Builtin { builtin: result, path: None }) => {
                assert_eq!(result.r#type, builtin.r#type);
            }
        );
    }

    #[test]
    fn extension_builtin_is_found_if_not_hidden_by_function_or_option() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Extension, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);

        assert_matches!(
            search(&mut env, "foo"),
            Some(Target::Builtin { builtin: result, path: None }) => {
                assert_eq!(result.r#type, builtin.r#type);
            }
        );
    }

    #[test]
    fn function_takes_priority_over_mandatory_builtin() {
        let mut env = DummyEnv::default();
        env.builtins
            .insert("foo", Builtin::new(Mandatory, |_, _| unreachable!()));

        let function = Rc::new(Function::new(
            "foo",
            full_compound_command("bar"),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Some(Target::Function(result)) => {
            assert_eq!(result, function);
        });
    }

    #[test]
    fn function_takes_priority_over_elective_builtin() {
        let mut env = DummyEnv::default();
        env.builtins
            .insert("foo", Builtin::new(Elective, |_, _| unreachable!()));

        let function = Rc::new(Function::new(
            "foo",
            full_compound_command("bar"),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Some(Target::Function(result)) => {
            assert_eq!(result, function);
        });
    }

    #[test]
    fn function_takes_priority_over_extension_builtin() {
        let mut env = DummyEnv::default();
        env.builtins
            .insert("foo", Builtin::new(Extension, |_, _| unreachable!()));

        let function = Rc::new(Function::new(
            "foo",
            full_compound_command("bar"),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Some(Target::Function(result)) => {
            assert_eq!(result, function);
        });
    }

    #[test]
    fn substitutive_builtin_is_found_if_external_executable_exists() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Substitutive, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        env.path = Expansion::from("/bin");
        env.executables.insert("/bin/foo".to_string());

        assert_matches!(
            search(&mut env, "foo"),
            Some(Target::Builtin { builtin: result, path: Some(path) }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(path.to_bytes(), b"/bin/foo");
            }
        );
    }

    #[test]
    fn substitutive_builtin_is_not_found_without_external_executable() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Substitutive, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);

        let target = search(&mut env, "foo");
        assert!(target.is_none(), "target = {target:?}");
    }

    #[test]
    fn function_takes_priority_over_substitutive_builtin() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Substitutive, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        env.path = Expansion::from("/bin");
        env.executables.insert("/bin/foo".to_string());

        let function = Rc::new(Function::new(
            "foo",
            full_compound_command("bar"),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Some(Target::Function(result)) => {
            assert_eq!(result, function);
        });
    }

    #[test]
    fn external_utility_is_found_if_external_executable_exists() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from("/bin");
        env.executables.insert("/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(path.to_bytes(), "/bin/foo".as_bytes());
        });
    }

    #[test]
    fn returns_external_utility_if_name_contains_slash() {
        // In this case, the external utility file does not have to exist.
        let mut env = DummyEnv::default();
        assert_matches!(search(&mut env, "bar/baz"), Some(Target::External { path }) => {
            assert_eq!(path.to_bytes(), "bar/baz".as_bytes());
        });
    }

    #[test]
    fn external_target_is_first_executable_found_in_path_scalar() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from("/usr/local/bin:/usr/bin:/bin");
        env.executables.insert("/usr/bin/foo".to_string());
        env.executables.insert("/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(path.to_bytes(), "/usr/bin/foo".as_bytes());
        });

        env.executables.insert("/usr/local/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(path.to_bytes(), "/usr/local/bin/foo".as_bytes());
        });
    }

    #[test]
    fn external_target_is_first_executable_found_in_path_array() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from(Value::array(["/usr/local/bin", "/usr/bin", "/bin"]));
        env.executables.insert("/usr/bin/foo".to_string());
        env.executables.insert("/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(path.to_bytes(), "/usr/bin/foo".as_bytes());
        });

        env.executables.insert("/usr/local/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(path.to_bytes(), "/usr/local/bin/foo".as_bytes());
        });
    }

    #[test]
    fn empty_string_in_path_names_current_directory() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from("/x::/y");
        env.executables.insert("foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(path.to_bytes(), "foo".as_bytes());
        });
    }
}
