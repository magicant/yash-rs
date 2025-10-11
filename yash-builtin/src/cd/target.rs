// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2023 WATANABE Yuki
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

//! Part of the cd built-in that computes the target directory path

use super::Command;
use super::Mode;
use std::borrow::Cow;
use thiserror::Error;
use yash_env::Env;
use yash_env::path::Path;
use yash_env::path::PathBuf;
use yash_env::semantics::ExitStatus;
use yash_env::variable::HOME;
use yash_env::variable::OLDPWD;
use yash_syntax::source::Location;
#[allow(deprecated)]
use yash_syntax::source::pretty::{Annotation, AnnotationType, MessageBase};
use yash_syntax::source::pretty::{Report, ReportType, Snippet};

/// Indicates how the target directory was resolved.
#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Origin {
    /// The target is `$HOME`. (The operand was omitted.)
    Home,
    /// The target is `$OLDPWD`. (The operand was `-`.)
    Oldpwd,
    /// The target was resolved relative to a directory in `$CDPATH`.
    Cdpath,
    /// The target was resolved relative to the current working directory.
    Literal,
}

/// Error in computing the target directory
#[derive(Debug, Clone, Eq, Error, PartialEq)]
#[non_exhaustive]
pub enum TargetError {
    /// The operand is omitted, but `$HOME` is not set or is empty.
    #[error("$HOME not set")]
    UnsetHome {
        /// Location in the source code where the cd built-in is called
        location: Location,
    },

    /// The operand is `-`, but `$OLDPWD` is not set or is empty.
    #[error("$OLDPWD not set")]
    UnsetOldpwd {
        /// Location of the `-` operand
        location: Location,
    },

    /// Non-existing directory
    ///
    /// When the `-L` option is specified, the built-in tries to canonicalize
    /// path `a/b/../c` to `a/c`. This is only possible if `a/b` exists. This
    /// error is returned when `a/b` is not a directory.
    #[error("change target contains non-existing directory component")]
    NonExistingDirectory {
        /// Path to the non-existing directory
        missing: PathBuf,
        /// Entire path to the target directory
        target: PathBuf,
        /// Location in the source code where the target directory is specified
        location: Location,
    },
}

impl TargetError {
    /// Returns the exit status that corresponds to this error.
    #[must_use]
    pub fn exit_status(&self) -> ExitStatus {
        match self {
            TargetError::UnsetHome { .. } | TargetError::UnsetOldpwd { .. } => {
                super::EXIT_STATUS_UNSET_VARIABLE
            }
            TargetError::NonExistingDirectory { .. } => super::EXIT_STATUS_CANNOT_CANONICALIZE,
        }
    }

    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        use TargetError::*;
        let (location, label) = match self {
            UnsetHome { location } => (
                location,
                "cd built-in used without operand requires non-empty $HOME".into(),
            ),

            UnsetOldpwd { location } => (location, "'-' operand requires non-empty $OLDPWD".into()),

            NonExistingDirectory {
                missing,
                target: _,
                location,
            } => (
                location,
                format!("intermediate directory '{}' not found", missing.display()).into(),
            ),
        };

        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report.snippets = Snippet::with_primary_span(location, label);
        report
    }
}

impl<'a> From<&'a TargetError> for Report<'a> {
    #[inline]
    fn from(error: &'a TargetError) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl MessageBase for TargetError {
    fn message_title(&self) -> Cow<'_, str> {
        self.to_string().into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        use TargetError::*;
        match self {
            UnsetHome { location } => Annotation::new(
                AnnotationType::Error,
                "cd built-in used without operand requires non-empty $HOME".into(),
                location,
            ),

            UnsetOldpwd { location } => Annotation::new(
                AnnotationType::Error,
                "'-' operand requires non-empty $OLDPWD".into(),
                location,
            ),

            NonExistingDirectory {
                missing,
                target: _,
                location,
            } => Annotation::new(
                AnnotationType::Error,
                format!("intermediate directory '{}' not found", missing.display()).into(),
                location,
            ),
        }
    }

    fn additional_annotations<'a, T: Extend<Annotation<'a>>>(&'a self, results: &mut T) {
        use TargetError::*;
        match self {
            UnsetHome { location: _ } | UnsetOldpwd { location: _ } => {}

            NonExistingDirectory {
                missing: _,
                target,
                location,
            } => results.extend(std::iter::once(Annotation::new(
                AnnotationType::Info,
                format!(
                    "while resolving '..' in target directory '{}'",
                    target.display()
                )
                .into(),
                location,
            ))),
        }
    }
}

/// Returns the variable value if it is a non-empty scalar.
fn get_scalar<'a>(env: &'a Env, name: &str) -> Option<&'a str> {
    env.variables
        .get_scalar(name)
        .filter(|value| !value.is_empty())
}

/// Computes the target directory of the cd built-in.
///
/// This function implements steps 1 through 8 of the POSIX specification of the
/// cd built-in. Additionally, this function resolves a `-` operand to
/// `$OLDPWD`.
///
/// The `pwd` parameter should be the current value of `$PWD`. This is used to
/// resolve a logical path.
pub fn target(env: &Env, command: &Command, pwd: &str) -> Result<(PathBuf, Origin), TargetError> {
    // Step 1 & 2: substitute $HOME and $OLDPWD
    let (mut curpath, mut origin) = match &command.operand {
        None => {
            let home = get_scalar(env, HOME).ok_or_else(|| {
                let builtin = env.stack.current_builtin();
                let location =
                    builtin.map_or_else(|| Location::dummy(""), |b| b.name.origin.clone());
                TargetError::UnsetHome { location }
            })?;
            (PathBuf::from(home), Origin::Home)
        }

        Some(operand) if operand.value == "-" => {
            let oldpwd = get_scalar(env, OLDPWD).ok_or_else(|| TargetError::UnsetOldpwd {
                location: operand.origin.clone(),
            })?;
            (PathBuf::from(&oldpwd), Origin::Oldpwd)
        }

        Some(operand) => (PathBuf::from(&operand.value), Origin::Literal),
    };

    // Step 3 through 6: search $CDPATH
    if let Some(path) = super::cdpath::search(env, &curpath) {
        curpath = path;
        origin = Origin::Cdpath;
    }

    if command.mode == Mode::Physical {
        // Step 7-1: return the result
        return Ok((curpath, origin));
    }

    // Step 7-2: make the path absolute
    curpath = Path::new(pwd).join(curpath);
    // TODO The current Rust implementation joins "//" and "foo" into "/foo"
    // where "//foo" is expected, but Rust is not yet ported to platforms where
    // this difference matters. We may need to revisit this when Rust supports
    // such a platform, notably Cygwin.

    // Step 8: canonicalize the path
    curpath = super::canonicalize::canonicalize(&env.system, &curpath).map_err(|e| {
        TargetError::NonExistingDirectory {
            missing: e.missing,
            target: curpath,
            location: {
                let field = command.operand.as_ref();
                let field = field.or_else(|| env.stack.current_builtin().map(|b| &b.name));
                field.map_or_else(|| Location::dummy(""), |f| f.origin.clone())
            },
        }
    })?;

    Ok((curpath, origin))
    /*
    // step 1
    if (operand == NULL) {
        if (HOME == NULL || HOME == "") {
            return error;
        }
        // step 2
        operand = HOME;
    } else if (operand == "-") {
        operand = OLDPWD;
    }
    // step 3 & 4
    if (!operand.starts_with('/') &&
            !operand.starts_with_component(".") &&
            !operand.starts_with_component("..")) {
        // step 5
        curpath = resolve_cdpath(operand);
        if (curpath == NULL) {
            curpath = operand;
        }
    } else {
        // step 6
        curpath = operand;
    }
    // step 7
    if (logical) {
        if (!curpath.starts_with('/')) {
            curpath = PWD + '/' + curpath;
        }
        // step 8
        curpath = logical_canonicalize(curpath);
        // step 9
        curpath = relativize(curpath);
    }
    // step 10
    chdir(curpath);
    */
}

#[cfg(test)]
mod tests {
    use super::*;
    use yash_env::semantics::Field;
    use yash_env::stack::Builtin;
    use yash_env::stack::Frame;
    use yash_env::variable::Scope;

    #[test]
    fn default_home() {
        let mut env = Env::new_virtual();
        let command = Command {
            mode: Mode::default(),
            ensure_pwd: false,
            operand: None,
        };
        env.get_or_create_variable(HOME, Scope::Global)
            .assign("/home/user", None)
            .unwrap();

        let target = target(&env, &command, "").unwrap();
        assert_eq!(target, (PathBuf::from("/home/user"), Origin::Home));
    }

    #[test]
    fn home_unset() {
        let mut env = Env::new_virtual();
        let command = Command {
            mode: Mode::default(),
            ensure_pwd: false,
            operand: None,
        };
        let arg0 = Field::dummy("cd");
        let location = arg0.origin.clone();
        let env = env.push_frame(Frame::Builtin(Builtin {
            name: arg0,
            is_special: false,
        }));

        let e = target(&env, &command, "").unwrap_err();
        assert_eq!(e, TargetError::UnsetHome { location });
    }

    #[test]
    fn home_empty() {
        let mut env = Env::new_virtual();
        let command = Command {
            mode: Mode::default(),
            ensure_pwd: false,
            operand: None,
        };
        let arg0 = Field::dummy("cd");
        let location = arg0.origin.clone();
        let mut env = env.push_frame(Frame::Builtin(Builtin {
            name: arg0,
            is_special: false,
        }));
        env.get_or_create_variable(HOME, Scope::Global)
            .assign("", None)
            .unwrap();

        let e = target(&env, &command, "/ignored").unwrap_err();
        assert_eq!(e, TargetError::UnsetHome { location });
    }

    #[test]
    fn oldpwd() {
        let mut env = Env::new_virtual();
        let command = Command {
            mode: Mode::default(),
            ensure_pwd: false,
            operand: Some(Field::dummy("-")),
        };
        env.get_or_create_variable(OLDPWD, Scope::Global)
            .assign("/old/dir", None)
            .unwrap();

        let target = target(&env, &command, "/ignored").unwrap();
        assert_eq!(target, (PathBuf::from("/old/dir"), Origin::Oldpwd));
    }

    #[test]
    fn oldpwd_unset() {
        let env = Env::new_virtual();
        let operand = Field::dummy("-");
        let location = operand.origin.clone();
        let command = Command {
            mode: Mode::default(),
            ensure_pwd: false,
            operand: Some(operand),
        };

        let e = target(&env, &command, "/ignored").unwrap_err();
        assert_eq!(e, TargetError::UnsetOldpwd { location });
    }

    #[test]
    fn oldpwd_empty() {
        let mut env = Env::new_virtual();
        let operand = Field::dummy("-");
        let location = operand.origin.clone();
        let command = Command {
            mode: Mode::default(),
            ensure_pwd: false,
            operand: Some(operand),
        };
        env.get_or_create_variable(OLDPWD, Scope::Global)
            .assign("", None)
            .unwrap();

        let e = target(&env, &command, "/ignored").unwrap_err();
        assert_eq!(e, TargetError::UnsetOldpwd { location });
    }

    #[test]
    fn literal_physical() {
        let env = Env::new_virtual();

        let result = target(
            &env,
            &Command {
                mode: Mode::Physical,
                ensure_pwd: false,
                operand: Some(Field::dummy("foo")),
            },
            "/ignored",
        )
        .unwrap();
        assert_eq!(result, (PathBuf::from("foo"), Origin::Literal));

        let result = target(
            &env,
            &Command {
                mode: Mode::Physical,
                ensure_pwd: false,
                operand: Some(Field::dummy("foo/bar")),
            },
            "/ignored",
        )
        .unwrap();
        assert_eq!(result, (PathBuf::from("foo/bar"), Origin::Literal));
    }

    #[test]
    fn literal_logical_absolute() {
        let env = Env::new_virtual();

        let result = target(
            &env,
            &Command {
                mode: Mode::Logical,
                ensure_pwd: false,
                operand: Some(Field::dummy("/foo")),
            },
            "/ignored",
        )
        .unwrap();
        assert_eq!(result, (PathBuf::from("/foo"), Origin::Literal));

        let result = target(
            &env,
            &Command {
                mode: Mode::Logical,
                ensure_pwd: false,
                operand: Some(Field::dummy("/foo/bar")),
            },
            "/ignored",
        )
        .unwrap();
        assert_eq!(result, (PathBuf::from("/foo/bar"), Origin::Literal));
    }

    #[test]
    fn literal_logical_relative() {
        // The relative path is made absolute by prepending the current directory.
        let env = Env::new_virtual();
        let command = Command {
            mode: Mode::Logical,
            ensure_pwd: false,
            operand: Some(Field::dummy("foo/bar")),
        };

        assert_eq!(
            target(&env, &command, "/current/pwd").unwrap(),
            (PathBuf::from("/current/pwd/foo/bar"), Origin::Literal)
        );
    }
}
