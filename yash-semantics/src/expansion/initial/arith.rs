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
use super::super::attr::Origin;
use super::super::phrase::Phrase;
use super::Env;
use super::Error;
use crate::expansion::expand_text;
use yash_arith::eval;
use yash_syntax::source::Location;
use yash_syntax::syntax::Text;

pub async fn expand(text: &Text, _location: &Location, env: &mut Env<'_>) -> Result<Phrase, Error> {
    let (expression, _exit_status) = expand_text(env.inner, text).await?;
    // TODO Handle exit status

    let result = eval(&expression);

    // TODO Test this
    match result {
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

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use super::*;

    #[test]
    fn successful_inner_text_expansion() {
        let text = "0".parse().unwrap();
        let location = Location::dummy("my location");
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let result = expand(&text, &location, &mut env).now_or_never().unwrap();
        let c = AttrChar {
            value: '0',
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        };
        assert_eq!(result, Ok(Phrase::Char(c)));
    }

    // TODO non_zero_exit_status_from_inner_text_expansion
    // TODO error_in_inner_text_expansion
}
