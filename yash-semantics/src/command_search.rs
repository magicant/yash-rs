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
//! 1. Special built-ins
//! 1. Functions
//! 1. Intrinsic built-ins
//! 1. Non-intrinsic built-ins
//! 1. External utilities
//!
//! For a non-intrinsic built-in or external utility to be chosen as a target, a
//! corresponding executable file must be present in a directory specified in
//! the `$PATH` variable.

use std::collections::HashMap;
use std::ffi::CStr;
use std::ffi::CString;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::rc::Rc;
use yash_env::builtin::Builtin;
use yash_env::builtin::Type::{Intrinsic, NonIntrinsic, Special};
use yash_env::function::Function;
use yash_env::function::FunctionSet;
use yash_env::variable::Variable;
use yash_env::Env;

/// Target of a simple command execution.
///
/// This is the result of the [command search](search).
#[derive(Clone, Debug)]
pub enum Target {
    /// Built-in utility.
    Builtin(Builtin),
    /// Function.
    Function(Rc<Function>),
    /// External utility.
    External {
        /// Path to the external utility.
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

impl From<Builtin> for Target {
    fn from(builtin: Builtin) -> Target {
        Target::Builtin(builtin)
    }
}

impl From<Rc<Function>> for Target {
    fn from(function: Rc<Function>) -> Target {
        Target::Function(function)
    }
}

// impl From<CString> for Target
// not implemented because of ambiguity between a non-intrinsic built-in and
// external utility

/// Part of the shell execution environment command path search depends on.
pub trait PathEnv {
    /// Accesses the `$PATH` variable in the environment.
    fn path(&self) -> Option<&Variable>;
    /// Whether there is an executable file at the specified path.
    fn is_executable_file(&self, path: &CStr) -> bool;
    // TODO Cache the results of external utility search
}

/// Part of the shell execution environment command search depends on.
pub trait SearchEnv: PathEnv {
    /// Accesses the built-in set in the environment.
    fn builtins(&self) -> &HashMap<&'static str, Builtin>;
    /// Accesses the function set in the environment.
    fn functions(&self) -> &FunctionSet;
}

impl PathEnv for Env {
    fn path(&self) -> Option<&Variable> {
        self.variables.get("PATH")
    }
    fn is_executable_file(&self, path: &CStr) -> bool {
        self.system.is_executable_file(path)
    }
}

impl SearchEnv for Env {
    fn builtins(&self) -> &HashMap<&'static str, Builtin> {
        &self.builtins
    }
    fn functions(&self) -> &FunctionSet {
        &self.functions
    }
}

/// Performs command search.
pub fn search<E: SearchEnv>(env: &mut E, name: &str) -> Option<Target> {
    if name.contains('/') {
        return if let Ok(path) = CString::new(name) {
            Some(Target::External { path })
        } else {
            None
        };
    }

    let builtin = env.builtins().get(name).copied();
    if let Some(builtin) = builtin {
        if builtin.r#type == Special {
            return Some(builtin.into());
        }
    }

    if let Some(function) = env.functions().get(name) {
        return Some(function.0.clone().into());
    }

    if let Some(builtin) = builtin {
        if builtin.r#type == Intrinsic {
            return Some(builtin.into());
        }
    }

    if let Some(path) = search_path(env, name) {
        if let Some(builtin) = builtin {
            assert_eq!(builtin.r#type, NonIntrinsic);
            return Some(builtin.into());
        }
        return Some(Target::External { path });
    }

    None
}

/// Searches the `$PATH` for an executable file.
///
/// Returns the path if successful. Note that the returned path may not be
/// absolute if the `$PATH` contains a relative path.
pub fn search_path<E: PathEnv>(env: &mut E, name: &str) -> Option<CString> {
    if let Some(path) = env.path() {
        for dir in path.value.split() {
            let mut file = PathBuf::new();
            file.push(dir);
            file.push(name);
            if let Ok(file) = CString::new(file.into_os_string().into_vec()) {
                if env.is_executable_file(&file) {
                    return Some(file);
                }
            }
        }
    }

    None
}

#[allow(clippy::field_reassign_with_default)]
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use yash_env::function::HashEntry as FunctionEntry;
    use yash_env::variable::Value::{Array, Scalar};
    use yash_syntax::source::Location;
    use yash_syntax::syntax::CompoundCommand;
    use yash_syntax::syntax::FullCompoundCommand;

    #[derive(Default)]
    struct DummyEnv {
        builtins: HashMap<&'static str, Builtin>,
        functions: FunctionSet,
        path: Option<Variable>,
        executables: HashSet<String>,
    }

    impl PathEnv for DummyEnv {
        fn path(&self) -> Option<&Variable> {
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
        fn builtins(&self) -> &HashMap<&'static str, Builtin> {
            &self.builtins
        }
        fn functions(&self) -> &FunctionSet {
            &self.functions
        }
    }

    fn full_compound_command(s: &str) -> Rc<FullCompoundCommand> {
        Rc::new(FullCompoundCommand {
            command: CompoundCommand::Grouping(s.parse().unwrap()),
            redirs: vec![],
        })
    }

    #[test]
    fn nothing_is_found_in_empty_env() {
        let mut env = DummyEnv::default();
        let target = search(&mut env, "foo");
        assert!(target.is_none(), "{:?}", target);
    }

    #[test]
    fn nothing_is_found_with_name_unmatched() {
        let mut env = DummyEnv::default();
        env.builtins.insert(
            "foo",
            Builtin {
                r#type: Special,
                execute: |_, _| panic!(),
            },
        );
        env.functions.insert(FunctionEntry::new(
            "foo".to_string(),
            full_compound_command(""),
            Location::dummy("".to_string()),
            false,
        ));

        let target = search(&mut env, "bar");
        assert!(target.is_none(), "{:?}", target);
    }

    #[test]
    fn special_builtin_is_found() {
        let mut env = DummyEnv::default();
        let builtin = Builtin {
            r#type: Special,
            execute: |_, _| panic!(),
        };
        env.builtins.insert("foo", builtin);

        match search(&mut env, "foo") {
            Some(Target::Builtin(result)) => assert_eq!(result.r#type, builtin.r#type),
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn function_is_found_if_not_hidden_by_special_builtin() {
        let mut env = DummyEnv::default();
        let function = FunctionEntry::new(
            "foo".to_string(),
            full_compound_command("bar"),
            Location::dummy("location".to_string()),
            false,
        );
        env.functions.insert(function.clone());

        match search(&mut env, "foo") {
            Some(Target::Function(result)) => assert_eq!(result, function.0),
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn special_builtin_takes_priority_over_function() {
        let mut env = DummyEnv::default();
        let builtin = Builtin {
            r#type: Special,
            execute: |_, _| panic!(),
        };
        env.builtins.insert("foo", builtin);
        env.functions.insert(FunctionEntry::new(
            "foo".to_string(),
            full_compound_command("bar"),
            Location::dummy("location".to_string()),
            false,
        ));

        match search(&mut env, "foo") {
            Some(Target::Builtin(result)) => assert_eq!(result.r#type, builtin.r#type),
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn intrinsic_builtin_is_found_if_not_hidden_by_function() {
        let mut env = DummyEnv::default();
        let builtin = Builtin {
            r#type: Intrinsic,
            execute: |_, _| panic!(),
        };
        env.builtins.insert("foo", builtin);

        match search(&mut env, "foo") {
            Some(Target::Builtin(result)) => assert_eq!(result.r#type, builtin.r#type),
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn function_takes_priority_over_intrinsic_builtin() {
        let mut env = DummyEnv::default();
        env.builtins.insert(
            "foo",
            Builtin {
                r#type: Intrinsic,
                execute: |_, _| panic!(),
            },
        );

        let function = FunctionEntry::new(
            "foo".to_string(),
            full_compound_command("bar"),
            Location::dummy("location".to_string()),
            false,
        );
        env.functions.insert(function.clone());

        match search(&mut env, "foo") {
            Some(Target::Function(result)) => assert_eq!(result, function.0),
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn non_intrinsic_builtin_is_found_if_external_executable_exists() {
        let mut env = DummyEnv::default();
        let builtin = Builtin {
            r#type: NonIntrinsic,
            execute: |_, _| panic!(),
        };
        env.builtins.insert("foo", builtin);
        env.path = Some(Variable {
            value: Scalar("/bin".to_string()),
            last_assigned_location: None,
            is_exported: true,
            read_only_location: None,
        });
        env.executables.insert("/bin/foo".to_string());

        match search(&mut env, "foo") {
            Some(Target::Builtin(result)) => assert_eq!(result.r#type, builtin.r#type),
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn non_intrinsic_builtin_is_not_found_without_external_executable() {
        let mut env = DummyEnv::default();
        let builtin = Builtin {
            r#type: NonIntrinsic,
            execute: |_, _| panic!(),
        };
        env.builtins.insert("foo", builtin);

        let target = search(&mut env, "foo");
        assert!(target.is_none(), "{:?}", target);
    }

    #[test]
    fn function_takes_priority_over_non_intrinsic_builtin() {
        let mut env = DummyEnv::default();
        let builtin = Builtin {
            r#type: NonIntrinsic,
            execute: |_, _| panic!(),
        };
        env.builtins.insert("foo", builtin);
        env.path = Some(Variable {
            value: Scalar("/bin".to_string()),
            last_assigned_location: None,
            is_exported: true,
            read_only_location: None,
        });
        env.executables.insert("/bin/foo".to_string());

        let function = FunctionEntry::new(
            "foo".to_string(),
            full_compound_command("bar"),
            Location::dummy("location".to_string()),
            false,
        );
        env.functions.insert(function.clone());

        match search(&mut env, "foo") {
            Some(Target::Function(result)) => assert_eq!(result, function.0),
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn external_utility_is_found_if_external_executable_exists() {
        let mut env = DummyEnv::default();
        env.path = Some(Variable {
            value: Scalar("/bin".to_string()),
            last_assigned_location: None,
            is_exported: true,
            read_only_location: None,
        });
        env.executables.insert("/bin/foo".to_string());

        match search(&mut env, "foo") {
            Some(Target::External { path }) => assert_eq!(path.to_bytes(), "/bin/foo".as_bytes()),
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn returns_external_utility_if_name_contains_slash() {
        let mut env = DummyEnv::default();
        let builtin = Builtin {
            r#type: NonIntrinsic,
            execute: |_, _| panic!(),
        };
        env.builtins.insert("foo", builtin);
        env.functions.insert(FunctionEntry::new(
            "foo".to_string(),
            full_compound_command("bar"),
            Location::dummy("location".to_string()),
            false,
        ));

        match search(&mut env, "bar/baz") {
            Some(Target::External { path }) => assert_eq!(path.to_bytes(), "bar/baz".as_bytes()),
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn external_target_is_first_executable_found_in_path_scalar() {
        let mut env = DummyEnv::default();
        env.path = Some(Variable {
            value: Scalar("/usr/local/bin:/usr/bin:/bin".to_string()),
            last_assigned_location: None,
            is_exported: true,
            read_only_location: None,
        });
        env.executables.insert("/usr/bin/foo".to_string());
        env.executables.insert("/bin/foo".to_string());

        match search(&mut env, "foo") {
            Some(Target::External { path }) => {
                assert_eq!(path.to_bytes(), "/usr/bin/foo".as_bytes())
            }
            result => panic!("{:?}", result),
        }

        env.executables.insert("/usr/local/bin/foo".to_string());

        match search(&mut env, "foo") {
            Some(Target::External { path }) => {
                assert_eq!(path.to_bytes(), "/usr/local/bin/foo".as_bytes())
            }
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn external_target_is_first_executable_found_in_path_array() {
        let mut env = DummyEnv::default();
        env.path = Some(Variable {
            value: Array(vec![
                "/usr/local/bin".to_string(),
                "/usr/bin".to_string(),
                "/bin".to_string(),
            ]),
            last_assigned_location: None,
            is_exported: true,
            read_only_location: None,
        });
        env.executables.insert("/usr/bin/foo".to_string());
        env.executables.insert("/bin/foo".to_string());

        match search(&mut env, "foo") {
            Some(Target::External { path }) => {
                assert_eq!(path.to_bytes(), "/usr/bin/foo".as_bytes())
            }
            result => panic!("{:?}", result),
        }

        env.executables.insert("/usr/local/bin/foo".to_string());

        match search(&mut env, "foo") {
            Some(Target::External { path }) => {
                assert_eq!(path.to_bytes(), "/usr/local/bin/foo".as_bytes())
            }
            result => panic!("{:?}", result),
        }
    }

    #[test]
    fn empty_string_in_path_names_current_directory() {
        let mut env = DummyEnv::default();
        env.path = Some(Variable {
            value: Scalar("/x::/y".to_string()),
            last_assigned_location: None,
            is_exported: true,
            read_only_location: None,
        });
        env.executables.insert("foo".to_string());

        match search(&mut env, "foo") {
            Some(Target::External { path }) => assert_eq!(path.to_bytes(), "foo".as_bytes()),
            result => panic!("{:?}", result),
        }
    }
}
