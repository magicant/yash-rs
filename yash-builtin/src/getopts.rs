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
//! The **`getopts`** built-in is used to parse options in shell scripts.
//!
//! # Synopsis
//!
//! ```sh
//! getopts option_spec variable_name [argument…]
//! ```
//!
//! # Description
//!
//! The getopts built-in parses single-character options in the specified
//! arguments according to the specified option specification, and assigns the
//! parsed options to the specified variable. This built-in is meant to be used
//! in the condition of a `while` loop to iterate over the options in the
//! arguments. Every invocation of the built-in parses the next option in the
//! arguments. The built-in returns a non-zero exit status when there are no more
//! options to parse.
//!
//! The shell uses the `$OPTIND` variable to keep track of the current position
//! in the arguments. When the shell starts, the variable is initialized to `1`.
//! The built-in updates the variable to the index of the next argument to parse.
//! When all arguments are parsed, the built-in sets the variable to the index of
//! the first operand after the options, or to the number of arguments plus one
//! if there are no operands.
//!
//! When the built-in parses an option, it sets the specified variable to the
//! option name. If the option takes an argument, the built-in also sets the
//! `$OPTARG` variable to the argument.
//!
//! If the built-in encounters an option that is not listed in the option
//! specification, the specified variable is set to `?`. Additionally, if the
//! option specification starts with a colon (`:`), the built-in sets the
//! `$OPTARG` variable to the encountered option character. Otherwise, the
//! built-in unsets the `$OPTARG` variable and prints an error message to the
//! standard error describing the invalid option.
//!
//! If the built-in encounters an option that takes an argument but the argument
//! is missing, the error handling is similar to the case of an invalid option.
//! If the option specification starts with a colon, the built-in sets the
//! the specified variable to `:` (not `?`) and sets the `$OPTARG` variable to
//! the option character. Otherwise, the built-in sets the specified variable to
//! `?`, unsets the `$OPTARG` variable, and prints an error message to the
//! standard error describing the missing argument.
//!
//! When there are no more options to parse, the specified variable is set to
//! `?` and the `$OPTARG` variable is unset.
//!
//! In repeated invocations of the built-in, you must pass the same arguments to
//! the built-in. You must not modify the `$OPTIND` variable between
//! invocations, either. Otherwise, the built-in may not be able to parse the
//! options correctly.
//!
//! To start parsing a new set of options, you must reset the `$OPTIND` variable
//! to `1` before invoking the built-in.
//!
//! # Options
//!
//! None.
//!
//! # Operands
//!
//! The first operand (***option_spec***) is the option specification. It is a
//! string that contains the option characters the built-in parses. If a
//! character is followed by a colon (`:`), the option takes an argument. If the
//! option specification starts with a colon, the built-in does not print an
//! error message when it encounters an invalid option or an option that is
//! missing an argument.
//!
//! The second operand (***variable_name***) is the name of the variable to
//! which the built-in assigns the parsed option. In case of an invalid option
//! or an option that is missing an argument, the built-in assigns `?` or `:` to
//! the variable (see above).
//!
//! The remaining operands (***argument…***) are the arguments to parse.
//! If there are no operands, the built-in parses the positional parameters.
//!
//! # Errors
//!
//! The built-in may print an error message to the standard error when it
//! encounters an invalid option or an option that is missing an argument (see
//! the description above). However, this is not considered an error of the
//! built-in itself.
//!
//! The following conditions are considered errors of the built-in:
//!
//! - The built-in is invoked with less than two operands.
//! - `$OPTIND`, `$OPTARG`, or the specified variable is read-only.
//! - The built-in is re-invoked with different arguments or a different value
//!   of `$OPTIND` than the previous invocation (except when `$OPTIND` is reset
//!   to `1`).
//! - The value of `$OPTIND` is not `1` on the first invocation.
//!
//! # Exit status
//!
//! The built-in returns an exit status of zero if it parses an option,
//! regardless of whether the option is valid or not. When there are no more
//! options to parse, the built-in returns a non-zero exit status.
//!
//! The exit status is non-zero on error.
//!
//! # Examples
//!
//! In the following example, the getopts built-in parses three kinds of options
//! (`-a`, `-b`, and `-c`), of which only `-b` takes an argument. In case of an
//! error, the built-in prints an error message to the standard error, so the
//! script just exits with a non-zero exit status when `$opt` is set to `?`.
//!
//! ```sh
//! a=false c=false
//! while getopts ab:c opt; do
//!     case "$opt" in
//!         a) a=true ;;
//!         b) b="$OPTARG" ;;
//!         c) c=true ;;
//!         '?') exit 1 ;;
//!     esac
//! done
//! shift "$((OPTIND - 1))"
//!
//! if "$a"; then printf 'The -a option was specified\n'; fi
//! if [ "${b+set}" ]; then printf 'The -b option was specified with argument %s\n' "$b"; fi
//! if "$c"; then printf 'The -c option was specified\n'; fi
//! printf 'The remaining operands are: %s\n' "$*"
//! ```
//!
//! If you prefer to print an error message yourself, put a colon at the
//! beginning of the option specification like this:
//!
//! ```sh
//! while getopts :ab:c opt; do
//!     case "$opt" in
//!         a) a=true ;;
//!         b) b="$OPTARG" ;;
//!         c) c=true ;;
//!         '?') printf 'Invalid option: -%s\n' "$OPTARG" >&2; exit 1 ;;
//!         :) printf 'Option -%s requires an argument\n' "$OPTARG" >&2; exit 1 ;;
//!     esac
//! done
//! ```
//!
//! # Portability
//!
//! The getopts built-in is specified by POSIX. Only ASCII alphanumeric
//! characters are allowed for option names, though this implementation allows
//! any characters but `:`.
//!
//! Although POSIX requires the built-in to support the Utility Syntax
//! Guidelines 3 to 10, some implementations do not support the `--` separator
//! placed before operands to the built-in itself, that is, between the built-in
//! name `getopts` and the first operand *option_spec*.
//!
//! The value of the `$OPTIND` variable is not portable until the built-in
//! finishes parsing all options. In this implementation, the value may
//! temporarily contain two integers separated by a colon. The first integer is
//! the index of the next argument to parse, and the second is the index of the
//! character in the argument to parse. Other implementations may use a
//! different scheme. Some sets `$OPTIND` to the index of the just-parsed
//! argument and uses a hidden variable to keep track of the character index.
//!
//! The behavior is unspecified if you modify the `$OPTIND` variable between
//! invocations of the built-in or to a value other than `1`.
//!
//! # Implementation notes
//!
//! This implementation uses the `getopts_state` field in the [`Env`] to check
//! if the built-in is invoked with the same arguments and `$OPTIND` as the
//! previous invocation.

use crate::common::report_error;
use crate::common::report_simple_error;
use crate::common::syntax::parse_arguments;
use crate::common::syntax::Mode;
use either::Either::{Left, Right};
use std::num::NonZeroUsize;
use yash_env::builtin::getopts::Origin;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::variable::OPTIND;
use yash_env::Env;

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
        (Left(iter), Origin::DirectArgs)
    } else {
        let params = &env.variables.positional_params().values;
        let iter = params.iter().map(|v| v.as_str());
        (Right(iter), Origin::PositionalParams)
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
    if let Some(previous) = &env.getopts_state {
        match current.verify(previous) {
            Ok(None) => {}
            Ok(Some(current)) => env.getopts_state = Some(current.into_state()),
            Err(e) => return report_simple_error(env, &e.to_string()).await,
        }
    } else {
        if optind != "1" {
            let message = format!("unexpected $OPTIND value `{optind}`");
            return report_simple_error(env, &message).await;
        }
        env.getopts_state = Some(current.into_state());
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
        Err(error) => report_error(env, error.to_message()).await,
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
