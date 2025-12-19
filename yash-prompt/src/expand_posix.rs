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

//! Prompt string expansion (POSIX-compatible)

use futures_util::FutureExt as _;
use std::pin::Pin;
use yash_env::Env;
use yash_env::semantics::ExitStatus;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::syntax::Text;
use yash_syntax::syntax::TextUnit::{self, Literal};

type PinFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
type ExpandTextResult = Option<(String, Option<ExitStatus>)>;

/// Wrapper for a text expansion function
///
/// This struct declares a function type for expanding a [`Text`].
/// An implementation of this function type should be provided and stored in the
/// environment's [`any`](Env::any) storage to be used for prompt string
/// expansion ([`expand_posix`]).
///
/// The function takes a mutable reference to the environment and a reference
/// to the [`Text`] to be expanded, and returns a future that resolves to an
/// optional tuple containing the expanded string and an optional exit status.
/// The exit status indicates that of the last command substitution performed
/// during the expansion, if any. If no command substitution was performed,
/// the exit status is `None`. If the expansion fails, the entire result is
/// `None`. The function should not modify the current exit status
/// ([`Env::exit_status`]) of the environment.
///
/// The most standard implementation of this function type is provided in the
/// [`yash-semantics` crate](https://crates.io/crates/yash-semantics):
///
/// ```
/// # use yash_env::{Env, VirtualSystem};
/// # use yash_prompt::ExpandText;
/// let mut env = Env::new_virtual();
/// env.any.insert(Box::new(ExpandText::<VirtualSystem>(|env, text| {
///     Box::pin(async move { yash_semantics::expansion::expand_text(env, text).await.ok() })
/// })));
/// ```
pub struct ExpandText<S>(
    pub for<'a> fn(&'a mut Env<S>, &'a Text) -> PinFuture<'a, ExpandTextResult>,
);

// Not derived automatically because S may not implement Clone, Copy, or Debug.
impl<S> Clone for ExpandText<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for ExpandText<S> {}

impl<S> std::fmt::Debug for ExpandText<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ExpandText").field(&self.0).finish()
    }
}

/// Expands the prompt string according to the POSIX standard.
///
/// The prompt string is parsed as a [`Text`].
///
/// If `excl` is true, occurrences of literal `!` in the text are expanded to
/// the history number of the current command and `!!` is expanded to `!`.
/// (TODO: Currently, the history feature is not implemented, so `!` simply
/// expands to `0`.)
///
/// The expansion of the text is returned.
///
/// # Dependency
///
/// This function relies on the [`ExpandText`] function stored in the
/// environment's [`any`](Env::any) storage to perform the actual text
/// expansion. If no such function is found, this function will **panic**.
///
/// # Portability
///
/// The current implementation does not recognize any backslash escapes in the
/// text since the POSIX standard does not specify any. However, other shell
/// implementations support backslash escapes in the prompt string. This
/// discrepancy may be reconsidered in the future.
pub async fn expand_posix<S>(env: &mut Env<S>, prompt: &str, excl: bool) -> String
where
    S: 'static,
{
    let mut lexer = Lexer::with_code(prompt);
    let text_result = lexer.text(|_| false, |_| false).now_or_never().unwrap();

    let mut text = text_result.unwrap_or_else(|_| {
        // If expansions in the prompt string cannot be parsed, treat all
        // characters as literals.
        Text::from_literal_chars(prompt.chars())
    });

    if excl {
        replace_exclamation_marks(&mut text.0);
    }

    let ExpandText(expand_text) = *env
        .any
        .get()
        .expect("`yash-prompt::expand_posix` requires `ExpandText` in `Env::any`");
    match expand_text(env, &text).await {
        Some((expansion, _exit_status)) => expansion,
        None => text.to_string(),
    }
}

/// Replaces all occurrences of `!` in the text with the history number of the
/// current command and `!!` with `!`.
fn replace_exclamation_marks(text: &mut Vec<TextUnit>) {
    let mut i = 0;
    while i < text.len() {
        if text[i] == Literal('!') {
            if text.get(i + 1) == Some(&Literal('!')) {
                text.remove(i + 1);
            } else {
                text[i] = Literal('0');
            }
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::env_with_expand_text;
    use yash_env::option::{Off, Unset};
    use yash_env::variable::Scope::Global;

    #[test]
    fn plain_prompt() {
        // If the prompt string contains no special characters, it should be
        // returned as is.
        let mut env = env_with_expand_text();
        let prompt = "my prompt > ";
        let result = expand_posix(&mut env, prompt, false)
            .now_or_never()
            .unwrap();
        assert_eq!(result, prompt);
    }

    #[test]
    fn parameter_expansion() {
        // If the prompt string contains a parameter expansion, it should be
        // expanded.
        let mut env = env_with_expand_text();
        env.variables
            .get_or_new("FOO", Global)
            .assign("bar", None)
            .unwrap();
        let prompt = "my $FOO > ";
        let result = expand_posix(&mut env, prompt, false)
            .now_or_never()
            .unwrap();
        assert_eq!(result, "my bar > ");
    }

    #[test]
    fn malformed_parameter_expansion() {
        // If a parameter expansion is malformed, we treat it as a literal.
        let mut env = env_with_expand_text();
        env.variables
            .get_or_new("FOO", Global)
            .assign("bar", None)
            .unwrap();
        let prompt = "my ${FOO > ";
        let result = expand_posix(&mut env, prompt, false)
            .now_or_never()
            .unwrap();
        assert_eq!(result, "my ${FOO > ");
    }

    #[test]
    fn failed_parameter_expansion() {
        // If a parameter expansion fails, we treat it a literal.
        let mut env = env_with_expand_text();
        env.options.set(Unset, Off);
        let prompt = "my $FOO > ";
        let result = expand_posix(&mut env, prompt, false)
            .now_or_never()
            .unwrap();
        assert_eq!(result, "my $FOO > ");
    }

    #[test]
    fn single_exclamation_mark_expands_to_history_number() {
        let mut env = env_with_expand_text();
        let prompt = "my prompt ! > ";
        let result = expand_posix(&mut env, prompt, true).now_or_never().unwrap();
        assert_eq!(result, "my prompt 0 > ");
    }

    #[test]
    fn double_exclamation_mark_expands_to_single_exclamation_mark() {
        let mut env = env_with_expand_text();
        let prompt = "my prompt !! > ";
        let result = expand_posix(&mut env, prompt, true).now_or_never().unwrap();
        assert_eq!(result, "my prompt ! > ");
    }

    #[test]
    fn trailing_consecutive_exclamation_marks() {
        // The first two exclamation marks are expanded to a single exclamation
        // mark, and the third exclamation mark to the history number.
        let mut env = env_with_expand_text();
        let prompt = "my prompt > !!!";
        let result = expand_posix(&mut env, prompt, true).now_or_never().unwrap();
        assert_eq!(result, "my prompt > !0");
    }

    #[test]
    fn no_excl_option() {
        // If the excl option is false, exclamation marks are treated as
        // literals.
        let mut env = env_with_expand_text();
        let prompt = "my prompt ! > !!!";
        let result = expand_posix(&mut env, prompt, false)
            .now_or_never()
            .unwrap();
        assert_eq!(result, "my prompt ! > !!!");
    }
}
