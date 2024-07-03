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

use yash_env::Env;
use yash_semantics::expansion::expand_text;
use yash_syntax::syntax::{
    Text,
    TextUnit::{self, Literal},
};

/// Expands the prompt string according to the POSIX standard.
///
/// The prompt string is parsed as a [`Text`].
///
/// If `excl` is true, occurrences of literal `!` in the text are expanded to
/// the history number of the current command and `!!` is expanded to `!`.
/// (TODO: Currently, the history feature is not implemented, so `!` simply
/// expands to `0`.)
///
/// The [expansion](expand_text) of the text is returned.
pub async fn expand_posix<'a>(env: &mut Env, prompt: &'a str, excl: bool) -> String {
    let mut text = prompt.parse().unwrap_or_else(|_| {
        // If expansions in the prompt string cannot be parsed, treat all
        // characters as literals.
        Text::from_literal_chars(prompt.chars())
    });

    if excl {
        replace_exclamation_marks(&mut text.0);
    }

    match expand_text(env, &text).await {
        Ok((expansion, _exit_status)) => expansion,
        Err(_) => text.to_string(),
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
    use futures_util::FutureExt as _;
    use yash_env::option::{Off, Unset};
    use yash_env::variable::Scope::Global;

    #[test]
    fn plain_prompt() {
        // If the prompt string contains no special characters, it should be
        // returned as is.
        let mut env = Env::new_virtual();
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
        let mut env = Env::new_virtual();
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
        let mut env = Env::new_virtual();
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
        let mut env = Env::new_virtual();
        env.options.set(Unset, Off);
        let prompt = "my $FOO > ";
        let result = expand_posix(&mut env, prompt, false)
            .now_or_never()
            .unwrap();
        assert_eq!(result, "my $FOO > ");
    }

    #[test]
    fn single_exclamation_mark_expands_to_history_number() {
        let mut env = Env::new_virtual();
        let prompt = "my prompt ! > ";
        let result = expand_posix(&mut env, prompt, true).now_or_never().unwrap();
        assert_eq!(result, "my prompt 0 > ");
    }

    #[test]
    fn double_exclamation_mark_expands_to_single_exclamation_mark() {
        let mut env = Env::new_virtual();
        let prompt = "my prompt !! > ";
        let result = expand_posix(&mut env, prompt, true).now_or_never().unwrap();
        assert_eq!(result, "my prompt ! > ");
    }

    #[test]
    fn trailing_consecutive_exclamation_marks() {
        // The first two exclamation marks are expanded to a single exclamation
        // mark, and the third exclamation mark to the history number.
        let mut env = Env::new_virtual();
        let prompt = "my prompt > !!!";
        let result = expand_posix(&mut env, prompt, true).now_or_never().unwrap();
        assert_eq!(result, "my prompt > !0");
    }

    #[test]
    fn no_excl_option() {
        // If the excl option is false, exclamation marks are treated as
        // literals.
        let mut env = Env::new_virtual();
        let prompt = "my prompt ! > !!!";
        let result = expand_posix(&mut env, prompt, false)
            .now_or_never()
            .unwrap();
        assert_eq!(result, "my prompt ! > !!!");
    }
}
