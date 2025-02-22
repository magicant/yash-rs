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

//! Main domain model of the getopts built-in

use std::num::NonZeroUsize;
use thiserror::Error;

/// Type of an option
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OptionType {
    /// Option without an argument
    NoArgument,
    /// Option that takes an argument
    TakesArgument,
    /// Option not listed in the option specification
    Unknown,
}

/// Option specification
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OptionSpec<'a> {
    raw: &'a str,
}

/// Creates an option specification from a raw string representation.
impl<'a, S: AsRef<str> + ?Sized> From<&'a S> for OptionSpec<'a> {
    #[inline(always)]
    fn from(raw: &'a S) -> Self {
        Self { raw: raw.as_ref() }
    }
}

impl OptionSpec<'_> {
    /// Returns the raw string representation of the option specification.
    #[inline(always)]
    #[must_use]
    pub fn as_raw(&self) -> &str {
        self.raw
    }

    /// Returns the type of the option.
    #[must_use]
    pub fn judge(&self, option: char) -> OptionType {
        if option == ':' {
            return OptionType::Unknown;
        }

        let mut iter = self.raw.chars();
        match iter.find(|&c| c == option) {
            None => OptionType::Unknown,
            Some(c) => {
                debug_assert_eq!(c, option);
                if iter.next() == Some(':') {
                    OptionType::TakesArgument
                } else {
                    OptionType::NoArgument
                }
            }
        }
    }
}

/// Data of a single option occurrence
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct OptionOccurrence {
    /// Option character
    pub option: char,

    /// Argument to the option
    pub argument: Option<String>,

    /// Error that occurred when parsing the option
    pub error: Option<Error>,
}

/// Result of parsing an option
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Result {
    /// Data of the parsed option
    ///
    /// This field is `None` if the next argument is not an option.
    pub option: Option<OptionOccurrence>,

    /// Index of the next argument to parse
    ///
    /// The index starts from 1 and may go beyond the number of arguments.
    /// When there are no more arguments to parse, the index is set to the
    /// number of arguments plus one.
    pub next_arg_index: NonZeroUsize,

    /// Index of the next character to parse in the argument at `next_arg_index`
    ///
    /// The hyphen character (`-`) is not included in the count. For example, if
    /// `next_char_index` is 1, the next character to parse is the first option
    /// character just following the leading hyphen character.
    pub next_char_index: NonZeroUsize,
}

impl Result {
    /// Creates a result for a non-option argument.
    #[must_use]
    fn non_option(next_arg_index: NonZeroUsize) -> Self {
        Self {
            option: None,
            next_arg_index,
            next_char_index: NonZeroUsize::MIN,
        }
    }
}

/// Error that may occur when parsing an option
#[derive(Clone, Copy, Debug, Eq, Error, Hash, PartialEq)]
pub enum Error {
    /// The option is not listed in the option specification.
    #[error("invalid option")]
    UnknownOption,
    /// The option takes an argument but the argument is missing.
    #[error("missing argument")]
    MissingArgument,
}

/// Parses an option from the specified arguments.
///
/// If the next argument is an option, returns the parsed option.
/// Otherwise, returns `None`.
#[must_use]
pub fn next<S, I>(
    args: I,
    spec: OptionSpec,
    arg_index: NonZeroUsize,
    char_index: NonZeroUsize,
) -> Result
where
    S: AsRef<str>,
    I: IntoIterator<Item = S>,
{
    let mut args = args.into_iter().skip(arg_index.get() - 1);
    let Some(arg) = args.next() else {
        return Result::non_option(arg_index);
    };
    let mut chars = arg.as_ref().chars();
    let Some('-') = chars.next() else {
        return Result::non_option(arg_index);
    };
    if chars.as_str() == "-" {
        debug_assert_eq!(arg.as_ref(), "--");
        // Rust's slices cannot be as large as `usize::MAX`, so we can safely unwrap here.
        return Result::non_option(arg_index.checked_add(1).unwrap());
    }

    let Some(option) = chars.nth(char_index.get() - 1) else {
        return Result::non_option(arg_index);
    };

    /// Computes the increment of the argument index.
    /// Only used when `option` does not take an argument.
    fn arg_index_incr<I: Iterator<Item = char>>(mut chars: I) -> usize {
        if chars.next().is_some() {
            0 // The next option is in the same argument, so no increment.
        } else {
            1 // No more options in the current argument, so increment.
        }
    }

    let (argument, arg_index_incr, error) = match spec.judge(option) {
        OptionType::Unknown => (None, arg_index_incr(chars), Some(Error::UnknownOption)),
        OptionType::NoArgument => (None, arg_index_incr(chars), None),
        OptionType::TakesArgument => {
            let remainder = chars.collect::<String>();
            if !remainder.is_empty() {
                (Some(remainder), 1, None)
            } else { match args.next() { Some(arg) => {
                (Some(arg.as_ref().to_owned()), 2, None)
            } _ => {
                (None, 1, Some(Error::MissingArgument))
            }}}
        }
    };

    // Rust's slices cannot be as large as `usize::MAX`, so we can safely unwrap here.
    let next_arg_index = arg_index.checked_add(arg_index_incr).unwrap();
    let next_char_index = if arg_index_incr == 0 {
        char_index.checked_add(1).unwrap()
    } else {
        NonZeroUsize::MIN
    };

    Result {
        option: Some(OptionOccurrence {
            option,
            argument,
            error,
        }),
        next_arg_index,
        next_char_index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn judge_options_without_arguments() {
        let spec = OptionSpec::from("abc:def");
        assert_eq!(spec.judge('a'), OptionType::NoArgument);
        assert_eq!(spec.judge('b'), OptionType::NoArgument);
        assert_eq!(spec.judge('d'), OptionType::NoArgument);
        assert_eq!(spec.judge('e'), OptionType::NoArgument);
        assert_eq!(spec.judge('f'), OptionType::NoArgument);
    }

    #[test]
    fn judge_options_that_take_argument() {
        let spec = OptionSpec::from("abc:de:f:");
        assert_eq!(spec.judge('c'), OptionType::TakesArgument);
        assert_eq!(spec.judge('e'), OptionType::TakesArgument);
        assert_eq!(spec.judge('f'), OptionType::TakesArgument);
    }

    #[test]
    fn judge_unknown_options() {
        let spec = OptionSpec::from("abc:df:");
        assert_eq!(spec.judge('x'), OptionType::Unknown);
        assert_eq!(spec.judge('e'), OptionType::Unknown);

        // Colon is always unknown
        assert_eq!(spec.judge(':'), OptionType::Unknown);
    }

    fn non_zero(i: usize) -> NonZeroUsize {
        NonZeroUsize::new(i).unwrap()
    }

    #[test]
    fn next_with_empty_arguments() {
        let args = [] as [&str; 0];
        assert_eq!(
            next(args, "a".into(), non_zero(1), non_zero(1)),
            Result {
                option: None,
                next_arg_index: non_zero(1),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_single_hyphen() {
        assert_eq!(
            next(["-"], "a".into(), non_zero(1), non_zero(1)),
            Result {
                option: None,
                next_arg_index: non_zero(1),
                next_char_index: non_zero(1),
            }
        );

        assert_eq!(
            next(["-a", "-a", "-"], "a".into(), non_zero(3), non_zero(1)),
            Result {
                option: None,
                next_arg_index: non_zero(3),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_non_option_argument() {
        assert_eq!(
            next([""], "a".into(), non_zero(1), non_zero(1)),
            Result {
                option: None,
                next_arg_index: non_zero(1),
                next_char_index: non_zero(1),
            }
        );

        assert_eq!(
            next(["-a", "-a", "abc"], "a".into(), non_zero(3), non_zero(1)),
            Result {
                option: None,
                next_arg_index: non_zero(3),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_double_hyphen_separator() {
        assert_eq!(
            next(["--"], "a".into(), non_zero(1), non_zero(1)),
            Result {
                option: None,
                next_arg_index: non_zero(2),
                next_char_index: non_zero(1),
            }
        );

        assert_eq!(
            next(["-a", "--", "x"], "a".into(), non_zero(2), non_zero(1)),
            Result {
                option: None,
                next_arg_index: non_zero(3),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_single_option() {
        assert_eq!(
            next(["-a"], "a".into(), non_zero(1), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'a',
                    argument: None,
                    error: None,
                }),
                next_arg_index: non_zero(2),
                next_char_index: non_zero(1),
            }
        );

        assert_eq!(
            next(["-x", "-x"], "x".into(), non_zero(2), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'x',
                    argument: None,
                    error: None,
                }),
                next_arg_index: non_zero(3),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_many_options_in_single_argument() {
        assert_eq!(
            next(["-abc"], "abc".into(), non_zero(1), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'a',
                    argument: None,
                    error: None,
                }),
                next_arg_index: non_zero(1),
                next_char_index: non_zero(2),
            }
        );

        assert_eq!(
            next(["-abc"], "abc".into(), non_zero(1), non_zero(2)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'b',
                    argument: None,
                    error: None,
                }),
                next_arg_index: non_zero(1),
                next_char_index: non_zero(3),
            }
        );

        assert_eq!(
            next(["-abc"], "abc".into(), non_zero(1), non_zero(3)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'c',
                    argument: None,
                    error: None,
                }),
                next_arg_index: non_zero(2),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_many_option_arguments() {
        assert_eq!(
            next(["-a", "-b", "-c"], "abc".into(), non_zero(1), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'a',
                    argument: None,
                    error: None,
                }),
                next_arg_index: non_zero(2),
                next_char_index: non_zero(1),
            }
        );

        assert_eq!(
            next(["-a", "-b", "-c"], "abc".into(), non_zero(2), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'b',
                    argument: None,
                    error: None,
                }),
                next_arg_index: non_zero(3),
                next_char_index: non_zero(1),
            }
        );

        assert_eq!(
            next(["-a", "-b", "-c"], "abc".into(), non_zero(3), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'c',
                    argument: None,
                    error: None,
                }),
                next_arg_index: non_zero(4),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_unknown_option() {
        assert_eq!(
            next(["-a"], "".into(), non_zero(1), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'a',
                    argument: None,
                    error: Some(Error::UnknownOption),
                }),
                next_arg_index: non_zero(2),
                next_char_index: non_zero(1),
            }
        );

        assert_eq!(
            next(["-x"], "a".into(), non_zero(1), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'x',
                    argument: None,
                    error: Some(Error::UnknownOption),
                }),
                next_arg_index: non_zero(2),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_option_argument_in_same_argument() {
        assert_eq!(
            next(["-abc"], "a:bc".into(), non_zero(1), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'a',
                    argument: Some("bc".into()),
                    error: None,
                }),
                next_arg_index: non_zero(2),
                next_char_index: non_zero(1),
            }
        );

        assert_eq!(
            next(["-cba", "-abc"], "ab:c".into(), non_zero(2), non_zero(2)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'b',
                    argument: Some("c".into()),
                    error: None,
                }),
                next_arg_index: non_zero(3),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_option_argument_in_next_argument() {
        assert_eq!(
            next(["-a", "bc"], "a:bc".into(), non_zero(1), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'a',
                    argument: Some("bc".into()),
                    error: None,
                }),
                next_arg_index: non_zero(3),
                next_char_index: non_zero(1),
            }
        );

        assert_eq!(
            next(["-a", "-b", "-c"], "ab:c".into(), non_zero(2), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'b',
                    argument: Some("-c".into()),
                    error: None,
                }),
                next_arg_index: non_zero(4),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_missing_option_argument() {
        assert_eq!(
            next(["-a"], "a:".into(), non_zero(1), non_zero(1)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'a',
                    argument: None,
                    error: Some(Error::MissingArgument),
                }),
                next_arg_index: non_zero(2),
                next_char_index: non_zero(1),
            }
        );

        assert_eq!(
            next(["-a", "-ab"], "ab:".into(), non_zero(2), non_zero(2)),
            Result {
                option: Some(OptionOccurrence {
                    option: 'b',
                    argument: None,
                    error: Some(Error::MissingArgument),
                }),
                next_arg_index: non_zero(3),
                next_char_index: non_zero(1),
            }
        );
    }

    #[test]
    fn next_with_too_large_arg_index() {
        // This case should not happen in practice, so we don't expect any
        // particular next indexes.
        let result = next(["-a"], "a".into(), non_zero(2), non_zero(1));
        assert_eq!(result.option, None);

        let result = next(["-a"], "a".into(), NonZeroUsize::MAX, non_zero(1));
        assert_eq!(result.option, None);
    }

    #[test]
    fn next_with_too_large_char_index() {
        // This case should not happen in practice, so we don't expect any
        // particular next indexes.
        let result = next(["-a"], "a".into(), non_zero(1), non_zero(2));
        assert_eq!(result.option, None);

        let result = next(["-a"], "a".into(), non_zero(1), NonZeroUsize::MAX);
        assert_eq!(result.option, None);
    }
}
