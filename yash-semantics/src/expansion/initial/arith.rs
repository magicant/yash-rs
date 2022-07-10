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

//! Arithmetic expansion

use super::super::attr::AttrChar;
use super::super::attr::AttrField;
use super::super::attr::Origin;
use super::super::phrase::Phrase;
use super::Env;
use super::Error;
use super::Expand;
use super::QuickExpand::{Interim, Ready};
use yash_arith::eval;
use yash_syntax::source::Location;
use yash_syntax::syntax::Text;

pub async fn expand(text: &Text, location: &Location, env: &mut Env<'_>) -> Result<Phrase, Error> {
    // TODO Extract expand_text function
    let phrase = match text.quick_expand(env) {
        Ready(result) => result?,
        Interim(interim) => text.async_expand(env, interim).await?,
    };
    let chars = phrase.ifs_join(&env.inner.variables);
    let origin = location.clone();
    let field = AttrField { chars, origin };
    let expression = field.remove_quotes_and_strip();
    // TODO Test this
    match eval(&expression.value) {
        Ok(value) => {
            let value = value.to_string();
            let chars = value
                .chars()
                .map(|c| AttrChar {
                    value: c,
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                })
                .collect();
            Ok(Phrase::Field(chars))
        }
        Err(error) => todo!("handle error: {}", error),
    }
}
