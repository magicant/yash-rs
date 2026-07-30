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
//! [Extension] built-ins are ignored (treated
//! as non-existing) when the [`PosixlyCorrect`] option is on, so the search
//! falls through to external utilities in that case.
//!
//! Note the difference between a built-in being *ignored* and *rejected*. An
//! ignored built-in is treated as if it did not exist, so the search falls
//! through to an external utility. A rejected built-in is still found, but it
//! cannot be executed; the search fails with [`Unusable`] rather than falling
//! through. See [`Availability`].
//!
//! [command search]: https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html#tag_19_09_01_04
//! [simple command]: https://pubs.opengroup.org/onlinepubs/9799919799/utilities/V3_chap02.html#tag_19_09_01

use crate::Env;
use crate::builtin::Builtin;
use crate::builtin::Type;
use crate::builtin::Type::{Elective, Extension, Mandatory, Special, Substitutive};
use crate::builtin::is_posix_special_builtin_name;
use crate::function::Function;
use crate::option::{On, Portable, PosixlyCorrect};
use crate::path::PathBuf;
use crate::semantics::ExitStatus;
use crate::system::IsExecutableFile;
use crate::variable::Expansion;
use crate::variable::PATH;
use std::ffi::CStr;
use std::ffi::CString;
use std::rc::Rc;

/// Whether a built-in found in the command search may be executed
///
/// The command search may find a built-in that the current shell options do not
/// allow to execute. Such a built-in is not ignored: the search does not fall
/// through to an external utility. Instead, it is reported as unavailable so
/// that the caller can tell the user why the built-in cannot be used.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Availability {
    /// The built-in may be executed.
    Available,

    /// The built-in, or the name under which it was found, is not defined in
    /// POSIX, and the [`Portable`] option rejects it.
    NotPortable,
}

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
pub enum Target<S> {
    /// Built-in utility
    Builtin {
        /// Definition of the built-in
        builtin: Builtin<S>,

        /// Whether the built-in may be executed
        ///
        /// [`search`] fails with [`Unusable::NotPortable`] instead of returning
        /// an unavailable built-in, so this value is always
        /// [`Available`](Availability::Available) in a target obtained from
        /// `search`. A target obtained from [`classify`] may have any value
        /// because `classify` does not reject anything by itself.
        availability: Availability,

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
    Function(Rc<Function<S>>),

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

// Not derived automatically because S may not implement Clone, PartialEq, or Debug.
impl<S> Clone for Target<S> {
    fn clone(&self) -> Self {
        match self {
            Self::Builtin {
                builtin,
                availability,
                path,
            } => Self::Builtin {
                builtin: *builtin,
                availability: *availability,
                path: path.clone(),
            },
            Self::Function(f) => Self::Function(f.clone()),
            Self::External { path } => Self::External { path: path.clone() },
        }
    }
}

impl<S> PartialEq for Target<S> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Builtin {
                    builtin: l_builtin,
                    availability: l_availability,
                    path: l_path,
                },
                Self::Builtin {
                    builtin: r_builtin,
                    availability: r_availability,
                    path: r_path,
                },
            ) => l_builtin == r_builtin && l_availability == r_availability && l_path == r_path,
            (Self::Function(l), Self::Function(r)) => l == r,
            (Self::External { path: l_path }, Self::External { path: r_path }) => l_path == r_path,
            _ => false,
        }
    }
}

impl<S> Eq for Target<S> {}

impl<S> std::fmt::Debug for Target<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Builtin {
                builtin,
                availability,
                path,
            } => f
                .debug_struct("Builtin")
                .field("builtin", builtin)
                .field("availability", availability)
                .field("path", path)
                .finish(),
            Self::Function(func) => f.debug_tuple("Function").field(func).finish(),
            Self::External { path } => f.debug_struct("External").field("path", path).finish(),
        }
    }
}

impl<S> From<Rc<Function<S>>> for Target<S> {
    #[inline]
    fn from(function: Rc<Function<S>>) -> Target<S> {
        Target::Function(function)
    }
}

impl<S> From<Function<S>> for Target<S> {
    #[inline]
    fn from(function: Function<S>) -> Target<S> {
        Target::Function(function.into())
    }
}

// impl From<CString> for Target
// not implemented because of ambiguity between substitutive built-ins and
// external utilities

/// Collection of data used in [classifying](classify) command names
pub trait ClassifyEnv<S> {
    /// Retrieves the built-in by name.
    ///
    /// This function returns `None` if there is no built-in with the given name
    /// or if the current shell options make the shell ignore it. An ignored
    /// built-in is treated as if it did not exist, so the command search falls
    /// through to an external utility.
    ///
    /// If a built-in is found, this function also returns its
    /// [`Availability`], which tells whether the shell options allow executing
    /// it. Unlike an ignored built-in, an unavailable built-in still hides an
    /// external utility of the same name.
    #[must_use]
    fn builtin(&self, name: &str) -> Option<(Builtin<S>, Availability)>;

    /// Retrieves the function by name.
    #[must_use]
    fn function(&self, name: &str) -> Option<&Rc<Function<S>>>;
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

impl<S: IsExecutableFile> PathEnv for Env<S> {
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

impl<S> ClassifyEnv<S> for Env<S> {
    fn builtin(&self, name: &str) -> Option<(Builtin<S>, Availability)> {
        let builtin = self.builtins.get(name).copied()?;

        // An extension built-in is ignored altogether when the shell must
        // behave POSIXly, so the search falls through to an external utility.
        let found = builtin.r#type != Extension || self.options.get(PosixlyCorrect) != On;
        if !found {
            return None;
        }

        // A built-in that POSIX does not define is found but rejected when the
        // shell must reject non-portable behavior. Unlike an ignored built-in,
        // it still hides an external utility of the same name. This also
        // applies to a special built-in found under a non-POSIX alias name
        // (e.g. `source`, an alias for `.`).
        let availability = match builtin.r#type {
            Elective | Extension if self.options.get(Portable) == On => Availability::NotPortable,
            Special if self.options.get(Portable) == On && !is_posix_special_builtin_name(name) => {
                Availability::NotPortable
            }
            Special | Mandatory | Elective | Extension | Substitutive => Availability::Available,
        };

        Some((builtin, availability))
    }

    #[inline]
    fn function(&self, name: &str) -> Option<&Rc<Function<S>>> {
        self.functions.get(name)
    }
}

/// Reason why a built-in found in the command search cannot be executed
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Unusable {
    /// The built-in is [substitutive](Substitutive), but no corresponding
    /// external utility was found in `$PATH`.
    NotInPath,

    /// The built-in, or the name under which it was found, is not defined in
    /// POSIX, and the [`Portable`] option rejects it.
    NotPortable,
}

impl Unusable {
    /// Returns the exit status the shell should produce for this reason.
    ///
    /// This is 126 if the built-in was rejected, and 127 if it effectively does
    /// not exist as a runnable command.
    #[must_use]
    pub const fn exit_status(self) -> ExitStatus {
        match self {
            Self::NotInPath => ExitStatus::NOT_FOUND,
            Self::NotPortable => ExitStatus::NOEXEC,
        }
    }
}

/// Describes the reason without naming the built-in.
///
/// The result is a sentence fragment meant to be embedded in an error message
/// that identifies the built-in, so it does not mention the name itself. It
/// states what the built-in is and why that makes it unusable, so the message
/// stands on its own without a title telling the two reasons apart.
impl std::fmt::Display for Unusable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInPath => "a substitutive built-in, so it cannot be used unless $PATH has \
                                an external utility of the same name"
                .fmt(f),
            Self::NotPortable => {
                "not a POSIX built-in, so it cannot be used while the `portable` option is on"
                    .fmt(f)
            }
        }
    }
}

/// Reason why the [command search](search) did not yield a target
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// No command was found with the given name.
    NotFound,

    /// A built-in was found, but it cannot be executed.
    ///
    /// Note that the shell must not fall back to an external utility in this
    /// case: the built-in was found, so the command search is over.
    Unusable(Unusable),
}

impl Error {
    /// Returns the exit status the shell should produce for this error.
    ///
    /// This is 127 if no command was found, and the
    /// [`Unusable::exit_status`] if a built-in was found but cannot be
    /// executed.
    #[must_use]
    pub const fn exit_status(self) -> ExitStatus {
        match self {
            Self::NotFound => ExitStatus::NOT_FOUND,
            Self::Unusable(cause) => cause.exit_status(),
        }
    }
}

/// Describes the reason without naming the command.
///
/// The result is a sentence fragment meant to be embedded in an error message
/// that identifies the command, so it does not mention the name itself.
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => "no command with this name".fmt(f),
            Self::Unusable(cause) => cause.fmt(f),
        }
    }
}

impl From<Unusable> for Error {
    #[inline]
    fn from(unusable: Unusable) -> Self {
        Self::Unusable(unusable)
    }
}

/// Completes the command search for a built-in.
///
/// This function decides whether a built-in found by [`classify`] can actually
/// be executed. On success, it returns the value to be stored in the `path`
/// field of [`Target::Builtin`], that is, the path to the external utility
/// shadowed by a [substitutive](Substitutive) built-in, or an empty string for
/// other types of built-ins.
///
/// The `r#type` and `availability` arguments should be the properties of the
/// built-in that was found for `name`.
///
/// [`search`] applies this function to a built-in target, but the caller may
/// need to apply it separately if it obtained a target from [`classify`]. That
/// is typically the case when the caller wants to postpone the check until
/// immediately before executing the built-in.
///
/// See [`search_path`] for why this function requires a mutable reference to
/// the environment.
pub fn resolve_builtin<E: PathEnv>(
    env: &mut E,
    name: &str,
    r#type: Type,
    availability: Availability,
) -> Result<CString, Unusable> {
    match availability {
        Availability::NotPortable => Err(Unusable::NotPortable),

        // A substitutive built-in is executed only if its external counterpart
        // is present in `$PATH`.
        Availability::Available if r#type == Substitutive => {
            search_path(env, name).ok_or(Unusable::NotInPath)
        }

        Availability::Available => Ok(CString::default()),
    }
}

/// Performs command search.
///
/// This function effectively combines the [`classify`], [`resolve_builtin`],
/// and [`search_path`] functions into a single operation performing full
/// command search.
///
/// See [`search_path`] for why this function requires a mutable reference to
/// the environment.
///
/// See the [module documentation](self) for details of the command search
/// process.
pub fn search<S, E: ClassifyEnv<S> + PathEnv>(env: &mut E, name: &str) -> Result<Target<S>, Error> {
    let mut target = classify(env, name);

    match &mut target {
        Target::Builtin {
            builtin,
            availability,
            path,
        } => *path = resolve_builtin(env, name, builtin.r#type, *availability)?,

        Target::External { path } => {
            *path = if name.contains('/') {
                // Just access the given path.
                CString::new(name).map_err(|_| Error::NotFound)?
            } else {
                // Need to actually find it in PATH.
                search_path(env, name).ok_or(Error::NotFound)?
            };
        }

        Target::Function(_) => {
            // Nothing to do.
        }
    }

    Ok(target)
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
///
/// This function does not reject a built-in that the shell options disallow,
/// either. It reports the [`Availability`] as part of the target so that the
/// caller can reject it later with [`resolve_builtin`].
#[must_use]
pub fn classify<S, E: ClassifyEnv<S>>(env: &E, name: &str) -> Target<S> {
    if name.contains('/') {
        return Target::External {
            path: CString::default(),
        };
    }

    let builtin = env.builtin(name);
    if let Some((builtin, availability)) = builtin
        && builtin.r#type == Special
    {
        let path = CString::default();
        return Target::Builtin {
            builtin,
            availability,
            path,
        };
    }

    if let Some(function) = env.function(name) {
        return Rc::clone(function).into();
    }

    if let Some((builtin, availability)) = builtin {
        let path = CString::default();
        return Target::Builtin {
            builtin,
            availability,
            path,
        };
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

#[allow(clippy::field_reassign_with_default, reason = "for readability")]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::function::{FunctionBody, FunctionBodyObject, FunctionSet};
    use crate::option::Off;
    use crate::source::Location;
    use crate::variable::Value;
    use assert_matches::assert_matches;
    use std::collections::HashMap;
    use std::collections::HashSet;

    #[test]
    fn env_builtin_returns_special_builtin() {
        let mut env = Env::new_virtual();
        let builtin = Builtin::new(Special, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        assert_eq!(env.builtin("foo"), Some((builtin, Availability::Available)));
    }

    #[test]
    fn env_builtin_returns_mandatory_builtin() {
        let mut env = Env::new_virtual();
        let builtin = Builtin::new(Mandatory, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        assert_eq!(env.builtin("foo"), Some((builtin, Availability::Available)));
    }

    #[test]
    fn env_builtin_returns_elective_builtin() {
        let mut env = Env::new_virtual();
        let builtin = Builtin::new(Elective, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        assert_eq!(env.builtin("foo"), Some((builtin, Availability::Available)));
    }

    #[test]
    fn env_builtin_returns_extension_builtin_if_not_posixly_correct() {
        let mut env = Env::new_virtual();
        let builtin = Builtin::new(Extension, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        assert_eq!(env.options.get(PosixlyCorrect), Off);
        assert_eq!(env.builtin("foo"), Some((builtin, Availability::Available)));
    }

    #[test]
    fn env_builtin_does_not_return_extension_builtin_if_posixly_correct() {
        let mut env = Env::new_virtual();
        env.options.set(PosixlyCorrect, On);
        let builtin = Builtin::new(Extension, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        assert_eq!(env.builtin("foo"), None);
    }

    #[test]
    fn env_builtin_rejects_elective_builtin_if_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, On);
        let builtin = Builtin::new(Elective, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        assert_eq!(
            env.builtin("foo"),
            Some((builtin, Availability::NotPortable))
        );
    }

    #[test]
    fn env_builtin_rejects_extension_builtin_if_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, On);
        let builtin = Builtin::new(Extension, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        assert_eq!(
            env.builtin("foo"),
            Some((builtin, Availability::NotPortable))
        );
    }

    #[test]
    fn env_builtin_accepts_posix_builtins_if_portable() {
        for r#type in [Mandatory, Substitutive] {
            let mut env = Env::new_virtual();
            env.options.set(Portable, On);
            let builtin = Builtin::new(r#type, |_, _| unreachable!());
            env.builtins.insert("foo", builtin);
            assert_eq!(
                env.builtin("foo"),
                Some((builtin, Availability::Available)),
                "type={type:?}"
            );
        }
    }

    #[test]
    fn env_builtin_accepts_special_builtin_with_posix_name_if_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, On);
        let builtin = Builtin::new(Special, |_, _| unreachable!());
        env.builtins.insert(".", builtin);
        assert_eq!(env.builtin("."), Some((builtin, Availability::Available)));
    }

    #[test]
    fn env_builtin_rejects_special_builtin_with_non_posix_name_if_portable() {
        let mut env = Env::new_virtual();
        env.options.set(Portable, On);
        let builtin = Builtin::new(Special, |_, _| unreachable!());
        env.builtins.insert("source", builtin);
        assert_eq!(
            env.builtin("source"),
            Some((builtin, Availability::NotPortable))
        );
    }

    #[test]
    fn env_builtin_returns_substitutive_builtin() {
        // The `$PATH` check for substitutive built-ins is part of `search`, not
        // `builtin`, so `builtin` returns the built-in regardless of `$PATH`.
        let mut env = Env::new_virtual();
        let builtin = Builtin::new(Substitutive, |_, _| unreachable!());
        env.builtins.insert("foo", builtin);
        assert_eq!(env.builtin("foo"), Some((builtin, Availability::Available)));
    }

    #[test]
    fn unusable_exit_status() {
        assert_eq!(Unusable::NotInPath.exit_status(), ExitStatus(127));
        assert_eq!(Unusable::NotPortable.exit_status(), ExitStatus(126));
    }

    #[test]
    fn unusable_display() {
        assert_eq!(
            Unusable::NotInPath.to_string(),
            "a substitutive built-in, so it cannot be used unless $PATH has an external utility \
             of the same name",
        );
        assert_eq!(
            Unusable::NotPortable.to_string(),
            "not a POSIX built-in, so it cannot be used while the `portable` option is on",
        );
    }

    #[test]
    fn error_exit_status() {
        assert_eq!(Error::NotFound.exit_status(), ExitStatus(127));
        assert_eq!(
            Error::Unusable(Unusable::NotInPath).exit_status(),
            ExitStatus(127),
        );
        assert_eq!(
            Error::Unusable(Unusable::NotPortable).exit_status(),
            ExitStatus(126),
        );
    }

    #[test]
    fn error_display() {
        assert_eq!(Error::NotFound.to_string(), "no command with this name");
        assert_eq!(
            Error::Unusable(Unusable::NotPortable).to_string(),
            "not a POSIX built-in, so it cannot be used while the `portable` option is on",
        );
    }

    #[derive(Default)]
    struct DummyEnv {
        builtins: HashMap<&'static str, (Builtin<()>, Availability)>,
        functions: FunctionSet<()>,
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

    impl ClassifyEnv<()> for DummyEnv {
        fn builtin(&self, name: &str) -> Option<(Builtin<()>, Availability)> {
            self.builtins.get(name).copied()
        }
        fn function(&self, name: &str) -> Option<&Rc<Function<()>>> {
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
    impl<S> FunctionBody<S> for FunctionBodyStub {
        async fn execute(&self, _: &mut Env<S>) -> crate::semantics::Result {
            unreachable!()
        }
    }

    fn function_body_stub<S>() -> Rc<dyn FunctionBodyObject<S>> {
        Rc::new(FunctionBodyStub)
    }

    #[test]
    fn resolve_builtin_returns_empty_path_for_non_substitutive_builtin() {
        let mut env = DummyEnv::default();

        let result = resolve_builtin(&mut env, "foo", Mandatory, Availability::Available);

        assert_eq!(result, Ok(CString::default()));
    }

    #[test]
    fn resolve_builtin_returns_external_path_for_substitutive_builtin() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from("/bin");
        env.executables.insert("/bin/foo".to_string());

        let result = resolve_builtin(&mut env, "foo", Substitutive, Availability::Available);

        assert_eq!(result, Ok(c"/bin/foo".to_owned()));
    }

    #[test]
    fn resolve_builtin_rejects_substitutive_builtin_missing_in_path() {
        let mut env = DummyEnv::default();

        let result = resolve_builtin(&mut env, "foo", Substitutive, Availability::Available);

        assert_eq!(result, Err(Unusable::NotInPath));
    }

    #[test]
    fn resolve_builtin_rejects_unavailable_builtin() {
        // The rejection takes precedence over the `$PATH` search, so a
        // substitutive built-in present in `$PATH` is still rejected.
        let mut env = DummyEnv::default();
        env.path = Expansion::from("/bin");
        env.executables.insert("/bin/foo".to_string());

        let result = resolve_builtin(&mut env, "foo", Substitutive, Availability::NotPortable);

        assert_eq!(result, Err(Unusable::NotPortable));
    }

    #[test]
    fn nothing_is_found_in_empty_env() {
        let mut env = DummyEnv::default();
        let target = search(&mut env, "foo");
        assert_eq!(target, Err(Error::NotFound));
    }

    #[test]
    fn nothing_is_found_with_name_unmatched() {
        let mut env = DummyEnv::default();
        env.builtins.insert(
            "foo",
            (
                Builtin::new(Special, |_, _| unreachable!()),
                Availability::Available,
            ),
        );
        let function = Function::new("foo", function_body_stub(), Location::dummy(""));
        env.functions.define(function).unwrap();

        let target = search(&mut env, "bar");
        assert_eq!(target, Err(Error::NotFound));
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
        env.builtins
            .insert("foo", (builtin, Availability::Available));

        assert_matches!(
            search(&mut env, "foo"),
            Ok(Target::Builtin { builtin: result, availability, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, availability, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
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

        assert_matches!(search(&mut env, "foo"), Ok(Target::Function(result)) => {
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
        env.builtins
            .insert("foo", (builtin, Availability::Available));
        let function = Function::new("foo", function_body_stub(), Location::dummy("location"));
        env.functions.define(function).unwrap();

        assert_matches!(
            search(&mut env, "foo"),
            Ok(Target::Builtin { builtin: result, availability, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, availability, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
    }

    #[test]
    fn mandatory_builtin_is_found_if_not_hidden_by_function() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Mandatory, |_, _| unreachable!());
        env.builtins
            .insert("foo", (builtin, Availability::Available));

        assert_matches!(
            search(&mut env, "foo"),
            Ok(Target::Builtin { builtin: result, availability, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, availability, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
    }

    #[test]
    fn elective_builtin_is_found_if_not_hidden_by_function() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Elective, |_, _| unreachable!());
        env.builtins
            .insert("foo", (builtin, Availability::Available));

        assert_matches!(
            search(&mut env, "foo"),
            Ok(Target::Builtin { builtin: result, availability, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, availability, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
    }

    #[test]
    fn extension_builtin_is_found_if_not_hidden_by_function_or_option() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Extension, |_, _| unreachable!());
        env.builtins
            .insert("foo", (builtin, Availability::Available));

        assert_matches!(
            search(&mut env, "foo"),
            Ok(Target::Builtin { builtin: result, availability, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, availability, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
    }

    #[test]
    fn function_takes_priority_over_mandatory_builtin() {
        let mut env = DummyEnv::default();
        env.builtins.insert(
            "foo",
            (
                Builtin::new(Mandatory, |_, _| unreachable!()),
                Availability::Available,
            ),
        );

        let function = Rc::new(Function::new(
            "foo",
            function_body_stub(),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Ok(Target::Function(result)) => {
            assert_eq!(result, function);
        });
        assert_matches!(classify(&env, "foo"), Target::Function(result) => {
            assert_eq!(result, function);
        });
    }

    #[test]
    fn function_takes_priority_over_elective_builtin() {
        let mut env = DummyEnv::default();
        env.builtins.insert(
            "foo",
            (
                Builtin::new(Elective, |_, _| unreachable!()),
                Availability::Available,
            ),
        );

        let function = Rc::new(Function::new(
            "foo",
            function_body_stub(),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Ok(Target::Function(result)) => {
            assert_eq!(result, function);
        });
        assert_matches!(classify(&env, "foo"), Target::Function(result) => {
            assert_eq!(result, function);
        });
    }

    #[test]
    fn function_takes_priority_over_extension_builtin() {
        let mut env = DummyEnv::default();
        env.builtins.insert(
            "foo",
            (
                Builtin::new(Extension, |_, _| unreachable!()),
                Availability::Available,
            ),
        );

        let function = Rc::new(Function::new(
            "foo",
            function_body_stub(),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Ok(Target::Function(result)) => {
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
        env.builtins
            .insert("foo", (builtin, Availability::Available));
        env.path = Expansion::from("/bin");
        env.executables.insert("/bin/foo".to_string());

        assert_matches!(
            search(&mut env, "foo"),
            Ok(Target::Builtin { builtin: result, availability, path }) => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"/bin/foo");
            }
        );
        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, availability, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
    }

    #[test]
    fn substitutive_builtin_is_unusable_without_external_executable() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Substitutive, |_, _| unreachable!());
        env.builtins
            .insert("foo", (builtin, Availability::Available));

        let target = search(&mut env, "foo");
        assert_eq!(target, Err(Error::Unusable(Unusable::NotInPath)));
    }

    #[test]
    fn builtin_rejected_by_options_is_unusable() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Mandatory, |_, _| unreachable!());
        env.builtins
            .insert("foo", (builtin, Availability::NotPortable));

        let target = search(&mut env, "foo");
        assert_eq!(target, Err(Error::Unusable(Unusable::NotPortable)));
    }

    #[test]
    fn builtin_rejected_by_options_is_still_classified() {
        // `classify` does not reject anything by itself, so the caller can
        // postpone the rejection until the built-in is about to be executed.
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Mandatory, |_, _| unreachable!());
        env.builtins
            .insert("foo", (builtin, Availability::NotPortable));

        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, availability, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::NotPortable);
                assert_eq!(*path, *c"");
            }
        );
    }

    #[test]
    fn substitutive_builtin_is_classified_even_without_external_executable() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Substitutive, |_, _| unreachable!());
        env.builtins
            .insert("foo", (builtin, Availability::Available));

        assert_matches!(
            classify(&env, "foo"),
            Target::Builtin { builtin: result, availability, path } => {
                assert_eq!(result.r#type, builtin.r#type);
                assert_eq!(availability, Availability::Available);
                assert_eq!(*path, *c"");
            }
        );
    }

    #[test]
    fn function_takes_priority_over_substitutive_builtin() {
        let mut env = DummyEnv::default();
        let builtin = Builtin::new(Substitutive, |_, _| unreachable!());
        env.builtins
            .insert("foo", (builtin, Availability::Available));
        env.path = Expansion::from("/bin");
        env.executables.insert("/bin/foo".to_string());

        let function = Rc::new(Function::new(
            "foo",
            function_body_stub(),
            Location::dummy("location"),
        ));
        env.functions.define(function.clone()).unwrap();

        assert_matches!(search(&mut env, "foo"), Ok(Target::Function(result)) => {
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

        assert_matches!(search(&mut env, "foo"), Ok(Target::External { path }) => {
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
        env.builtins
            .insert("bar/baz", (builtin, Availability::Available));

        assert_matches!(search(&mut env, "bar/baz"), Ok(Target::External { path }) => {
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

        assert_matches!(search(&mut env, "foo"), Ok(Target::External { path }) => {
            assert_eq!(*path, *c"/usr/bin/foo");
        });

        env.executables.insert("/usr/local/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Ok(Target::External { path }) => {
            assert_eq!(*path, *c"/usr/local/bin/foo");
        });
    }

    #[test]
    fn external_target_is_first_executable_found_in_path_array() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from(Value::array(["/usr/local/bin", "/usr/bin", "/bin"]));
        env.executables.insert("/usr/bin/foo".to_string());
        env.executables.insert("/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Ok(Target::External { path }) => {
            assert_eq!(*path, *c"/usr/bin/foo");
        });

        env.executables.insert("/usr/local/bin/foo".to_string());

        assert_matches!(search(&mut env, "foo"), Ok(Target::External { path }) => {
            assert_eq!(*path, *c"/usr/local/bin/foo");
        });
    }

    #[test]
    fn empty_string_in_path_names_current_directory() {
        let mut env = DummyEnv::default();
        env.path = Expansion::from("/x::/y");
        env.executables.insert("foo".to_string());

        assert_matches!(search(&mut env, "foo"), Ok(Target::External { path }) => {
            assert_eq!(*path, *c"foo");
        });
    }
}
