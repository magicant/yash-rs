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

//! Execution of the for loop

use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::syntax::List;
use yash_syntax::syntax::Word;

/// Executes the for loop.
pub async fn execute(
    _env: &mut Env,
    _name: &Word,
    _values: &Option<Vec<Word>>,
    _body: &List,
) -> Result {
    todo!()
}

#[cfg(test)]
mod tests {
    // use super::*;

    // TODO without_words_without_positional_parameters
    // TODO without_words_with_one_positional_parameters
    // TODO without_words_with_many_positional_parameters
    // TODO with_one_word
    // TODO with_many_words
    // TODO with empty body
}
