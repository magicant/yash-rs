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

use crate::Handle;
use crate::expansion::expand_text;
use std::fmt::Write;
use std::ops::{Deref, DerefMut};
use yash_env::Env;
use yash_env::option::OptionSet;
use yash_env::option::State;
use yash_env::semantics::Field;
use yash_env::variable::PS4;
use yash_quote::quoted;
use yash_syntax::syntax::Text;

async fn expand_ps4(env: &mut Env) -> String {
    let value = env.variables.get_scalar(PS4).unwrap_or_default().to_owned();

    let text = match value.parse::<Text>() {
        Ok(text) => text,
        Err(error) => {
            _ = error.handle(env).await;
            return value;
        }
    };

    match expand_text(env, &text).await {
        Ok((expansion, _exit_status)) => expansion,
        Err(error) => {
            _ = error.handle(env).await;
            value
        }
    }
}

/// Flag to indicate whether `$PS4` is being expanded
///
/// This is used in [`XTrace::finish`] to prevent (possibly infinite) recursion
/// when `$PS4` contains a command substitution that causes `XTrace::finish` to
/// be called again.
///
/// This flag is stored in [`Env::any`].
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ExpandingPs4(bool);

/// Guard that sets the [`ExpandingPs4`] flag to true while it is alive
/// and resets it to false when dropped
#[derive(Debug)]
struct ExpandingGuard<'a>(&'a mut Env);

impl Deref for ExpandingGuard<'_> {
    type Target = Env;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl DerefMut for ExpandingGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.0
    }
}

impl<'a> ExpandingGuard<'a> {
    /// Creates a new guard that sets the [`ExpandingPs4`] flag to true.
    ///
    /// If the flag is already true, this function returns `None`.
    #[must_use]
    fn new(env: &'a mut Env) -> Option<Self> {
        let expanding_ps4 = env.any.get_or_insert_with(Box::<ExpandingPs4>::default);
        if expanding_ps4.0 {
            None
        } else {
            expanding_ps4.0 = true;
            Some(Self(env))
        }
    }
}

impl Drop for ExpandingGuard<'_> {
    fn drop(&mut self) {
        if let Some(expanding_ps4) = self.any.get_mut::<ExpandingPs4>() {
            expanding_ps4.0 = false;
        }
    }
}

/// Collection of temporary string buffers that accumulate expanded strings
///
/// See the [module documentation](self) for details.
///
/// An `XTrace` contains four string buffers that accumulate each of the
/// following:
///
/// - Command words (command name and arguments)
/// - Assignments
/// - Redirections
/// - Here-document contents
///
/// The [`finish`](Self::finish) function creates the final string to be
/// printed.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct XTrace {
    words: String,
    assigns: String,
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

    /// Returns a reference to the words buffer.
    ///
    /// The words buffer is for tracing command words.
    /// When writing to the buffer, the content should end with a space.
    #[inline]
    #[must_use]
    pub fn words(&mut self) -> &mut (impl Write + use<>) {
        &mut self.words
    }

    /// Returns a reference to the assignments buffer.
    ///
    /// The assignments buffer is for tracing assignments.
    /// When writing to the buffer, the content should end with a space.
    #[inline]
    #[must_use]
    pub fn assigns(&mut self) -> &mut (impl Write + use<>) {
        &mut self.assigns
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
    pub fn redirs(&mut self) -> &mut (impl Write + use<>) {
        &mut self.redirs
    }

    /// Returns a reference to the here-document contents buffer.
    ///
    /// You should write the contents of here-documents you wrote to the
    /// [redirections buffer](Self::redirs()).
    #[inline]
    #[must_use]
    pub fn here_doc_contents(&mut self) -> &mut (impl Write + use<>) {
        &mut self.here_doc_contents
    }

    /// Clears the buffer contents.
    pub fn clear(&mut self) {
        self.words.clear();
        self.assigns.clear();
        self.redirs.clear();
        self.here_doc_contents.clear();
    }

    /// Returns whether all the buffers are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
            && self.assigns.is_empty()
            && self.redirs.is_empty()
            && self.here_doc_contents.is_empty()
    }

    /// Constructs the final trace to be printed to stderr.
    ///
    /// If all the buffers are empty, the result is empty. Otherwise, the result
    /// is the concatenation of the following:
    ///
    /// - The expansion of `$PS4`
    /// - The concatenation of the `assigns`, `words`, and `redirs` buffers with
    ///   trailing spaces trimmed and a newline appended
    /// - The `here_doc_contents` buffer
    ///
    /// If `$PS4` fails to expand, this function prints an error message and
    /// uses the variable value intact.
    ///
    /// If this function is called while `$PS4` is being expanded inside this
    /// function, the expansion of `$PS4` is skipped and an empty string is
    /// returned. This prevents infinite recursion when `$PS4` contains a
    /// command substitution that causes `XTrace::finish` to be called again.
    pub async fn finish(&self, env: &mut Env) -> String {
        let len = self.assigns.len()
            + self.words.len()
            + self.redirs.len()
            + self.here_doc_contents.len();
        if len == 0 {
            return String::new();
        }

        // Expand $PS4 while preventing infinite recursion
        let Some(mut env) = ExpandingGuard::new(env) else {
            return String::new();
        };
        // TODO Support $YASH_PS4 and $YASH_PS4S
        let ps4 = expand_ps4(&mut env).await;
        drop(env);

        // Construct the final string
        let ps4_len = ps4.len();
        let mut result = ps4;
        result.reserve_exact(len);
        result += &self.assigns;
        result += &self.words;
        result += &self.redirs;
        result.truncate(ps4_len + result[ps4_len..].trim_end_matches(' ').len());
        result.push('\n');
        result += &self.here_doc_contents;
        result
    }
}

/// Convenience function for tracing fields.
///
/// This function writes the field values to the words buffer of the `XTrace`.
pub fn trace_fields(xtrace: Option<&mut XTrace>, fields: &[Field]) {
    if let Some(xtrace) = xtrace {
        for field in fields {
            write!(xtrace.words(), "{} ", quoted(&field.value)).unwrap();
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

/// Convenience function for [finish]ing and
/// [print](yash_env::SharedSystem::print_error)ing an (optional) `XTrace`.
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
    use crate::tests::echo_builtin;
    use futures_util::FutureExt;
    use yash_env::variable::Scope::Global;
    use yash_env_test_helper::in_virtual_system;

    #[test]
    fn tracing_some_fields() {
        let mut xtrace = XTrace::new();
        let fields = Field::dummies(["one", "two", "'three'"]);
        trace_fields(Some(&mut xtrace), &fields);
        assert_eq!(xtrace.words, r#"one two "'three'" "#);
        assert_eq!(xtrace.assigns, "");
        assert_eq!(xtrace.redirs, "");
        assert_eq!(xtrace.here_doc_contents, "");
    }

    fn fixture() -> Env {
        let mut env = Env::new_virtual();
        env.variables
            .get_or_new(PS4, Global)
            .assign("+${X=x}+ ", None)
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
    fn finish_with_assigns() {
        let mut env = fixture();

        let mut xtrace = XTrace::new();
        xtrace.assigns.push_str("VAR=VALUE FOO=BAR ");
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "+x+ VAR=VALUE FOO=BAR\n");

        // $PS4 should have been expanded
        assert_ne!(env.variables.get("X"), None);
    }

    #[test]
    fn finish_with_words() {
        let mut env = fixture();

        let mut xtrace = XTrace::new();
        xtrace.words.push_str("abc '~' foo ");
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
    fn finish_with_assigns_and_words() {
        let mut env = fixture();

        let mut xtrace = XTrace::new();
        xtrace.assigns.push_str("VAR=VALUE ");
        xtrace.words.push_str("echo argument ");
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "+x+ VAR=VALUE echo argument\n");

        let mut xtrace = XTrace::new();
        xtrace.assigns.push(' ');
        xtrace.words.push(' ');
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "+x+ \n");
    }

    #[test]
    fn finish_with_words_and_redirs() {
        let mut env = fixture();

        let mut xtrace = XTrace::new();
        xtrace.words.push_str("echo argument ");
        xtrace.redirs.push_str("2> errors ");
        let result = xtrace.finish(&mut env).now_or_never().unwrap();
        assert_eq!(result, "+x+ echo argument 2> errors\n");

        let mut xtrace = XTrace::new();
        xtrace.words.push(' ');
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

    #[test]
    fn finish_prevents_recursion() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.variables
                .get_or_new(PS4, Global)
                .assign("$(echo recursive) ", None)
                .unwrap();

            let mut xtrace = XTrace::new();
            xtrace.words.push_str("foo bar ");
            let result = xtrace.finish(&mut env).await;
            assert_eq!(result, "recursive foo bar\n");
        })
    }
}
