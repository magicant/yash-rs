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

//! Word expansion.
//!
//! The word expansion involves many kinds of operations grouped into the
//! categories described below. The [`expand_words`] function carries out all of
//! them.
//!
//! # Initial expansion
//!
//! The initial expansion converts a word fragment to attributed characters
//! ([`AttrChar`]). It may involve the tilde expansion, parameter expansion,
//! command substitution, and arithmetic expansion performed by the [`Expand`]
//! implementors.
//!
//! Depending on the context, you can configure the expansion to produce either
//! a single field or any number of fields. Using `Vec<AttrChar>` as
//! [`Expansion`] will result in a single field. `Vec<Vec<AttrChar>>` may yield
//! any number of fields.
//!
//! To perform the initial expansion on a text/word fragment that implements
//! `Expand`, you call [`expand`](Expand::expand) on the text/word instance by
//! providing an [`Env`] and [`Output`]. You can create the `Output` from an
//! [`Expansion`] implementor. If successful, the `Expansion` implementor will
//! contain the result.
//!
//! To expand a whole [word](Word), you can instead call a method of
//! [`ExpandToField`]. It produces [`AttrField`]s instead of `AttrChar` vectors.
//!
//! # Multi-field expansion
//!
//! In a context expecting any number of fields, the results of the initial
//! expansion can be subjected to the multi-field expansion. It consists of the
//! brace expansion, field splitting, and pathname expansion, performed in this
//! order. The field splitting includes empty field removal, and the pathname
//! expansion includes the quote removal described below.
//!
//! (TBD: How do users perform multi-field expansion?)
//!
//! # Quote removal
//!
//! The [quote removal](QuoteRemoval) is the last step of the word expansion
//! that removes quotes from the field. It takes an [`AttrField`] input and
//! returns a [`Field`].

mod command_subst;
mod param;
mod quote_removal;
mod text;
mod word;

use async_trait::async_trait;
use std::borrow::Cow;
use std::future::Future;
use std::ops::Deref;
use std::ops::DerefMut;
use std::pin::Pin;
use yash_env::io::Fd;
use yash_env::job::Pid;
use yash_env::job::WaitStatus;
use yash_env::semantics::ExitStatus;
use yash_env::system::Errno;
use yash_env::variable::ContextStack;
use yash_env::variable::ContextType;
use yash_env::variable::ReadOnlyError;
use yash_env::variable::Scope;
use yash_env::variable::Variable;
use yash_env::System;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Location;
use yash_syntax::syntax::Word;

#[doc(no_inline)]
pub use yash_env::semantics::Field;

pub use quote_removal::*;

/// Types of errors that may occur in the word expansion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorCause {
    // TODO Define error cause types
    Dummy(String),
    /// System error while performing a command substitution.
    CommandSubstError(Errno),
    /// Assignment to a read-only variable.
    AssignReadOnly(ReadOnlyError),
}

impl ErrorCause {
    /// Returns an error message describing the error.
    #[must_use]
    pub fn message(&self) -> &str {
        // TODO Localize
        use ErrorCause::*;
        match self {
            Dummy(message) => message,
            CommandSubstError(_) => "error performing the command substitution",
            AssignReadOnly(_) => "cannot assign to read-only variable",
        }
    }

    /// Returns a label for annotating the error location.
    #[must_use]
    pub fn label(&self) -> Cow<'_, str> {
        // TODO Localize
        use ErrorCause::*;
        match self {
            Dummy(_) => "".into(),
            CommandSubstError(e) => e.desc().into(),
            AssignReadOnly(e) => format!("variable `{}` is read-only", e.name).into(),
        }
    }

    /// Returns a location related with the error cause and a message describing
    /// the location.
    #[must_use]
    pub fn related_location(&self) -> Option<(&Location, &'static str)> {
        // TODO Localize
        use ErrorCause::*;
        match self {
            Dummy(_) | CommandSubstError(_) => None,
            AssignReadOnly(e) => Some((
                &e.read_only_location,
                "the variable was made read-only here",
            )),
        }
    }
}

/// Explanation of an expansion failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub cause: ErrorCause,
    pub location: Location,
}

impl<'a> From<&'a Error> for Message<'a> {
    fn from(e: &'a Error) -> Self {
        let mut a = vec![Annotation::new(
            AnnotationType::Error,
            e.cause.label(),
            &e.location,
        )];

        e.location.code.source.complement_annotations(&mut a);

        if let Some((location, label)) = e.cause.related_location() {
            a.push(Annotation::new(
                AnnotationType::Info,
                label.into(),
                location,
            ));
        }

        Message {
            r#type: AnnotationType::Error,
            title: e.cause.message().into(),
            annotations: a,
        }
    }
}

/// Result of word expansion.
///
/// Because fields resulting from the expansion are stored in an [`Output`],
/// the OK value of the result is usually `()`.
pub type Result<T = ()> = std::result::Result<T, Error>;

/// Part of the shell execution environment the word expansion depends on.
#[async_trait(?Send)]
pub trait Env: std::fmt::Debug {
    /// Gets a reference to the variable with the specified name.
    #[must_use]
    fn get_variable(&self, name: &str) -> Option<&Variable>;

    /// Assigns a variable.
    fn assign_variable(
        &mut self,
        scope: Scope,
        name: String,
        value: Variable,
    ) -> std::result::Result<Option<Variable>, ReadOnlyError>;

    /// Returns a reference to the positional parameters.
    fn positional_params(&self) -> &Variable;

    /// Returns a mutable reference to the positional parameters.
    fn positional_params_mut(&mut self) -> &mut Variable;

    /// Gets the exit status of the last command.
    fn exit_status(&self) -> ExitStatus;

    /// Sets the exit status of a command substitution.
    ///
    /// This method is called when a command substitution is performed so that
    /// the environment can remember its exit status.
    ///
    /// This method does _not_ affect the value returned from
    /// [`exit_status`](Self::exit_status).
    fn save_command_subst_exit_status(&mut self, exit_status: ExitStatus);

    /// Gets the process ID of the last executed asynchronous command.
    ///
    /// This function marks the process ID as known by the user so that the
    /// shell does not disown the process too soon. See
    /// [`yash_env::job::JobSet::expand_last_async_pid`] for details.
    fn last_async_pid(&mut self) -> Pid;

    /// Opens a pipe.
    fn pipe(&mut self) -> std::result::Result<(Fd, Fd), Errno>;

    /// Duplicates a file descriptor.
    fn dup2(&mut self, from: Fd, to: Fd) -> std::result::Result<Fd, Errno>;

    /// Closes a file descriptor.
    fn close(&mut self, fd: Fd) -> std::result::Result<(), Errno>;

    /// Reads from the file descriptor.
    async fn read_async(&mut self, fd: Fd, buffer: &mut [u8]) -> std::result::Result<usize, Errno>;

    /// Runs the given function asynchronously in a subshell and returns its
    /// process ID.
    async fn start_subshell<F>(&mut self, f: F) -> std::result::Result<Pid, Errno>
    where
        F: for<'a> FnOnce(
                &'a mut yash_env::Env,
            )
                -> Pin<Box<dyn Future<Output = yash_env::semantics::Result> + 'a>>
            + 'static;

    // Waits for a subshell to terminate.
    async fn wait_for_subshell(&mut self, target: Pid) -> std::result::Result<WaitStatus, Errno>;

    // TODO define Env methods
}
// TODO Should we split Env for the initial expansion and multi-field expansion?

#[async_trait(?Send)]
impl Env for yash_env::Env {
    fn get_variable(&self, name: &str) -> Option<&Variable> {
        self.variables.get(name)
    }
    fn assign_variable(
        &mut self,
        scope: Scope,
        name: String,
        value: Variable,
    ) -> std::result::Result<Option<Variable>, ReadOnlyError> {
        self.variables.assign(scope, name, value)
    }
    fn positional_params(&self) -> &Variable {
        self.variables.positional_params()
    }
    fn positional_params_mut(&mut self) -> &mut Variable {
        self.variables.positional_params_mut()
    }
    fn exit_status(&self) -> ExitStatus {
        self.exit_status
    }
    /// This method is no-op for `yash_env::Env`.
    ///
    /// See also [`ExitStatusAdapter`].
    fn save_command_subst_exit_status(&mut self, _: ExitStatus) {}
    fn last_async_pid(&mut self) -> Pid {
        self.jobs.expand_last_async_pid()
    }
    fn pipe(&mut self) -> std::result::Result<(Fd, Fd), Errno> {
        self.system.pipe()
    }
    fn dup2(&mut self, from: Fd, to: Fd) -> std::result::Result<Fd, Errno> {
        self.system.dup2(from, to)
    }
    fn close(&mut self, fd: Fd) -> std::result::Result<(), Errno> {
        self.system.close(fd)
    }
    async fn read_async(&mut self, fd: Fd, buffer: &mut [u8]) -> std::result::Result<usize, Errno> {
        self.system.read_async(fd, buffer).await
    }
    async fn start_subshell<F>(&mut self, f: F) -> std::result::Result<Pid, Errno>
    where
        F: for<'a> FnOnce(
                &'a mut yash_env::Env,
            )
                -> Pin<Box<dyn Future<Output = yash_env::semantics::Result> + 'a>>
            + 'static,
    {
        self.start_subshell(f).await
    }
    async fn wait_for_subshell(&mut self, target: Pid) -> std::result::Result<WaitStatus, Errno> {
        self.wait_for_subshell(target).await
    }
}

/// Adapter for implementing `save_command_subst_exit_status`.
///
/// Although [`yash_env::Env`] implements [`Env`], its
/// `save_command_subst_exit_status` method is a dummy because `yash_env::Env`
/// does not have any variable for saving such data. You can wrap
/// `yash_env::Env` with `ExitStatusAdapter` to add a working implementation of
/// the method.
#[derive(Debug)]
pub struct ExitStatusAdapter<'a, E> {
    command_subst_exit_status: Option<ExitStatus>,
    env: &'a mut E,
}

impl<'a, E> ExitStatusAdapter<'a, E> {
    /// Creates a new `ExitStatusAdapter` wrapping the specified environment.
    pub fn new(env: &'a mut E) -> Self {
        ExitStatusAdapter {
            command_subst_exit_status: None,
            env,
        }
    }

    /// Returns the exit status that was passed in the last call to
    /// `save_command_subst_exit_status`, or `None` if the method has not been
    /// called at all.
    pub fn last_command_subst_exit_status(&self) -> Option<ExitStatus> {
        self.command_subst_exit_status
    }
}

impl<E> Deref for ExitStatusAdapter<'_, E> {
    type Target = E;
    fn deref(&self) -> &E {
        self.env
    }
}

impl<E> DerefMut for ExitStatusAdapter<'_, E> {
    fn deref_mut(&mut self) -> &mut E {
        self.env
    }
}

#[async_trait(?Send)]
impl<E: Env> Env for ExitStatusAdapter<'_, E> {
    fn get_variable(&self, name: &str) -> Option<&Variable> {
        self.env.get_variable(name)
    }
    fn assign_variable(
        &mut self,
        scope: Scope,
        name: String,
        value: Variable,
    ) -> std::result::Result<Option<Variable>, ReadOnlyError> {
        self.env.assign_variable(scope, name, value)
    }
    fn positional_params(&self) -> &Variable {
        self.env.positional_params()
    }
    fn positional_params_mut(&mut self) -> &mut Variable {
        self.env.positional_params_mut()
    }
    fn exit_status(&self) -> ExitStatus {
        self.env.exit_status()
    }
    fn save_command_subst_exit_status(&mut self, exit_status: ExitStatus) {
        self.command_subst_exit_status = Some(exit_status);
    }
    fn last_async_pid(&mut self) -> Pid {
        self.env.last_async_pid()
    }
    fn pipe(&mut self) -> std::result::Result<(Fd, Fd), Errno> {
        self.env.pipe()
    }
    fn dup2(&mut self, from: Fd, to: Fd) -> std::result::Result<Fd, Errno> {
        self.env.dup2(from, to)
    }
    fn close(&mut self, fd: Fd) -> std::result::Result<(), Errno> {
        self.env.close(fd)
    }
    async fn read_async(&mut self, fd: Fd, buffer: &mut [u8]) -> std::result::Result<usize, Errno> {
        self.env.read_async(fd, buffer).await
    }
    async fn start_subshell<F>(&mut self, f: F) -> std::result::Result<Pid, Errno>
    where
        F: for<'a> FnOnce(
                &'a mut yash_env::Env,
            )
                -> Pin<Box<dyn Future<Output = yash_env::semantics::Result> + 'a>>
            + 'static,
    {
        self.env.start_subshell(f).await
    }
    async fn wait_for_subshell(&mut self, target: Pid) -> std::result::Result<WaitStatus, Errno> {
        self.env.wait_for_subshell(target).await
    }
}

impl<E: ContextStack> ContextStack for ExitStatusAdapter<'_, E> {
    fn push_context(&mut self, context_type: ContextType) {
        self.env.push_context(context_type)
    }
    fn pop_context(&mut self) {
        self.env.pop_context()
    }
}

/// Origin of a character produced in the initial expansion.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Origin {
    /// The character appeared literally in the original word.
    Literal,
    /// The character originates from a tilde expansion or sequencing brace expansion.
    ///
    /// This kind of character is treated literally in the pathname expansion.
    HardExpansion,
    /// The character originates from a parameter expansion, command substitution, or arithmetic expansion.
    ///
    /// This kind of character is subject to field splitting where applicable.
    SoftExpansion,
}

/// Character with attributes describing its origin.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct AttrChar {
    /// Character value.
    pub value: char,
    /// Character origin.
    pub origin: Origin,
    /// Whether this character is quoted by another character.
    pub is_quoted: bool,
    /// Whether this is a quotation character that quotes another character.
    ///
    /// Note that a character can be both quoting and quoted. For example, the
    /// backslash in `"\$"` quotes the dollar and is quoted by the
    /// double-quotes.
    pub is_quoting: bool,
}

/// Result of the initial expansion.
///
/// An `AttrField` is a string of `AttrChar`s with the location of the word.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttrField {
    /// Value of the field.
    pub chars: Vec<AttrChar>,
    /// Location of the word this field resulted from.
    pub origin: Location,
}

/// Interface to accumulate results of the initial expansion.
///
/// `Expansion` is implemented by types that can accumulate [`AttrChar`]s or
/// vectors of them. You construct an [`Output`] using an `Expansion`
/// implementor and then use it to carry out the initial expansion.
pub trait Expansion: std::fmt::Debug {
    /// Appends a character to the current field.
    fn push_char(&mut self, c: AttrChar);

    /// Appends characters to the current field.
    ///
    /// The appended characters share the same `origin`, `is_quoted`, and
    /// `is_quoting` attributes.
    fn push_str(&mut self, s: &str, origin: Origin, is_quoted: bool, is_quoting: bool) {
        for c in s.chars() {
            self.push_char(AttrChar {
                value: c,
                origin,
                is_quoted,
                is_quoting,
            });
        }
    }
}
// TODO impl Expansion::push_fields

/// Produces a single field as a result of the expansion.
impl Expansion for Vec<AttrChar> {
    fn push_char(&mut self, c: AttrChar) {
        self.push(c)
    }
}

/// Produces any number of fields as a result of the expansion.
impl Expansion for Vec<Vec<AttrChar>> {
    fn push_char(&mut self, c: AttrChar) {
        if let Some(field) = self.last_mut() {
            field.push(c);
        } else {
            self.push(vec![c]);
        }
    }
}

/// Wrapper of [`Expansion`] with quotation tracking.
///
/// An output tracks whether the currently expanded part is inside a quotation
/// or not and sets the `is_quoted` flag when results are inserted into it.
#[derive(Debug)]
pub struct Output<'e> {
    /// Fields resulting from the initial expansion.
    inner: &'e mut dyn Expansion,
    /// Whether the currently expanded part is double-quoted.
    is_quoted: bool,
}

impl<'e> Output<'e> {
    /// Creates a new output.
    ///
    /// This function requires a reference to an [`Expansion`] into which the
    /// expansion results are inserted.
    pub fn new(inner: &'e mut dyn Expansion) -> Self {
        let is_quoted = false;
        Output { inner, is_quoted }
    }

    /// Whether the currently expanded part is quoted.
    ///
    /// By default, this function returns `false`. If you [begin a
    /// quotation](Self::begin_quote), it will return `true` until you [end the
    /// quotation](Self::end_quote).
    pub fn is_quoted(&self) -> bool {
        self.is_quoted
    }
}

/// The `Expansion` implementation for `Output` delegates to the `Expansion`
/// implementor contained in the `Output`.
///
/// However, if [`self.is_quoted()`](Output::is_quoted) is `true`, the
/// `is_quoted` flag of resulting `AttrChar`s will also be `true`.
impl Expansion for Output<'_> {
    fn push_char(&mut self, mut c: AttrChar) {
        c.is_quoted |= self.is_quoted;
        self.inner.push_char(c)
    }
    fn push_str(&mut self, s: &str, origin: Origin, is_quoted: bool, is_quoting: bool) {
        self.inner
            .push_str(s, origin, is_quoted | self.is_quoted, is_quoting);
    }
}

/// RAII-style guard for temporarily setting [`Output::is_quoted`] to `true`.
///
/// When the instance of `QuotedOutput` is dropped, `is_quoted` is reset to
/// the previous value.
#[derive(Debug)]
#[must_use = "You must retain QuotedOutput to keep is_quoted true"]
pub struct QuotedOutput<'q, 'e> {
    /// The output
    output: &'q mut Output<'e>,
    /// Previous value of `is_quoted`.
    was_quoted: bool,
}

impl<'q, 'e> Drop for QuotedOutput<'q, 'e> {
    /// Resets `is_quoted` of the output to the previous value.
    fn drop(&mut self) {
        self.output.is_quoted = self.was_quoted;
    }
}

impl<'q, 'e> Deref for QuotedOutput<'q, 'e> {
    type Target = Output<'e>;
    fn deref(&self) -> &Output<'e> {
        self.output
    }
}

impl<'q, 'e> DerefMut for QuotedOutput<'q, 'e> {
    fn deref_mut(&mut self) -> &mut Output<'e> {
        self.output
    }
}

impl<'e> Output<'e> {
    /// Sets `is_quoted` to true.
    ///
    /// This function returns an instance of `QuotedOutput` that borrows `self`.
    /// As an implementor of `Deref` and `DerefMut`, it allows you to access the
    /// original output. When the `QuotedOutput` is dropped or passed to
    /// [`end_quote`](Self::end_quote), `is_quoted` is reset to the previous
    /// value.
    ///
    /// While `is_quoted` is `true`, all characters pushed to the output are
    /// considered quoted; that is, `is_quoted` of [`AttrChar`]s will be `true`.
    pub fn begin_quote(&mut self) -> QuotedOutput<'_, 'e> {
        let was_quoted = std::mem::replace(&mut self.is_quoted, true);
        let output = self;
        QuotedOutput { output, was_quoted }
    }

    /// Resets `is_quoted` to the previous value.
    ///
    /// This function is equivalent to dropping the `QuotedOutput` instance but
    /// allows more descriptive code.
    pub fn end_quote(_: QuotedOutput<'_, 'e>) {}
}

/// Syntactic construct that can be subjected to the word expansion.
///
/// Implementors of this trait expand themselves to an [`Output`].
/// See also [`ExpandToField`].
#[async_trait(?Send)]
pub trait Expand {
    /// Performs the initial expansion.
    ///
    /// The results should be pushed to the output.
    async fn expand<E: Env>(&self, env: &mut E, output: &mut Output<'_>) -> Result;
}

#[async_trait(?Send)]
impl<T: Expand> Expand for [T] {
    /// Expands a slice.
    ///
    /// This function expands each item of the slice in sequence.
    async fn expand<E: Env>(&self, env: &mut E, output: &mut Output<'_>) -> Result {
        for item in self {
            item.expand(env, output).await?;
        }
        Ok(())
    }
}

/// Syntactic construct that can be expanded to an [`AttrField`].
///
/// Implementors of this trait expand themselves directly to an `AttrField` or
/// a vector of `AttrField`s. See also [`Expand`].
#[async_trait(?Send)]
pub trait ExpandToField {
    /// Performs the initial expansion on `self`, producing a single field.
    ///
    /// This is usually used in contexts where field splitting will not be
    /// performed on the result.
    async fn expand_to_field<E: Env>(&self, env: &mut E) -> Result<AttrField>;

    /// Performs the initial expansion on `self`, producing any number of
    /// fields.
    ///
    /// This is usually used in contexts where field splitting will be performed
    /// on the result.
    ///
    /// This function inserts the results into `fields`.
    async fn expand_to_fields<E: Env, F: Extend<AttrField>>(
        &self,
        env: &mut E,
        fields: &mut F,
    ) -> Result;
}

/// Expands a word to a field.
///
/// This function performs the initial expansion and quote removal.
///
/// To expand multiple words to multiple fields, use [`expand_words`].
pub async fn expand_word<E: Env>(env: &mut E, word: &Word) -> Result<Field> {
    // TODO Optimize by taking advantage of MaybeLiteral
    let field = word.expand_to_field(env).await?;
    Ok(field.do_quote_removal())
}

/// Expands words to fields.
///
/// This function performs all of the initial expansion, multi-field expansion,
/// and quote removal.
///
/// To expand a single word to a single field, use [`expand_word`].
pub async fn expand_words<'a, E, I>(env: &mut E, words: I) -> Result<Vec<Field>>
where
    E: Env,
    I: IntoIterator<Item = &'a Word>,
{
    // TODO Optimize by taking advantage of MaybeLiteral

    let mut fields = Vec::new();
    for word in words {
        word.expand_to_fields(env, &mut fields).await?;
    }
    // TODO multi-field expansion
    Ok(fields
        .into_iter()
        .map(QuoteRemoval::do_quote_removal)
        .collect())
}

/// Expands an assignment value.
///
/// This function expands a [`yash_syntax::syntax::Value`] to a
/// [`yash_env::variable::Value`]. A scalar value is expanded by [`expand_word`]
/// and an array by [`expand_words`].
pub async fn expand_value<E: Env>(
    env: &mut E,
    value: &yash_syntax::syntax::Value,
) -> Result<yash_env::variable::Value> {
    match value {
        yash_syntax::syntax::Scalar(word) => {
            let field = expand_word(env, word).await?;
            Ok(yash_env::variable::Scalar(field.value))
        }
        yash_syntax::syntax::Array(words) => {
            let fields = expand_words(env, words).await?;
            let fields = fields.into_iter().map(|f| f.value).collect();
            Ok(yash_env::variable::Array(fields))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures_executor::block_on;
    use std::num::NonZeroU64;
    use std::rc::Rc;
    use yash_env::variable::Value;
    use yash_syntax::source::Code;
    use yash_syntax::source::Source;

    #[derive(Debug)]
    pub(crate) struct NullEnv;

    #[async_trait(?Send)]
    impl Env for NullEnv {
        fn get_variable(&self, _: &str) -> Option<&Variable> {
            unimplemented!("NullEnv's method must not be called")
        }
        fn assign_variable(
            &mut self,
            _: Scope,
            _: String,
            _: Variable,
        ) -> std::result::Result<Option<Variable>, ReadOnlyError> {
            unimplemented!("NullEnv's method must not be called")
        }
        fn positional_params(&self) -> &Variable {
            unimplemented!("NullEnv's method must not be called")
        }
        fn positional_params_mut(&mut self) -> &mut Variable {
            unimplemented!("NullEnv's method must not be called")
        }
        fn exit_status(&self) -> yash_env::semantics::ExitStatus {
            unimplemented!("NullEnv's method must not be called")
        }
        fn save_command_subst_exit_status(&mut self, _: ExitStatus) {
            unimplemented!("NullEnv's method must not be called")
        }
        fn last_async_pid(&mut self) -> yash_env::job::Pid {
            unimplemented!("NullEnv's method must not be called")
        }
        fn pipe(&mut self) -> std::result::Result<(Fd, Fd), Errno> {
            unimplemented!("NullEnv's method must not be called")
        }
        fn dup2(&mut self, _from: Fd, _to: Fd) -> std::result::Result<Fd, Errno> {
            unimplemented!("NullEnv's method must not be called")
        }
        fn close(&mut self, _fd: Fd) -> std::result::Result<(), Errno> {
            unimplemented!("NullEnv's method must not be called")
        }
        async fn read_async(
            &mut self,
            _fd: Fd,
            _buffer: &mut [u8],
        ) -> std::result::Result<usize, Errno> {
            unimplemented!("NullEnv's method must not be called")
        }
        async fn start_subshell<F>(&mut self, _f: F) -> std::result::Result<Pid, Errno>
        where
            F: for<'a> FnOnce(
                    &'a mut yash_env::Env,
                )
                    -> Pin<Box<dyn Future<Output = yash_env::semantics::Result> + 'a>>
                + 'static,
        {
            unimplemented!("NullEnv's method must not be called")
        }
        async fn wait_for_subshell(
            &mut self,
            _target: Pid,
        ) -> std::result::Result<WaitStatus, Errno> {
            unimplemented!("NullEnv's method must not be called")
        }
    }

    #[test]
    fn from_error_for_message() {
        let code = Rc::new(Code {
            value: "".to_string().into(),
            start_line_number: NonZeroU64::new(1).unwrap(),
            source: Source::Unknown,
        });
        let location = Location { code, index: 0 };
        let new_value = Variable {
            value: Value::Scalar("value".into()),
            last_assigned_location: Some(Location::dummy("assigned")),
            is_exported: false,
            read_only_location: None,
        };
        let error = Error {
            cause: ErrorCause::AssignReadOnly(ReadOnlyError {
                name: "var".into(),
                read_only_location: Location::dummy("ROL"),
                new_value,
            }),
            location,
        };
        let message = Message::from(&error);
        assert_eq!(message.r#type, AnnotationType::Error);
        assert_eq!(message.title, "cannot assign to read-only variable");
        assert_eq!(message.annotations.len(), 2);
        assert_eq!(message.annotations[0].r#type, AnnotationType::Error);
        assert_eq!(message.annotations[0].label, "variable `var` is read-only");
        assert_eq!(message.annotations[0].location, &error.location);
        assert_eq!(message.annotations[1].r#type, AnnotationType::Info);
        assert_eq!(
            message.annotations[1].label,
            "the variable was made read-only here"
        );
        assert_eq!(message.annotations[1].location, &Location::dummy("ROL"));
    }

    #[test]
    fn expansion_push_str() {
        let a = AttrChar {
            value: 'a',
            origin: Origin::SoftExpansion,
            is_quoted: true,
            is_quoting: false,
        };
        let to = AttrChar {
            value: '-',
            origin: Origin::SoftExpansion,
            is_quoted: true,
            is_quoting: false,
        };
        let z = AttrChar {
            value: 'z',
            origin: Origin::SoftExpansion,
            is_quoted: true,
            is_quoting: false,
        };

        let mut field = Vec::<AttrChar>::default();
        field.push_str("a-z", Origin::SoftExpansion, true, false);
        assert_eq!(field, [a, to, z]);
    }

    #[test]
    fn attr_field_push_char() {
        let c = AttrChar {
            value: 'X',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: true,
        };
        let d = AttrChar {
            value: 'Y',
            origin: Origin::SoftExpansion,
            is_quoted: true,
            is_quoting: false,
        };
        let mut field = Vec::<AttrChar>::default();
        field.push_char(c);
        assert_eq!(field, [c]);
        field.push_char(d);
        assert_eq!(field, [c, d]);
    }

    #[test]
    fn vec_attr_field_push_char() {
        let c = AttrChar {
            value: 'X',
            origin: Origin::Literal,
            is_quoted: true,
            is_quoting: false,
        };
        let d = AttrChar {
            value: 'Y',
            origin: Origin::HardExpansion,
            is_quoted: false,
            is_quoting: true,
        };
        let mut fields = Vec::<Vec<AttrChar>>::default();
        fields.push_char(c);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0], [c]);
        fields.push_char(d);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0], [c, d]);
    }

    // TODO Test Vec<Vec<AttrChar>>::push_char with multiple existing fields

    #[allow(clippy::bool_assert_comparison)]
    #[test]
    fn quoted_output() {
        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        assert_eq!(output.is_quoted(), false);
        {
            let mut output = output.begin_quote();
            assert_eq!(output.is_quoted(), true);
            {
                let output = output.begin_quote();
                assert_eq!(output.is_quoted(), true);
                Output::end_quote(output);
            }
            assert_eq!(output.is_quoted(), true);
            Output::end_quote(output);
        }
        assert_eq!(output.is_quoted(), false);
    }

    #[test]
    fn output_put_char_quoted() {
        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        let not_quoted = AttrChar {
            value: 'X',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let quoted = AttrChar {
            is_quoted: true,
            ..not_quoted
        };
        output.push_char(not_quoted);
        output.push_char(quoted);
        let mut output = output.begin_quote();
        output.push_char(not_quoted);
        output.push_char(quoted);
        Output::end_quote(output);
        assert_eq!(field, [not_quoted, quoted, quoted, quoted]);
    }

    #[test]
    fn output_put_str_quoted() {
        let mut field = Vec::<AttrChar>::default();
        let mut output = Output::new(&mut field);
        output.push_str("X", Origin::Literal, false, false);
        output.push_str("X", Origin::Literal, true, false);
        let mut output = output.begin_quote();
        output.push_str("X", Origin::Literal, false, false);
        output.push_str("X", Origin::Literal, true, false);
        Output::end_quote(output);

        let not_quoted = AttrChar {
            value: 'X',
            origin: Origin::Literal,
            is_quoted: false,
            is_quoting: false,
        };
        let quoted = AttrChar {
            is_quoted: true,
            ..not_quoted
        };
        assert_eq!(field, [not_quoted, quoted, quoted, quoted]);
    }

    #[test]
    fn expand_value_scalar() {
        let v = yash_syntax::syntax::Scalar(r"1\\".parse().unwrap());
        let result = block_on(expand_value(&mut NullEnv, &v)).unwrap();
        let content = assert_matches!(result, yash_env::variable::Scalar(content) => content);
        assert_eq!(content, r"1\");
    }

    #[test]
    fn expand_value_array() {
        let v = yash_syntax::syntax::Array(vec!["''".parse().unwrap(), r"2\\".parse().unwrap()]);
        let result = block_on(expand_value(&mut NullEnv, &v)).unwrap();
        let content = assert_matches!(result, yash_env::variable::Array(content) => content);
        assert_eq!(content, ["".to_string(), r"2\".to_string()]);
    }
}
