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

//! Assigning the input to variables

use yash_env::semantics::Field;
use yash_env::Env;
use yash_semantics::expansion::attr::AttrChar;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::pretty::MessageBase as _;

pub use crate::typeset::AssignReadOnlyError as Error;

/// Converts errors to a message.
///
/// Returns `None` if `errors` is empty.
pub fn to_message(errors: &[Error]) -> Option<Message> {
    let mut message = Message::from(errors.first()?);
    let other_errors = errors[1..].iter().map(Error::main_annotation);
    message.annotations.extend(other_errors);
    Some(message)
}

/// Assigns the text to variables.
///
/// This function performs field splitting on the text and assigns the resulting
/// fields to the variables. When there are more fields than variables, the last
/// variable receives all remaining fields, including the intermediate (but not
/// trailing) field separators. When there are fewer fields than variables, the
/// remaining variables are set to empty strings.
///
/// The return value is a vector of errors that occurred while assigning the
/// variables. The vector is empty if no error occurred.
pub fn assign(
    env: &mut Env,
    text: &[AttrChar],
    variables: Vec<Field>,
    last_variable: Field,
) -> Vec<Error> {
    _ = env;
    _ = text;
    _ = variables;
    _ = last_variable;
    todo!()
}
