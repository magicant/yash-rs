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

//! Helper items for printing expansion results
//!
//! When the `xtrace` [shell option](yash_env::option) is on, the shell traces
//! words expanded during command execution. For each command executed, the
//! shell prints to the standard error a line containing the following:
//!
//! - An expansion of `$PS4`
//! - Expanded command words (assignments, command name, and arguments)
//! - Expanded redirections, possibly followed by here-document contents
//!
//! The trace of a command is printed at a time even though many separate steps
//! perform expansions during the execution of the command. [`XTrace`] is a
//! collection of string buffers that accumulates the results of expansions
//! until they are printed to the standard error.
//!
//! It is no use to collect the expansions when the `xtrace` option is off, so
//! you should create an `XTrace` only if the option is on.
//! [`XTrace::from_options`] is a convenient method to do so.

use crate::expansion::expand_text;
use crate::Handle;
use std::fmt::Write;
use yash_env::io::Stderr;
use yash_env::option::OptionSet;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_env::variable::Value::Scalar;
use yash_env::variable::Variable;
use yash_env::Env;
use yash_quote::quoted;
use yash_syntax::syntax::Text;

fn join(a: String, b: String) -> String {
    if a.is_empty() {
        b
    } else {
        a + &b
    }
}

async fn expand_ps4(env: &mut Env) -> String {
    let value = match env.variables.get("PS4") {
        Some(Variable {
            value: Some(Scalar(value)),
            ..
        }) => value.to_owned(),

        _ => String::new(),
    };

    let text = match value.parse::<Text>() {
        Ok(text) => text,
        Err(error) => {
            error.handle(env).await;
            return value;
        }
    };

    match expand_text(env, &text).await {
        Ok((expansion, _exit_status)) => expansion,
        Err(error) => {
            error.handle(env).await;
            value
        }
    }
}

/// Collection of temporary string buffers that accumulate expanded strings
///
/// See the [module documentation](self) for details.
///
/// An `XTrace` contains three string buffers that accumulate each of the
/// following:
///
/// - Command words (assignments, command name, and arguments)
/// - Redirections
/// - Here-document contents
///
/// The [`finish`](Self::finish) function creates the final string to be
/// printed.
#[derive(Clone, Debug, Default)]
pub struct XTrace {
    main: String,
    redirs: String,
    here_doc_contents: String,
}

impl XTrace {
    /// Creates a new trace buffer.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new trace buffer if the `xtrace` option is on.
    ///
    /// If the option is off, this function returns `None`.
    #[must_use]
    pub fn from_options(options: &OptionSet) -> Option<Self> {
        match options.get(yash_env::option::Option::XTrace) {
            State::On => Some(Self::new()),
            State::Off => None,
        }
    }

    /// Returns a reference to the main buffer.
    ///
    /// The main buffer is for tracing command words and assignments.
    /// When writing to the buffer, the content should end with a space.
    #[inline]
    #[must_use]
    pub fn main(&mut self) -> &mut impl Write {
        &mut self.main
    }

    /// Returns a reference to the redirections buffer.
    ///
    /// The redirections buffer is for tracing redirections.
    /// When writing to the buffer, the content should end with a space.
    ///
    /// You should not write the contents of here-documents to this buffer.
    /// See also [`here_doc_contents`](Self::here_doc_contents).
    #[inline]
    #[must_use]
    pub fn redirs(&mut self) -> &mut impl Write {
        &mut self.redirs
    }

    /// Returns a reference to the here-document contents buffer.
    ///
    /// You should write the contents of here-documents you wrote to the
    /// [redirections buffer](Self::redirs()).
    #[inline]
    #[must_use]
    pub fn here_doc_contents(&mut self) -> &mut impl Write {
        &mut self.here_doc_contents
    }

    /// Clears the buffer contents.
    pub fn clear(&mut self) {
        self.main.clear();
        self.redirs.clear();
        self.here_doc_contents.clear();
    }

    /// Constructs the final trace to be printed to stderr.
    ///
    /// If all the buffers are empty, the result is empty. Otherwise, the result
    /// is the concatenation of the following:
    ///
    /// - The expansion of `$PS4`
    /// - The concatenation of the `main` and `redirs` buffers with trailing
    ///   spaces trimmed and a newline appended
    /// - The `here_doc_contents` buffer
    ///
    /// If `$PS4` fails to expand, this function prints an error message and
    /// uses the variable value intact.
    pub async fn finish(self, env: &mut Env) -> String {
        let mut result = join(self.main, self.redirs);
        if !result.is_empty() || !self.here_doc_contents.is_empty() {
            let ps4 = expand_ps4(env).await;
            // TODO Support $YASH_PS4 and $YASH_PS4S
            result.truncate(result.trim_end_matches(' ').len());
            result = join(ps4, result);
            result.push('\n');
        }
        join(result, self.here_doc_contents)
    }
}

/// Convenience function for tracing fields.
///
/// This function writes the field values to the main buffer of the `XTrace`.
pub fn trace_fields(xtrace: Option<&mut XTrace>, fields: &[Field]) {
    if let Some(xtrace) = xtrace {
        for field in fields {
            write!(xtrace.main(), "{} ", quoted(&field.value)).unwrap();
        }
    }
}

/// Convenience function for calling [`XTrace::finish`] on an optional `XTrace`.
pub async fn finish(env: &mut Env, xtrace: Option<XTrace>) -> String {
    if let Some(xtrace) = xtrace {
        xtrace.finish(env).await
    } else {
        String::new()
    }
}

/// Convenience function for [finish]ing and [print](Stderr::print_error)ing an
/// (optional) `XTrace`.
pub async fn print<X: Into<Option<XTrace>>>(env: &mut Env, xtrace: X) {
    async fn inner(env: &mut Env, xtrace: Option<XTrace>) {
        let s = finish(env, xtrace).await;
        env.system.print_error(&s).await;
    }
    inner(env, xtrace.into()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use yash_env::variable::Scope::Global;

    #[test]
    fn joining() {
        assert_eq!(join(String::new(), String::new()), "");
        assert_eq!(join("foo".to_owned(), String::new()), "foo");
        assert_eq!(join(String::new(), "bar".to_owned()), "bar");
        assert_eq!(join("to".to_owned(), "night".to_owned()), "tonight");
    }

    #[test]
    fn tracing_some_fields() {
        let mut xtrace = XTrace::new();
        let fields = Field::dummies(["one", "two", "'three'"]);
        trace_fields(Some(&mut xtrace), &fields);
        assert_eq!(xtrace.main, r#"one two "'three'" "#);
        assert_eq!(xtrace.redirs, "");
        assert_eq!(xtrace.here_doc_contents, "");
    }

    fn fixture() -> Env {
        let mut env = Env::new_virtual();
        env.variables
            .assign(Global, "PS4".to_owned(), Variable::new("+${X=x}+ "))
            .unwrap();
        env
    }

    #[test]
    fn empty_finish() {
        let mut env = fixture();
        let result = XTrace::new().finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "");

        // $PS4 should not have been expanded
        assert_eq!(env.variables.get("X"), None);
    }

    #[test]
    fn finish_with_main() {
        let mut env = fixture();

        let mut xtrace = XTrace::new();
        xtrace.main.push_str("abc '~' foo ");
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "+x+ abc '~' foo\n");

        // $PS4 should have been expanded
        assert_ne!(env.variables.get("X"), None);
    }

    #[test]
    fn finish_with_redirs() {
        let mut env = fixture();

        let mut xtrace = XTrace::new();
        xtrace.redirs.push_str("0< /dev/null 1> foo/bar ");
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "+x+ 0< /dev/null 1> foo/bar\n");
    }

    #[test]
    fn finish_with_main_and_redirs() {
        let mut env = fixture();

        let mut xtrace = XTrace::new();
        xtrace.main.push_str("VAR=X echo argument ");
        xtrace.redirs.push_str("2> errors ");
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "+x+ VAR=X echo argument 2> errors\n");

        let mut xtrace = XTrace::new();
        xtrace.main.push(' ');
        xtrace.redirs.push(' ');
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "+x+ \n");
    }

    #[test]
    fn finish_with_here_doc_contents() {
        let mut env = fixture();

        let mut xtrace = XTrace::new();
        xtrace.here_doc_contents.push_str("EOF\n");
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "+x+ \nEOF\n");
    }

    #[test]
    fn finish_with_redirs_and_here_doc_contents() {
        let mut env = fixture();

        let mut xtrace = XTrace::new();
        xtrace.redirs.push_str("0<< END ");
        xtrace.here_doc_contents.push_str(" X \nEND\n");
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "+x+ 0<< END\n X \nEND\n");
    }
}
