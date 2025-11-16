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

use super::Identify;
use super::search::SearchEnv;
use crate::command::Category;
use crate::common::output;
use crate::common::report::{merge_reports, report_failure};
use std::borrow::Cow;
use std::ffi::CStr;
use std::ffi::CString;
use std::rc::Rc;
use yash_env::Env;
use yash_env::System;
use yash_env::alias::Alias;
use yash_env::builtin::{Builtin, Type};
use yash_env::parser::IsKeyword;
use yash_env::path::PathBuf;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::semantics::command::search::{Target, search};
#[allow(deprecated)]
use yash_env::source::pretty::{Annotation, AnnotationType, MessageBase};
use yash_env::source::pretty::{Report, ReportType, Snippet};
use yash_env::str::UnixStr;
use yash_quote::quoted;

/// Result of [categorizing](categorize) a command
///
/// # Notes on equality
///
/// Although this type implements `PartialEq`, comparison between instances of
/// this type may not always yield predictable results due to the presence of
/// function pointers in [`Target`]. As a result, it is recommended to avoid
/// relying on equality comparisons for values of this type. See
/// <https://doc.rust-lang.org/std/ptr/fn.fn_addr_eq.html> for the
/// characteristics of function pointer comparisons.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Categorization {
    /// Shell reserved word
    Keyword,
    /// Alias
    Alias(Rc<Alias>),
    /// Target program that can be executed
    Target(Target),
}

impl From<Rc<Alias>> for Categorization {
    fn from(alias: Rc<Alias>) -> Self {
        Self::Alias(alias)
    }
}

impl From<&Rc<Alias>> for Categorization {
    fn from(alias: &Rc<Alias>) -> Self {
        Self::Alias(Rc::clone(alias))
    }
}

impl From<Target> for Categorization {
    fn from(target: Target) -> Self {
        Self::Target(target)
    }
}

/// Error object for the command not found
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotFound<'a> {
    /// Command name that was not found
    pub name: &'a Field,
}

impl NotFound<'_> {
    /// Converts this error to a [`Report`].
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = "command not found".into();
        report.snippets = Snippet::with_primary_span(
            &self.name.origin,
            format!("{}: not found", self.name.value).into(),
        );
        report
    }
}

impl<'a> From<&'a NotFound<'a>> for Report<'a> {
    #[inline]
    fn from(error: &'a NotFound<'a>) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl MessageBase for NotFound<'_> {
    fn message_title(&self) -> Cow<'_, str> {
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
///
/// The error returned from this function does not contain any message because
/// this function is used only by [`categorize`], which only need to report
/// that the target is not found.
fn normalize_target<E: NormalizeEnv>(env: &E, target: &mut Target) -> Result<(), ()> {
    match target {
        Target::External { path }
        | Target::Builtin {
            builtin:
                Builtin {
                    r#type: Type::Substitutive,
                    ..
                },
            path,
        } => {
            if !env.is_executable_file(path) {
                return Err(());
            }
            if !path.as_bytes().starts_with(b"/") {
                let mut absolute_path = env.pwd()?;
                absolute_path.push(UnixStr::from_bytes(path.as_bytes()));
                *path = CString::new(absolute_path.into_unix_string().into_vec()).map_err(drop)?;
            }
            Ok(())
        }

        Target::Function(_) | Target::Builtin { .. } => Ok(()),
    }
}

/// Determines the category of the given command name.
///
/// This function requires an instance of [`IsKeyword`] to be present in the
/// environment's [`any`](Env::any) storage to check for keywords. If no such
/// instance is found, this function will **panic**.
pub fn categorize<'f>(
    name: &'f Field,
    env: &mut SearchEnv,
) -> Result<Categorization, NotFound<'f>> {
    if env.params.categories.contains(Category::Keyword) {
        let IsKeyword(is_keyword) = env.env.any.get().expect("IsKeyword not found in env.any");
        if is_keyword(env.env, &name.value) {
            return Ok(Categorization::Keyword);
        }
    }

    if env.params.categories.contains(Category::Alias) {
        if let Some(alias) = env.env.aliases.get(name.value.as_str()) {
            return Ok((&alias.0).into());
        }
    }

    let mut target = search(env, &name.value).ok_or(NotFound { name })?;
    normalize_target(env.env, &mut target).map_err(|()| NotFound { name })?;
    Ok(target.into())
}

/// Appends the description of the given target to the result.
///
/// This function is a specialized helper for [`describe`]. It produces the
/// description of the command search result that is to be printed to the
/// standard output.
pub fn describe_target<W>(
    target: &Target,
    name: &Field,
    verbose: bool,
    result: &mut W,
) -> std::fmt::Result
where
    W: std::fmt::Write,
{
    match target {
        Target::Builtin { builtin, path } => {
            let path = path.to_string_lossy();
            if verbose {
                let desc = match builtin.r#type {
                    Type::Special => "special built-in",
                    Type::Mandatory => "mandatory built-in",
                    Type::Elective => "elective built-in",
                    Type::Extension => "extension built-in",
                    Type::Substitutive => "substitutive built-in",
                };
                write!(result, "{}: {}", name.value, desc)?;
                if !path.is_empty() {
                    write!(result, " at {}", quoted(&path))?;
                }
                writeln!(result)?;
            } else {
                let output = if path.is_empty() {
                    &*name.value
                } else {
                    &*path
                };
                writeln!(result, "{output}")?;
            }
            Ok(())
        }

        Target::Function(_) => {
            if verbose {
                writeln!(result, "{}: function", name.value)?;
            } else {
                writeln!(result, "{}", name.value)?;
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
                )?;
            } else {
                writeln!(result, "{path}")?;
            }
            Ok(())
        }
    }
}

/// Appends the description of the given categorization to the result.
///
/// This function produces the description of the command search result that is
/// to be printed to the standard output.
pub fn describe<W>(
    categorization: &Categorization,
    name: &Field,
    verbose: bool,
    result: &mut W,
) -> std::fmt::Result
where
    W: std::fmt::Write,
{
    match categorization {
        Categorization::Keyword => {
            if verbose {
                writeln!(result, "{}: keyword", name.value)
            } else {
                writeln!(result, "{}", name.value)
            }
        }

        Categorization::Alias(alias) => {
            if verbose {
                writeln!(result, "{}: alias for `{}`", alias.name, alias.replacement)
            } else {
                write!(result, "alias ")?;
                if alias.name.starts_with('-') {
                    write!(result, "-- ")?;
                }
                writeln!(
                    result,
                    "{}={}",
                    quoted(&alias.name),
                    quoted(&alias.replacement)
                )
            }
        }

        Categorization::Target(target) => describe_target(target, name, verbose, result),
    }
}

impl Identify {
    /// Identifies the commands and returns the result.
    ///
    /// This function returns a string that should be printed to the standard
    /// output, as well as a list of errors that should be printed to the
    /// standard error.
    ///
    /// This function requires an instance of [`IsKeyword`] to be present in the
    /// environment's [`any`](Env::any) storage to check for keywords. If no
    /// such instance is found, this function will **panic**.
    pub fn result(&self, env: &mut Env) -> (String, Vec<NotFound<'_>>) {
        let params = &self.search;
        let env = &mut SearchEnv { env, params };
        let mut result = String::new();
        let mut errors = Vec::new();
        for name in &self.names {
            match categorize(name, env) {
                Ok(categorization) => {
                    describe(&categorization, name, self.verbose, &mut result).unwrap()
                }
                Err(error) => errors.push(error),
            }
        }
        (result, errors)
    }

    /// Performs the identifying semantics.
    pub async fn execute(&self, env: &mut Env) -> crate::Result {
        let (result, errors) = self.result(env);

        let output_result = output(env, &result).await;

        let error_result = if let Some(report) = merge_reports(&errors) {
            if self.verbose {
                report_failure(env, report).await
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
    use yash_env::alias::HashEntry;
    use yash_env::builtin::Builtin;
    use yash_env::function::{Function, FunctionBody, FunctionBodyObject};
    use yash_env::source::Location;

    #[derive(Clone, Debug)]
    struct FunctionBodyStub;

    impl std::fmt::Display for FunctionBodyStub {
        fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            unreachable!()
        }
    }
    impl FunctionBody for FunctionBodyStub {
        async fn execute(&self, _: &mut Env) -> yash_env::semantics::Result {
            unreachable!()
        }
    }

    fn function_body_stub() -> Rc<dyn FunctionBodyObject> {
        Rc::new(FunctionBodyStub)
    }

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
            path: c"/bin/sh".to_owned(),
        };
        let result = normalize_target(&TestEnv, &mut external_target);
        assert_eq!(result, Ok(()));
        assert_eq!(
            external_target,
            Target::External {
                path: c"/bin/sh".to_owned(),
            }
        );

        let builtin = Builtin::new(Type::Substitutive, |_, _| unreachable!());
        let mut builtin_target = Target::Builtin {
            builtin,
            path: c"/usr/bin/echo".to_owned(),
        };
        let result = normalize_target(&TestEnv, &mut builtin_target);
        assert_eq!(result, Ok(()));
        assert_eq!(
            builtin_target,
            Target::Builtin {
                builtin,
                path: c"/usr/bin/echo".to_owned(),
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
            path: c"foo/sh".to_owned(),
        };
        let result = normalize_target(&TestEnv, &mut external_target);
        assert_eq!(result, Ok(()));
        assert_eq!(
            external_target,
            Target::External {
                path: c"/bin/foo/sh".to_owned(),
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
            path: c"/bin/sh".to_owned(),
        };
        let result = normalize_target(&TestEnv, &mut external_target);
        assert_eq!(result, Err(()));
    }

    #[test]
    fn categorize_keyword() {
        let name = &Field::dummy("if");
        let env = &mut Env::new_virtual();
        env.any.insert(Box::new(IsKeyword(|_env, word| {
            assert_eq!(word, "if");
            true
        })));
        let params = &Search::default_for_identify();
        let env = &mut SearchEnv { env, params };

        let result = categorize(name, env);
        assert_eq!(result, Ok(Categorization::Keyword));
    }

    #[test]
    fn categorize_non_keyword() {
        let name = &Field::dummy("foo");
        let env = &mut Env::new_virtual();
        env.any.insert(Box::new(IsKeyword(|_env, word| {
            assert_eq!(word, "foo");
            false
        })));
        let params = &Search::default_for_identify();
        let env = &mut SearchEnv { env, params };

        let result = categorize(name, env);
        assert_eq!(result, Err(NotFound { name }));
    }

    #[test]
    fn excluding_keyword() {
        let name = &Field::dummy("if");
        let env = &mut Env::new_virtual();
        let params = &mut Search::default_for_identify();
        params.categories.remove(Category::Keyword);
        let env = &mut SearchEnv { env, params };

        let result = categorize(name, env);
        assert_eq!(result, Err(NotFound { name }));
    }

    #[test]
    fn categorize_alias() {
        let name = &Field::dummy("a");
        let env = &mut Env::new_virtual();
        let entry = HashEntry::new(
            "a".to_string(),
            "A".to_string(),
            false,
            Location::dummy("a"),
        );
        let alias = entry.0.clone();
        env.aliases.insert(entry);
        env.any.insert(Box::new(IsKeyword(|_, _| false)));
        let params = &Search::default_for_identify();
        let env = &mut SearchEnv { env, params };

        let result = categorize(name, env);
        assert_eq!(result, Ok(Categorization::Alias(alias)));
    }

    #[test]
    fn categorize_non_alias() {
        let name = &Field::dummy("a");
        let env = &mut Env::new_virtual();
        env.any.insert(Box::new(IsKeyword(|_, _| false)));
        let params = &Search::default_for_identify();
        let env = &mut SearchEnv { env, params };

        let result = categorize(name, env);
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
        env.any.insert(Box::new(IsKeyword(|_, _| false)));
        let params = &mut Search::default_for_identify();
        params.categories.remove(Category::Alias);
        let env = &mut SearchEnv { env, params };

        let result = categorize(name, env);
        assert_eq!(result, Err(NotFound { name }));
    }

    #[test]
    fn describe_builtin_without_path() {
        let name = &Field::dummy(":");
        let target = &Target::Builtin {
            builtin: Builtin::new(Type::Special, |_, _| unreachable!()),
            path: CString::default(),
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
            builtin: Builtin::new(Type::Substitutive, |_, _| unreachable!()),
            path: c"/bin/echo".to_owned(),
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
        let location = Location::dummy("f");
        let function = Function::new("f", function_body_stub(), location);
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
            path: c"/bin/ls".to_owned(),
        };

        let mut output = String::new();
        describe_target(target, name, false, &mut output).unwrap();
        assert_eq!(output, "/bin/ls\n");

        let mut output = String::new();
        describe_target(target, name, true, &mut output).unwrap();
        assert_eq!(output, "ls: external utility at /bin/ls\n");
    }

    #[test]
    fn describe_keyword() {
        let categorization = &Categorization::Keyword;
        let name = &Field::dummy("if");

        let mut output = String::new();
        let result = describe(categorization, name, false, &mut output);
        assert_eq!(result, Ok(()));
        assert_eq!(output, "if\n");

        let mut output = String::new();
        let result = describe(categorization, name, true, &mut output);
        assert_eq!(result, Ok(()));
        assert_eq!(output, "if: keyword\n");
    }

    #[test]
    fn describe_alias() {
        let categorization = &Categorization::Alias(Rc::new(Alias {
            name: "foo".to_string(),
            replacement: "bar".to_string(),
            global: false,
            origin: Location::dummy("dummy location"),
        }));
        let name = &Field::dummy("foo");

        let mut output = String::new();
        let result = describe(categorization, name, false, &mut output);
        assert_eq!(result, Ok(()));
        assert_eq!(output, "alias foo=bar\n");

        let mut output = String::new();
        let result = describe(categorization, name, true, &mut output);
        assert_eq!(result, Ok(()));
        assert_eq!(output, "foo: alias for `bar`\n");
    }

    #[test]
    fn describe_alias_starting_with_hyphen() {
        let categorization = &Categorization::Alias(Rc::new(Alias {
            name: "-foo".to_string(),
            replacement: "bar".to_string(),
            global: false,
            origin: Location::dummy("dummy location"),
        }));
        let name = &Field::dummy("-foo");

        let mut output = String::new();
        let result = describe(categorization, name, false, &mut output);
        assert_eq!(result, Ok(()));
        assert_eq!(output, "alias -- -foo=bar\n");
    }

    #[test]
    fn identify_result_without_error() {
        let env = &mut Env::new_virtual();
        env.any.insert(Box::new(IsKeyword(|_, _| true)));

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
        env.any
            .insert(Box::new(IsKeyword(|_, word| word == "if" || word == "fi")));
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
