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

//! Getopts built-in
//!
//! This module implements the [`getopts` built-in], which is used to parse
//! options in shell scripts.
//!
//! [`getopts` built-in]: https://magicant.github.io/yash-rs/builtins/getopts.html
//!
//! # Implementation notes
//!
//! This implementation uses the `any` field in the [`Env`] to check if the
//! built-in is invoked with the same arguments and `$OPTIND` as the previous
//! invocation.

use crate::common::report::{report_error, report_simple_error};
use crate::common::syntax::Mode;
use crate::common::syntax::parse_arguments;
use either::Either::{Left, Right};
use std::num::NonZeroUsize;
use yash_env::Env;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::variable::OPTIND;

pub mod model;
pub mod report;
pub mod verify;

/// Computes the `arg_index` and `char_index` parameters of the
/// [`next`](self::model::next) function from the `$OPTIND` value.
fn indexes_from_optind(optind: &str) -> (NonZeroUsize, NonZeroUsize) {
    const DEFAULT: NonZeroUsize = NonZeroUsize::MIN;
    let mut iter = optind.split(':');
    let arg_index = iter.next().and_then(|s| s.parse().ok()).unwrap_or(DEFAULT);
    let char_index = iter.next().and_then(|s| s.parse().ok()).unwrap_or(DEFAULT);
    (arg_index, char_index)
}

/// Computes the `$OPTIND` value from the `arg_index` and `char_index`.
fn indexes_to_optind(arg_index: NonZeroUsize, char_index: NonZeroUsize) -> String {
    if char_index == NonZeroUsize::MIN {
        format!("{arg_index}")
    } else {
        format!("{arg_index}:{char_index}")
    }
}

/// Entry point of the getopts built-in
pub async fn main(env: &mut Env, args: Vec<Field>) -> crate::Result {
    // Parse arguments
    let operands = match parse_arguments(&[], Mode::with_env(env), args) {
        Ok((_, operands)) => operands,
        Err(error) => return report_error(env, &error).await,
    };
    if operands.len() < 2 {
        let message = format!(
            "insufficient operands (2 or more required, {} given)",
            operands.len()
        );
        return report_simple_error(env, &message).await;
    }

    let spec = model::OptionSpec::from(&operands[0].value);

    // Get the arguments to parse
    let (args, arg_origin) = if operands.len() > 2 {
        let iter = operands[2..].iter().map(|f| f.value.as_str());
        (Left(iter), verify::Origin::DirectArgs)
    } else {
        let params = &env.variables.positional_params().values;
        let iter = params.iter().map(|v| v.as_str());
        (Right(iter), verify::Origin::PositionalParams)
    };

    // Get the `$OPTIND` value
    let optind = env.variables.get_scalar(OPTIND).unwrap_or_default();
    let (arg_index, char_index) = indexes_from_optind(optind);

    // Verify the state
    let current = verify::GetoptsStateRef {
        args: args.clone(),
        origin: arg_origin,
        optind,
    };
    if let Some(state) = env.any.get_mut::<verify::GetoptsState>() {
        match current.verify(&*state) {
            Ok(None) => {}
            Ok(Some(current)) => *state = current.into_state(),
            Err(e) => return report_simple_error(env, &e.to_string()).await,
        }
    } else {
        if optind != "1" {
            let message = format!("unexpected $OPTIND value `{optind}`");
            return report_simple_error(env, &message).await;
        }
        env.any.insert(Box::new(current.into_state()));
    }

    // Parse the next option
    let result = model::next(args, spec, arg_index, char_index);

    // Report the result
    let colon = spec.as_raw().starts_with(':');
    let option_var = { operands }.swap_remove(1);
    match result.report(env, colon, option_var) {
        Ok(Some(message)) => {
            env.system.print_error(&message).await;
            ExitStatus::SUCCESS.into()
        }
        Ok(None) => ExitStatus::FAILURE.into(),
        Err(error) => report_error(env, &error).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn non_zero(i: usize) -> NonZeroUsize {
        NonZeroUsize::new(i).unwrap()
    }

    #[test]
    fn indexes_from_optind_with_normal_values() {
        assert_eq!(indexes_from_optind("1"), (non_zero(1), non_zero(1)));
        assert_eq!(indexes_from_optind("2"), (non_zero(2), non_zero(1)));
        assert_eq!(indexes_from_optind("3"), (non_zero(3), non_zero(1)));
        assert_eq!(indexes_from_optind("1:2"), (non_zero(1), non_zero(2)));
        assert_eq!(indexes_from_optind("1:3"), (non_zero(1), non_zero(3)));
        assert_eq!(indexes_from_optind("2:4"), (non_zero(2), non_zero(4)));
    }

    #[test]
    fn indexes_from_optind_with_abnormal_values() {
        assert_eq!(indexes_from_optind(""), (non_zero(1), non_zero(1)));
        assert_eq!(indexes_from_optind("0"), (non_zero(1), non_zero(1)));
        assert_eq!(indexes_from_optind("2:0"), (non_zero(2), non_zero(1)));
        assert_eq!(indexes_from_optind("2:3:4"), (non_zero(2), non_zero(3)));
    }

    #[test]
    fn indexes_to_optind_with_min_char_index() {
        assert_eq!(indexes_to_optind(non_zero(1), non_zero(1)), "1");
        assert_eq!(indexes_to_optind(non_zero(2), non_zero(1)), "2");
        assert_eq!(indexes_to_optind(non_zero(4), non_zero(1)), "4");
        assert_eq!(indexes_to_optind(non_zero(10), non_zero(1)), "10");
    }

    #[test]
    fn indexes_to_optind_with_large_char_index() {
        assert_eq!(indexes_to_optind(non_zero(1), non_zero(2)), "1:2");
        assert_eq!(indexes_to_optind(non_zero(1), non_zero(3)), "1:3");
        assert_eq!(indexes_to_optind(non_zero(2), non_zero(2)), "2:2");
        assert_eq!(indexes_to_optind(non_zero(2), non_zero(4)), "2:4");
        assert_eq!(indexes_to_optind(non_zero(10), non_zero(13)), "10:13");
    }
}
