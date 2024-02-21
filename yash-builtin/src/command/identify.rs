// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Command identifying semantics

use super::search::SearchEnv;
use super::Identify;
use crate::command::Category;
use crate::common::{output, report_failure, to_single_message};
use std::borrow::Cow;
use std::ffi::CStr;
use std::ffi::CString;
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::ffi::OsStringExt as _;
use std::path::PathBuf;
use yash_env::builtin::Type;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::Env;
use yash_env::System;
use yash_quote::quoted;
use yash_semantics::command_search::search;
use yash_semantics::command_search::Target;
use yash_syntax::parser::lex::Keyword;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::MessageBase;

/// Error object for the command not found
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotFound<'a> {
    /// Command name that was not found
    pub name: &'a Field,
}

impl MessageBase for NotFound<'_> {
    fn message_title(&self) -> Cow<str> {
        "command not found".into()
    }

    fn main_annotation(&self) -> Annotation<'_> {
        let label = format!("{}: not found", self.name.value).into();
        let location = &self.name.origin;
        Annotation::new(AnnotationType::Error, label, location)
    }
}

/// Environment for [normalizing a target](normalize_target).
trait NormalizeEnv {
    fn is_executable_file(&self, path: &CStr) -> bool;
    fn pwd(&self) -> Result<PathBuf, ()>;
}

impl NormalizeEnv for Env {
    #[inline]
    fn is_executable_file(&self, path: &CStr) -> bool {
        self.system.is_executable_file(path)
    }

    fn pwd(&self) -> Result<PathBuf, ()> {
        match self.get_pwd_if_correct() {
            Some(pwd) => Ok(pwd.into()),
            None => self.system.getcwd().map_err(|_| ()),
        }
    }
}

/// Updates the target to make it suitable for the [description](describe_target).
///
/// This function makes sure any path contained in the target is absolute and
/// names an executable file. If the path cannot be normalized, this function
/// returns an error.
fn normalize_target<E: NormalizeEnv>(env: &E, target: &mut Target) -> Result<(), ()> {
    match target {
        Target::Function(_) | Target::Builtin { path: None, .. } => Ok(()),

        Target::External { path }
        | Target::Builtin {
            path: Some(path), ..
        } => {
            if !env.is_executable_file(path) {
                return Err(());
            }
            if !path.as_bytes().starts_with(b"/") {
                let mut absolute_path = env.pwd()?;
                absolute_path.push(OsStr::from_bytes(path.as_bytes()));
                *path = CString::new(absolute_path.into_os_string().into_vec()).map_err(|_| ())?;
            }
            Ok(())
        }
    }
}

/// Appends the description of the given target to the result.
///
/// This function is a helper for [`identify`]. It produces the description of
/// the command search result that is to be printed to the standard output.
fn describe_target<'f, W>(
    target: &Target,
    name: &'f Field,
    verbose: bool,
    result: &mut W,
) -> Result<(), NotFound<'f>>
where
    W: std::fmt::Write,
{
    match target {
        Target::Builtin { builtin, path } => {
            let path = path.as_ref().map(|p| p.to_string_lossy());
            if verbose {
                let desc = match builtin.r#type {
                    Type::Special => "special built-in",
                    Type::Mandatory => "mandatory built-in",
                    Type::Elective => "elective built-in",
                    Type::Extension => "extension built-in",
                    Type::Substitutive => "substitutive built-in",
                };
                write!(result, "{}: {}", name.value, desc).unwrap();
                if let Some(path) = path {
                    write!(result, " at {}", quoted(&path)).unwrap();
                }
                writeln!(result).unwrap();
            } else {
                let output = path.as_deref().unwrap_or(&name.value);
                writeln!(result, "{}", output).unwrap();
            }
            Ok(())
        }

        Target::Function(_) => {
            if verbose {
                writeln!(result, "{}: function", name.value).unwrap();
            } else {
                writeln!(result, "{}", name.value).unwrap();
            }
            Ok(())
        }

        Target::External { path } => {
            let path = path.to_string_lossy();
            if verbose {
                writeln!(
                    result,
                    "{}: external utility at {}",
                    name.value,
                    quoted(&path)
                )
                .unwrap();
            } else {
                writeln!(result, "{}", path).unwrap();
            }
            Ok(())
        }
    }
}

/// Identifies the command and appends the description to the result.
///
/// This function produces the description of the command search result that is
/// to be printed to the standard output.
pub fn identify<'f, W>(
    name: &'f Field,
    env: &mut SearchEnv,
    verbose: bool,
    result: &mut W,
) -> Result<(), NotFound<'f>>
where
    W: std::fmt::Write,
{
    if env.params.categories.contains(Category::Keyword)
        && Keyword::try_from(name.value.as_str()).is_ok()
    {
        if verbose {
            writeln!(result, "{}: keyword", name.value).unwrap();
        } else {
            writeln!(result, "{}", name.value).unwrap();
        }
        return Ok(());
    }

    if env.params.categories.contains(Category::Alias) {
        if let Some(alias) = env.env.aliases.get(name.value.as_str()) {
            if verbose {
                writeln!(
                    result,
                    "{}: alias for `{}`",
                    alias.0.name, alias.0.replacement
                )
                .unwrap();
            } else {
                write!(result, "alias ").unwrap();
                if alias.0.name.starts_with('-') {
                    write!(result, "-- ").unwrap();
                }
                writeln!(
                    result,
                    "{}={}",
                    quoted(&alias.0.name),
                    quoted(&alias.0.replacement)
                )
                .unwrap();
            }
            return Ok(());
        }
    }

    let mut target = search(env, &name.value).ok_or(NotFound { name })?;
    normalize_target(env.env, &mut target).map_err(|()| NotFound { name })?;
    describe_target(&target, name, verbose, result)
}

impl Identify {
    /// Identifies the commands and returns the result.
    ///
    /// This function returns a string that should be printed to the standard
    /// output, as well as a list of errors that should be printed to the
    /// standard error.
    pub fn result(&self, env: &mut Env) -> (String, Vec<NotFound>) {
        let params = &self.search;
        let env = &mut SearchEnv { env, params };
        let mut result = String::new();
        let mut errors = Vec::new();
        for name in &self.names {
            if let Err(error) = identify(name, env, self.verbose, &mut result) {
                errors.push(error);
            }
        }
        (result, errors)
    }

    /// Performs the identifying semantics.
    pub async fn execute(&self, env: &mut Env) -> crate::Result {
        let (result, errors) = self.result(env);

        let output_result = output(env, &result).await;

        let error_result = if let Some(message) = to_single_message(&errors) {
            if self.verbose {
                report_failure(env, message).await
            } else {
                crate::Result::from(ExitStatus::FAILURE)
            }
        } else {
            crate::Result::default()
        };

        output_result.max(error_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Search;
    use std::ffi::CString;
    use yash_env::builtin::Builtin;
    use yash_env::function::Function;
    use yash_syntax::alias::HashEntry;
    use yash_syntax::source::Location;
    use yash_syntax::syntax::FullCompoundCommand;

    #[test]
    fn normalize_absolute_executable() {
        struct TestEnv;
        impl NormalizeEnv for TestEnv {
            fn is_executable_file(&self, _path: &CStr) -> bool {
                true
            }
            fn pwd(&self) -> Result<PathBuf, ()> {
                unreachable!()
            }
        }

        let mut external_target = Target::External {
            path: CString::new("/bin/sh").unwrap(),
        };
        let result = normalize_target(&TestEnv, &mut external_target);
        assert_eq!(result, Ok(()));
        assert_eq!(
            external_target,
            Target::External {
                path: CString::new("/bin/sh").unwrap(),
            }
        );

        let builtin = Builtin {
            r#type: Type::Substitutive,
            execute: |_, _| unreachable!(),
        };
        let mut builtin_target = Target::Builtin {
            builtin,
            path: Some(CString::new("/usr/bin/echo").unwrap()),
        };
        let result = normalize_target(&TestEnv, &mut builtin_target);
        assert_eq!(result, Ok(()));
        assert_eq!(
            builtin_target,
            Target::Builtin {
                builtin,
                path: Some(CString::new("/usr/bin/echo").unwrap()),
            }
        );
    }

    #[test]
    fn normalize_relative_executable() {
        struct TestEnv;
        impl NormalizeEnv for TestEnv {
            fn is_executable_file(&self, _path: &CStr) -> bool {
                true
            }
            fn pwd(&self) -> Result<PathBuf, ()> {
                Ok(PathBuf::from("/bin"))
            }
        }

        let mut external_target = Target::External {
            path: CString::new("foo/sh").unwrap(),
        };
        let result = normalize_target(&TestEnv, &mut external_target);
        assert_eq!(result, Ok(()));
        assert_eq!(
            external_target,
            Target::External {
                path: CString::new("/bin/foo/sh").unwrap(),
            }
        );
    }

    #[test]
    fn normalize_non_executable() {
        struct TestEnv;
        impl NormalizeEnv for TestEnv {
            fn is_executable_file(&self, _path: &CStr) -> bool {
                false
            }
            fn pwd(&self) -> Result<PathBuf, ()> {
                unreachable!()
            }
        }

        let mut external_target = Target::External {
            path: CString::new("/bin/sh").unwrap(),
        };
        let result = normalize_target(&TestEnv, &mut external_target);
        assert_eq!(result, Err(()));
    }

    #[test]
    fn identify_keyword() {
        let name = &Field::dummy("if");
        let env = &mut Env::new_virtual();
        let params = &Search::default_for_identify();
        let env = &mut SearchEnv { env, params };

        let mut output = String::new();
        identify(name, env, false, &mut output).unwrap();
        assert_eq!(output, "if\n");

        let mut output = String::new();
        identify(name, env, true, &mut output).unwrap();
        assert_eq!(output, "if: keyword\n");
    }

    #[test]
    fn identify_non_keyword() {
        let name = &Field::dummy("foo");
        let env = &mut Env::new_virtual();
        let params = &Search::default_for_identify();
        let env = &mut SearchEnv { env, params };

        let mut output = String::new();
        let result = identify(name, env, false, &mut output);
        assert_eq!(output, "");
        assert_eq!(result, Err(NotFound { name }));
    }

    #[test]
    fn excluding_keyword() {
        let name = &Field::dummy("if");
        let env = &mut Env::new_virtual();
        let params = &mut Search::default_for_identify();
        params.categories.remove(Category::Keyword);
        let env = &mut SearchEnv { env, params };

        let mut output = String::new();
        let result = identify(name, env, false, &mut output);
        assert_eq!(output, "");
        assert_eq!(result, Err(NotFound { name }));
    }

    #[test]
    fn identify_alias() {
        let name = &Field::dummy("a");
        let env = &mut Env::new_virtual();
        env.aliases.insert(HashEntry::new(
            "a".to_string(),
            "A".to_string(),
            false,
            Location::dummy("a"),
        ));
        let params = &Search::default_for_identify();
        let env = &mut SearchEnv { env, params };

        let mut output = String::new();
        identify(name, env, false, &mut output).unwrap();
        assert_eq!(output, "alias a=A\n");

        let mut output = String::new();
        identify(name, env, true, &mut output).unwrap();
        assert_eq!(output, "a: alias for `A`\n");
    }

    #[test]
    fn identify_non_alias() {
        let name = &Field::dummy("a");
        let env = &mut Env::new_virtual();
        let params = &Search::default_for_identify();
        let env = &mut SearchEnv { env, params };

        let mut output = String::new();
        let result = identify(name, env, false, &mut output);
        assert_eq!(output, "");
        assert_eq!(result, Err(NotFound { name }));
    }

    #[test]
    fn excluding_alias() {
        let name = &Field::dummy("a");
        let env = &mut Env::new_virtual();
        env.aliases.insert(HashEntry::new(
            "a".to_string(),
            "A".to_string(),
            false,
            Location::dummy("a"),
        ));
        let params = &mut Search::default_for_identify();
        params.categories.remove(Category::Alias);
        let env = &mut SearchEnv { env, params };

        let mut output = String::new();
        let result = identify(name, env, false, &mut output);
        assert_eq!(output, "");
        assert_eq!(result, Err(NotFound { name }));
    }

    #[test]
    fn describe_builtin_without_path() {
        let name = &Field::dummy(":");
        let target = &Target::Builtin {
            builtin: Builtin {
                r#type: Type::Special,
                execute: |_, _| unreachable!(),
            },
            path: None,
        };

        let mut output = String::new();
        describe_target(target, name, false, &mut output).unwrap();
        assert_eq!(output, ":\n");

        let mut output = String::new();
        describe_target(target, name, true, &mut output).unwrap();
        assert_eq!(output, ":: special built-in\n");
    }

    #[test]
    fn describe_builtin_with_path() {
        let name = &Field::dummy("echo");
        let target = &Target::Builtin {
            builtin: Builtin {
                r#type: Type::Substitutive,
                execute: |_, _| unreachable!(),
            },
            path: Some(CString::new("/bin/echo").unwrap()),
        };

        let mut output = String::new();
        describe_target(target, name, false, &mut output).unwrap();
        assert_eq!(output, "/bin/echo\n");

        let mut output = String::new();
        describe_target(target, name, true, &mut output).unwrap();
        assert_eq!(output, "echo: substitutive built-in at /bin/echo\n");
    }

    #[test]
    fn describe_function() {
        let name = &Field::dummy("f");
        let command: FullCompoundCommand = "{ :; }".parse().unwrap();
        let location = Location::dummy("f");
        let function = Function::new("f", command, location);
        let target = &Target::Function(function.into());

        let mut output = String::new();
        describe_target(target, name, false, &mut output).unwrap();
        assert_eq!(output, "f\n");

        let mut output = String::new();
        describe_target(target, name, true, &mut output).unwrap();
        assert_eq!(output, "f: function\n");
    }

    #[test]
    fn describe_external() {
        let name = &Field::dummy("ls");
        let target = &Target::External {
            path: CString::new("/bin/ls").unwrap(),
        };

        let mut output = String::new();
        describe_target(target, name, false, &mut output).unwrap();
        assert_eq!(output, "/bin/ls\n");

        let mut output = String::new();
        describe_target(target, name, true, &mut output).unwrap();
        assert_eq!(output, "ls: external utility at /bin/ls\n");
    }

    #[test]
    fn identify_result_without_error() {
        let env = &mut Env::new_virtual();

        let mut identify = Identify::default();
        let (result, errors) = identify.result(env);
        assert_eq!(result, "");
        assert_eq!(errors, []);

        identify.names.push(Field::dummy("if"));
        let (result, errors) = identify.result(env);
        assert_eq!(result, "if\n");
        assert_eq!(errors, []);

        identify.verbose = true;
        let (result, errors) = identify.result(env);
        assert_eq!(result, "if: keyword\n");
        assert_eq!(errors, []);

        identify.names.push(Field::dummy("fi"));
        let (result, errors) = identify.result(env);
        assert_eq!(result, "if: keyword\nfi: keyword\n");
        assert_eq!(errors, []);
    }

    #[test]
    fn identify_result_with_error() {
        let env = &mut Env::new_virtual();
        let identify = Identify {
            names: Field::dummies(["if", "oops", "fi", "bar"]),
            ..Identify::default()
        };

        let (result, errors) = identify.result(env);

        assert_eq!(result, "if\nfi\n");
        assert_eq!(
            errors,
            [
                NotFound {
                    name: &Field::dummy("oops")
                },
                NotFound {
                    name: &Field::dummy("bar")
                }
            ]
        );
    }
}
