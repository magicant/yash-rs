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

//! Command search
//!
//! The [command search], implemented by [`search`], is part of the execution of
//! a [simple command]. It determines a command target that is to be invoked. A
//! [target](Target) can be a built-in utility, function, or external utility.
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
//! specified in the `PATH` variable.
//!
//! [command search]: https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html#tag_19_09_01_04
//! [simple command]: https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html#tag_19_09_01

use crate::Env;
use crate::System;
use crate::builtin::Builtin;
use crate::builtin::Type::{Special, Substitutive};
use crate::function::Function;
use crate::path::PathBuf;
use crate::variable::Expansion;
use crate::variable::PATH;
use std::ffi::CStr;
use std::ffi::CString;
use std::rc::Rc;

/// Target of a simple command execution
///
/// This is the result of the [command search](search).
///
/// # Notes on equality
///
/// Although this type implements `PartialEq`, comparison between instances of
/// this type may not always yield predictable results due to the presence of
/// function pointers in [`Builtin`]. As a result, it is recommended to avoid
/// relying on equality comparisons for values of this type. See
/// <https://doc.rust-lang.org/std/ptr/fn.fn_addr_eq.html> for the
/// characteristics of function pointer comparisons.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Target {
    /// Built-in utility
    Builtin {
        /// Definition of the built-in
        builtin: Builtin,

        /// Path to the external utility that is shadowed by the substitutive
        /// built-in
        ///
        /// This value is only used for substitutive built-ins. For other types
        /// of built-ins, this value is always empty.
        ///
        /// The path may not necessarily be absolute. If the `PATH` variable
        /// contains a relative directory name and the external utility is found
        /// in that directory, the path will be relative.
        path: CString,
    },

    /// Function
    Function(Rc<Function>),

    /// External utility
    External {
        /// Path to the external utility
        ///
        /// The path may not necessarily be absolute. If the `PATH` variable
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

/// Collection of data used in [classifying](classify) command names
pub trait ClassifyEnv {
    /// Retrieves the built-in by name.
    #[must_use]
    fn builtin(&self, name: &str) -> Option<Builtin>;

    /// Retrieves the function by name.
    #[must_use]
    fn function(&self, name: &str) -> Option<&Rc<Function>>;
}

/// Part of the shell execution environment command path search depends on
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

impl ClassifyEnv for Env {
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
/// This function effectively combines the [`classify`] and [`search_path`]
/// functions into a single operation performing full command search.
///
/// See [`search_path`] for why this function requires a mutable reference to
/// the environment.
///
/// See the [module documentation](self) for details of the command search
/// process.
#[must_use]
pub fn search<E: ClassifyEnv + PathEnv>(env: &mut E, name: &str) -> Option<Target> {
    let mut target = classify(env, name);

    'fill_path: {
        let path = match &mut target {
            Target::Builtin { builtin, path } if builtin.r#type == Substitutive => {
                // Must verify the external counterpart exists.
                path
            }

            Target::External { path } => {
                if name.contains('/') {
                    // Just access the given path.
                    *path = CString::new(name).ok()?;
                    break 'fill_path;
                } else {
                    // Need to actually find it in PATH.
                    path
                }
            }

            Target::Builtin { .. } | Target::Function(_) => {
                // Nothing to do.
                break 'fill_path;
            }
        };

        if let Some(real_path) = search_path(env, name) {
            *path = real_path;
        } else {
            return None;
        }
    }

    Some(target)
}

/// Determines the type of command target without performing a full search.
///
/// This function is a simplified version of [`search`] that only classifies the
/// command name into one of the target types. It does not return the actual
/// target path, so it is more efficient than `search` if the caller only needs
/// to know the type of target. However, since the function does not search for
/// external utilities, it cannot determine whether a substitutive built-in or
/// an external utility is the actual target. This function always assumes that
/// searching for an external utility would succeed and returns a target with
/// an empty path in such cases.
#[must_use]
pub fn classify<E: ClassifyEnv>(env: &E, name: &str) -> Target {
    if name.contains('/') {
        return Target::External {
            path: CString::default(),
        };
    }

    let builtin = env.builtin(name);
    if let Some(builtin) = builtin {
        if builtin.r#type == Special {
            let path = CString::default();
            return Target::Builtin { builtin, path };
        }
    }

    if let Some(function) = env.function(name) {
        return Rc::clone(function).into();
    }

    if let Some(builtin) = builtin {
        let path = CString::default();
        return Target::Builtin { builtin, path };
    }

    Target::External {
        path: CString::default(),
    }
}

/// Searches the `$PATH` for an executable file.
///
/// Returns the path to the executable if found. Note that the returned path may
/// not be absolute if the `$PATH` contains a relative path.
///
/// This function requires a mutable reference to the environment because it may
/// need to update a cache of the results of external utility search (TODO:
/// which is not yet implemented). The function does not otherwise modify the
/// environment.
#[must_use]
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
    use crate::builtin::Type::{Elective, Extension, Mandatory};
    use crate::function::{FunctionBody, FunctionBodyObject, FunctionSet};
    use crate::source::Location;
    use crate::variable::Value;
    use assert_matches::assert_matches;
    use std::collections::HashMap;
    use std::collections::HashSet;

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

    impl ClassifyEnv for DummyEnv {
        fn builtin(&self, name: &str) -> Option<Builtin> {
            self.builtins.get(name).copied()
        }
        fn function(&self, name: &str) -> Option<&Rc<Function>> {
            self.functions.get(name)
        }
    }

    #[derive(Clone, Debug)]
    struct FunctionBodyStub;

    impl std::fmt::Display for FunctionBodyStub {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            unreachable!()
        }
    }
    impl FunctionBody for FunctionBodyStub {
        async fn execute(&self, _: &mut Env) -> crate::semantics::Result {
            unreachable!()
        }
    }

    fn function_body_stub() -> Rc<dyn FunctionBodyObject> {
        Rc::new(FunctionBodyStub)
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
        let function = Function::new("foo", function_body_stub(), Location::dummy(""));
        env.functions.define(function).unwrap();

        let target = search(&mut env, "bar");
        assert!(target.is_none(), "target = {target:?}");
    }

    #[test]
    fn classify_defaults_to_external() {
        // In an empty environment, any name is not a built-in or function, so it
        // is classified as an external utility.
        let env = DummyEnv::default();
        let target = classify(&env, "foo");
        assert_eq!(
            target,
            Target::External {
                path: CString::default()
            }
        );
    }

    #[test]
    fn special_builtin_is_found() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Special, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);

        assert_matches!(
            search(&mut env, "foo"),
            Some(Target::Builtin { builtin: result, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
            }
        );
    }

    #[test]
    fn function_is_found_if_not_hidden_by_special_builtin() {
        let mut env = DummyEnv::default();
        let function = Rc::new(Function::new(
            "foo",
            function_body_stub(),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Some(Target::Function(result)) => {
            assert_eq!(result, function);
        });
        assert_matches!(classify(&env, "foo"), Target::Function(result) => {
            assert_eq!(result, function);
        });
    }

    #[test]
    fn special_builtin_takes_priority_over_function() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Special, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        let function = Function::new("foo", function_body_stub(), Location::dummy("location"));
        env.functions.define(function).unwrap();

        assert_matches!(
            search(&mut env, "foo"),
            Some(Target::Builtin { builtin: result, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
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
            Some(Target::Builtin { builtin: result, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
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
            Some(Target::Builtin { builtin: result, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
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
            Some(Target::Builtin { builtin: result, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
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
            function_body_stub(),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Some(Target::Function(result)) => {
            assert_eq!(result, function);
        });
        assert_matches!(classify(&env, "foo"), Target::Function(result) => {
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
            function_body_stub(),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Some(Target::Function(result)) => {
            assert_eq!(result, function);
        });
        assert_matches!(classify(&env, "foo"), Target::Function(result) => {
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
            function_body_stub(),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Some(Target::Function(result)) => {
            assert_eq!(result, function);
        });
        assert_matches!(classify(&env, "foo"), Target::Function(result) => {
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
            Some(Target::Builtin { builtin: result, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"/bin/foo");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
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
    fn substitutive_builtin_is_classified_even_without_external_executable() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Substitutive, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);

        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(*path, *c"");
            }
        );
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
            function_body_stub(),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Some(Target::Function(result)) => {
            assert_eq!(result, function);
        });
        assert_matches!(classify(&env, "foo"), Target::Function(result) => {
            assert_eq!(result, function);
        });
    }

    #[test]
    fn external_utility_is_found_if_external_executable_exists() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from("/bin");
        env.executables.insert("/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(*path, *c"/bin/foo");
        });
        assert_matches!(classify(&env, "foo"), Target::External { path } => {
            assert_eq!(*path, *c"");
        });
    }

    #[test]
    fn returns_external_utility_if_name_contains_slash() {
        // In this case, the external utility file does not have to exist.
        let mut env = DummyEnv::default();
        // The special built-in should be ignored because the command name
        // contains a slash.
        let builtin = Builtin::new(Special, |_, _| unreachable!());
        env.builtins.insert("bar/baz", builtin);

        assert_matches!(search(&mut env, "bar/baz"), Some(Target::External { path }) => {
            assert_eq!(*path, *c"bar/baz");
        });
        assert_matches!(classify(&env, "bar/baz"), Target::External { path } => {
            assert_eq!(*path, *c"");
        });
    }

    #[test]
    fn external_target_is_first_executable_found_in_path_scalar() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from("/usr/local/bin:/usr/bin:/bin");
        env.executables.insert("/usr/bin/foo".to_string());
        env.executables.insert("/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(*path, *c"/usr/bin/foo");
        });

        env.executables.insert("/usr/local/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(*path, *c"/usr/local/bin/foo");
        });
    }

    #[test]
    fn external_target_is_first_executable_found_in_path_array() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from(Value::array(["/usr/local/bin", "/usr/bin", "/bin"]));
        env.executables.insert("/usr/bin/foo".to_string());
        env.executables.insert("/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(*path, *c"/usr/bin/foo");
        });

        env.executables.insert("/usr/local/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(*path, *c"/usr/local/bin/foo");
        });
    }

    #[test]
    fn empty_string_in_path_names_current_directory() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from("/x::/y");
        env.executables.insert("foo".to_string());

        assert_matches!(search(&mut env, "foo"), Some(Target::External { path }) => {
            assert_eq!(*path, *c"foo");
        });
    }
}
