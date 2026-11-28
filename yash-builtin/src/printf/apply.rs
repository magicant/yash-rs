// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Application of a parsed format to arguments

use super::arg::{NumberError, parse_float, parse_signed, parse_unsigned, skip_spaces};
use super::format::{Conversion, Count, Flag, Flags, Format, Kind, MAX_COUNT, Segment};
use super::number::{format_float, format_signed, format_unsigned};
use crate::echo::append_escaped;
use std::io::Write as _;
use std::ops::ControlFlow::{self, Continue};
use thiserror::Error;
use yash_env::semantics::Field;
use yash_env::source::pretty::{Footnote, FootnoteType, Report, ReportType, Snippet};
use yash_quote::quoted;

/// Feature of an argument whose result POSIX does not specify
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NonPortable {
    /// An argument for `%b` contains an escape sequence that POSIX does not
    /// specify.
    Escape,
    /// An argument for `%b` contains `\c` after the bytes the precision
    /// allows.
    BackslashCAfterPrecision,
    /// The argument for `%c` is empty.
    EmptyChar,
    /// A numeric argument starts with a blank.
    LeadingBlank,
    /// A numeric argument is a quote without a following character.
    LoneQuote,
    /// An argument for an unsigned conversion is negative.
    NegativeUnsigned,
    /// Arguments are given to a format that consumes no argument.
    Unconsumed,
}

impl NonPortable {
    fn label(self) -> &'static str {
        match self {
            Self::Escape => "the result of this escape sequence may differ in other shells",
            Self::BackslashCAfterPrecision => {
                "whether `\\c` after the precision takes effect may differ in other shells"
            }
            Self::EmptyChar => {
                "the result of `%c` with an empty argument may differ in other shells"
            }
            Self::LeadingBlank => {
                "the result of a number preceded by blanks may differ in other shells"
            }
            Self::LoneQuote => {
                "the result of a quote without a following character may differ in other shells"
            }
            Self::NegativeUnsigned => {
                "the result of a negative value for an unsigned conversion may differ in other shells"
            }
            Self::Unconsumed => {
                "arguments for a format that consumes no argument may be handled differently in other shells"
            }
        }
    }
}

/// Error in converting an argument
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum Error<'a> {
    /// An argument is not a valid integer.
    #[error("invalid integer")]
    InvalidInteger(&'a Field),
    /// An integer argument is out of range.
    #[error("integer out of range")]
    IntegerOutOfRange(&'a Field),
    /// An argument is not a valid floating-point number.
    #[error("invalid number")]
    InvalidFloat(&'a Field),
    /// A floating-point argument is out of range.
    #[error("number out of range")]
    FloatOutOfRange(&'a Field),
    /// A field width or precision given by `*` exceeds [`MAX_COUNT`] in
    /// absolute value.
    #[error("field width or precision out of range")]
    CountOutOfRange(&'a Field),
    /// A numbered conversion refers to a missing argument.
    #[error("missing argument")]
    MissingArgument {
        /// Format operand containing the conversion
        format: &'a Field,
        /// One-based index of the missing argument in the current pass
        position: usize,
    },
    /// An argument has a feature whose result POSIX does not specify while
    /// the `portable` shell option is on.
    #[error("non-portable argument")]
    NonPortable {
        /// Argument (or the format operand for a missing argument)
        field: &'a Field,
        reason: NonPortable,
    },
}

impl Error<'_> {
    /// Converts this error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let (field, label) = match self {
            Self::InvalidInteger(field) => (field, "not a valid integer".to_owned()),
            Self::IntegerOutOfRange(field) => (
                field,
                "the value is out of the range of integers".to_owned(),
            ),
            Self::InvalidFloat(field) => (field, "not a valid number".to_owned()),
            Self::FloatOutOfRange(field) => (
                field,
                "the value is out of the representable range".to_owned(),
            ),
            Self::CountOutOfRange(field) => (
                field,
                format!("the absolute value must not exceed {MAX_COUNT}"),
            ),
            Self::MissingArgument { format, position } => {
                (format, format!("there is no argument {position}"))
            }
            Self::NonPortable { field, reason } => (field, reason.label().to_owned()),
        };
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report.snippets = Snippet::with_primary_span(&field.origin, label.into());
        if let Self::NonPortable { .. } = self {
            report.footnotes.push(Footnote {
                r#type: FootnoteType::Note,
                label: "this error is reported because the `portable` shell option is enabled"
                    .into(),
            });
        }
        report
    }
}

impl<'a> From<&'a Error<'a>> for Report<'a> {
    #[inline]
    fn from(error: &'a Error<'a>) -> Self {
        error.to_report()
    }
}

/// Tests whether the argument for `%b` contains an escape sequence whose result
/// POSIX does not specify.
fn has_unspecified_escape(arg: &str) -> bool {
    let mut rest = arg.as_bytes();
    while let Some(i) = rest.iter().position(|&b| b == b'\\') {
        rest = &rest[i + 1..];
        match rest {
            [b'\\' | b'a'..=b'c' | b'f' | b'n' | b'r' | b't' | b'v', ..] => rest = &rest[1..],
            // A value larger than 0377 does not fit in a byte.
            [b'0', b'4'..=b'7', b'0'..=b'7', b'0'..=b'7', ..] => return true,
            [b'0', ..] => rest = &rest[1..],
            _ => return true,
        }
    }
    false
}

/// Truncates the bytes appended to `out` after `start` to `precision` bytes
/// and pads them to `width` bytes.
fn justify(out: &mut Vec<u8>, start: usize, flags: Flags, width: usize, precision: Option<usize>) {
    if let Some(precision) = precision {
        out.truncate(start + precision);
    }
    let padding = std::iter::repeat_n(b' ', width.saturating_sub(out.len() - start));
    if flags.contains(Flag::Left) {
        out.extend(padding);
    } else {
        out.splice(start..start, padding);
    }
}

/// Appends `content` to `out`, truncating it to `precision` bytes and padding
/// it to `width` bytes.
fn pad_bytes(
    out: &mut Vec<u8>,
    content: &[u8],
    flags: Flags,
    width: usize,
    precision: Option<usize>,
) {
    let len = precision.map_or(content.len(), |precision| precision.min(content.len()));
    let start = out.len();
    out.extend_from_slice(&content[..len]);
    justify(out, start, flags, width, None);
}

/// State of applying a format to arguments
struct Applier<'a> {
    format: &'a Field,
    args: &'a [Field],
    portable: bool,
    /// Index of the first argument in the current pass
    base: usize,
    /// Index of the next unnumbered argument relative to `base`
    next: usize,
    /// Number of arguments consumed in the current pass
    consumed: usize,
    out: Vec<u8>,
    errors: Vec<Error<'a>>,
}

impl<'a> Applier<'a> {
    /// Returns the argument for the position and marks it as consumed.
    ///
    /// The position is relative to the current pass, and `None` means the next
    /// unnumbered argument.
    fn arg(&mut self, position: Option<usize>) -> Option<&'a Field> {
        let index = position.unwrap_or_else(|| {
            self.next += 1;
            self.next - 1
        });
        self.consumed = self.consumed.max(index + 1);
        let arg = self.args.get(self.base + index);
        if arg.is_none() && position.is_some() {
            self.errors.push(Error::MissingArgument {
                format: self.format,
                position: index + 1,
            });
        }
        arg
    }

    /// Converts the argument to a number with `parse`, recording the error if
    /// any.
    ///
    /// A missing argument is zero.
    fn number<T: Default>(
        &mut self,
        arg: Option<&'a Field>,
        parse: fn(&str) -> (T, Option<NumberError>),
        float: bool,
    ) -> T {
        let Some(arg) = arg else {
            return T::default();
        };
        let (value, error) = parse(&arg.value);
        let error = match (error, float) {
            (None, _) => return value,
            (Some(NumberError::Invalid), false) => Error::InvalidInteger(arg),
            (Some(NumberError::OutOfRange), false) => Error::IntegerOutOfRange(arg),
            (Some(NumberError::Invalid), true) => Error::InvalidFloat(arg),
            (Some(NumberError::OutOfRange), true) => Error::FloatOutOfRange(arg),
        };
        self.errors.push(error);
        value
    }

    /// Records a non-portable feature of the argument if the `portable` shell
    /// option is on.
    fn non_portable(&mut self, field: &'a Field, reason: NonPortable) {
        if self.portable {
            self.errors.push(Error::NonPortable { field, reason });
        }
    }

    /// Checks the portability of an argument for an integer conversion.
    fn check_integer(&mut self, arg: Option<&'a Field>, unsigned: bool) {
        let Some(arg) = arg.filter(|_| self.portable) else {
            return;
        };
        let value = arg.value.as_str();
        let reason = if skip_spaces(value).len() < value.len() {
            NonPortable::LeadingBlank
        } else if matches!(value, "'" | "\"") {
            NonPortable::LoneQuote
        } else if unsigned && parse_signed(value).0 < 0 {
            NonPortable::NegativeUnsigned
        } else {
            return;
        };
        self.non_portable(arg, reason);
    }

    /// Resolves a field width or precision.
    ///
    /// Returns `None` if the count is not specified. A count from an argument
    /// is clamped to the range from `-MAX_COUNT` to `MAX_COUNT`.
    fn count(&mut self, count: Option<Count>) -> Option<i64> {
        const MAX: i64 = MAX_COUNT as i64;
        let position = match count? {
            Count::Fixed(count) => return Some(count as i64),
            Count::Argument(position) => position,
        };
        let Some(arg) = self.arg(position) else {
            return Some(0);
        };
        let (count, error) = parse_signed(&arg.value);
        let error = match error {
            Some(NumberError::Invalid) => Some(Error::InvalidInteger(arg)),
            Some(NumberError::OutOfRange) => Some(Error::CountOutOfRange(arg)),
            None if !(-MAX..=MAX).contains(&count) => Some(Error::CountOutOfRange(arg)),
            None => None,
        };
        self.errors.extend(error);
        Some(count.clamp(-MAX, MAX))
    }

    /// Applies a conversion specification.
    ///
    /// Returns `Break` if the output should stop because of `\c`.
    fn convert(&mut self, conversion: &Conversion) -> ControlFlow<()> {
        let mut flags = conversion.flags;
        let width = self.count(conversion.width).map_or(0, |width| {
            // A negative width from an argument means left-justification.
            if width < 0 {
                flags.insert(Flag::Left);
            }
            // This does not truncate because `count` limits the value to MAX_COUNT.
            width.unsigned_abs() as usize
        });
        let precision = self
            .count(conversion.precision)
            .and_then(|precision| usize::try_from(precision).ok());
        let arg = self.arg(conversion.position);

        match conversion.kind {
            Kind::Signed => {
                self.check_integer(arg, false);
                let value = self.number(arg, parse_signed, false);
                format_signed(&mut self.out, value, flags, width, precision);
            }
            Kind::Unsigned(radix) => {
                self.check_integer(arg, true);
                let value = self.number(arg, parse_unsigned, false);
                format_unsigned(&mut self.out, value, radix, flags, width, precision);
            }
            Kind::Float { style, upper } => {
                let value = self.number(arg, parse_float, true);
                format_float(&mut self.out, value, style, upper, flags, width, precision);
            }
            Kind::Char => {
                let value = arg.map_or("", |arg| &arg.value);
                if value.is_empty() {
                    // A missing argument is regarded as empty.
                    self.non_portable(arg.unwrap_or(self.format), NonPortable::EmptyChar);
                }
                pad_bytes(&mut self.out, value.as_bytes(), flags, width, Some(1));
            }
            Kind::String => {
                let value = arg.map_or("", |arg| &arg.value);
                pad_bytes(&mut self.out, value.as_bytes(), flags, width, precision);
            }
            Kind::Quoted => {
                let start = self.out.len();
                let value = quoted(arg.map_or("", |arg| &arg.value));
                write!(self.out, "{value}").unwrap();
                justify(&mut self.out, start, flags, width, precision);
            }
            Kind::Escaped => {
                if self.portable
                    && let Some(arg) = arg
                    && has_unspecified_escape(&arg.value)
                {
                    self.non_portable(arg, NonPortable::Escape);
                }
                let start = self.out.len();
                let result = append_escaped(&mut self.out, arg.map_or("", |arg| &arg.value));
                if let Some(arg) = arg
                    && result.is_break()
                    && precision.is_some_and(|precision| self.out.len() - start >= precision)
                {
                    self.non_portable(arg, NonPortable::BackslashCAfterPrecision);
                }
                justify(&mut self.out, start, flags, width, precision);
                return result;
            }
        }
        Continue(())
    }

    /// Applies the format once.
    fn apply_once(&mut self, format: &Format) -> ControlFlow<()> {
        for segment in &format.segments {
            match segment {
                Segment::Literal(bytes) => self.out.extend_from_slice(bytes),
                Segment::Conversion(conversion) => self.convert(conversion)?,
            }
        }
        Continue(())
    }
}

/// Applies the format to the arguments.
///
/// The format is reused until all the arguments are consumed. Returns the
/// output and the errors in converting the arguments.
pub fn apply<'a>(
    format: &Format,
    format_field: &'a Field,
    args: &'a [Field],
    portable: bool,
) -> (Vec<u8>, Vec<Error<'a>>) {
    let mut applier = Applier {
        format: format_field,
        args,
        portable,
        base: 0,
        next: 0,
        consumed: 0,
        out: Vec::new(),
        errors: Vec::new(),
    };
    if let Some(arg) = args.first()
        && !format
            .segments
            .iter()
            .any(|segment| matches!(segment, Segment::Conversion(_)))
    {
        applier.non_portable(arg, NonPortable::Unconsumed);
    }
    while applier.apply_once(format).is_continue() && applier.consumed > 0 {
        applier.base += applier.consumed;
        if applier.base >= args.len() {
            break;
        }
        applier.next = 0;
        applier.consumed = 0;
    }
    (applier.out, applier.errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printf::format::parse;

    fn run<'a>(format: &'a Field, args: &'a [Field], portable: bool) -> (String, Vec<Error<'a>>) {
        let parsed = parse(&format.value).unwrap();
        let (out, errors) = apply(&parsed, format, args, portable);
        (String::from_utf8(out).unwrap(), errors)
    }

    #[test]
    fn has_unspecified_escape_detects_non_posix_sequences() {
        assert!(!has_unspecified_escape(r"\\\a\b\c\f\n\r\t\v\0123"));
        assert!(has_unspecified_escape(r"\e"));
        assert!(has_unspecified_escape(r"\123"));
        assert!(has_unspecified_escape("a\\"));
        assert!(!has_unspecified_escape(r"\0377\077\0\\0777"));
        assert!(has_unspecified_escape(r"\0400"));
        assert!(has_unspecified_escape(r"\0777"));
    }

    #[test]
    fn apply_without_arguments() {
        let format = Field::dummy("[%s|%d|%c|%b|%f]");

        let (out, errors) = run(&format, &[], false);

        assert_eq!(out, "[|0|||0.000000]");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_reuses_format() {
        let format = Field::dummy("%s-%s,");
        let args = Field::dummies(["1", "2", "3"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "1-2,3-,");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_ignores_arguments_without_conversions() {
        let format = Field::dummy("foo");
        let args = Field::dummies(["1", "2"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "foo");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_numbered_conversions() {
        let format = Field::dummy("%3$s %1$s|");
        let args = Field::dummies(["a", "b", "c", "d", "e", "f"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "c a|f d|");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_mixed_numbered_and_unnumbered_conversions() {
        let format = Field::dummy("%2$s %s %s|");
        let args = Field::dummies(["a", "b", "c", "d", "e", "f"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "b a b|d c d|f e f|");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_mixed_numbered_and_unnumbered_counts() {
        let format = Field::dummy("[%*1$d]");
        let args = Field::dummies(["3", "4"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "[  3][   4]");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_reports_missing_numbered_argument() {
        let format = Field::dummy("%2$s|");
        let args = Field::dummies(["a", "b", "c"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "b||");
        assert_eq!(
            errors,
            [Error::MissingArgument {
                format: &format,
                position: 2
            }]
        );
    }

    #[test]
    fn apply_stops_at_backslash_c() {
        let format = Field::dummy("%b|%s\n");
        let args = Field::dummies(["a", "b", r"c\cd", "e"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "a|b\nc");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_escaped_with_width_and_precision() {
        let format = Field::dummy("[%5.2b][%-4b]");
        let args = Field::dummies([r"\tbc", r"\0101"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "[   \tb][A   ]");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_char_and_string_count_bytes() {
        let format = Field::dummy("[%c][%3c][%.1s][%4s][%-3s]");
        let args = Field::dummies(["xyz", "", "ab", "あ", "a"]);
        let parsed = parse(&format.value).unwrap();

        let (out, errors) = apply(&parsed, &format, &args, false);

        assert_eq!(out, b"[x][   ][a][ \xE3\x81\x82][a  ]");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_char_writes_first_byte_of_multibyte_character() {
        let format = Field::dummy("%c");
        let args = Field::dummies(["あ"]);
        let parsed = parse(&format.value).unwrap();

        let (out, _) = apply(&parsed, &format, &args, false);

        assert_eq!(out, b"\xE3");
    }

    #[test]
    fn apply_quoted() {
        let format = Field::dummy("%q %q %q|%5q");
        let args = Field::dummies(["a b", "", "x", "y"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "'a b' '' x|    y");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_numbers() {
        let format = Field::dummy("%d %u %o %x %.2f");
        let args = Field::dummies(["-1", "-1", "8", "255", "3.14159"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "-1 18446744073709551615 10 ff 3.14");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_width_and_precision_from_arguments() {
        let format = Field::dummy("[%*d][%-*d][%.*s][%*d][%.*s]");
        let args = Field::dummies(["4", "1", "3", "2", "2", "abc", "-3", "5", "-1", "xyz"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "[   1][2  ][ab][5  ][xyz]");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_numbered_width() {
        let format = Field::dummy("[%2$*1$s]");
        let args = Field::dummies(["3", "a"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "[  a]");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_reports_invalid_numbers() {
        let format = Field::dummy("%d %d %f %f\n");
        let args = Field::dummies(["5a", "99999999999999999999", "x", "1e999"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "5 9223372036854775807 0.000000 inf\n");
        assert_eq!(
            errors,
            [
                Error::InvalidInteger(&args[0]),
                Error::IntegerOutOfRange(&args[1]),
                Error::InvalidFloat(&args[2]),
                Error::FloatOutOfRange(&args[3]),
            ]
        );
    }

    #[test]
    fn apply_reports_invalid_width() {
        let format = Field::dummy("[%*s]");
        let args = Field::dummies(["x", "a"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "[a]");
        assert_eq!(errors, [Error::InvalidInteger(&args[0])]);
    }

    #[test]
    fn apply_reports_too_large_precision() {
        // Too large precisions are tested instead of widths because a width
        // clamped to MAX_COUNT would allocate gigabytes of padding.
        let format = Field::dummy("[%.*s][%.*s][%.*s][%.*s][%.*s]");
        let args = Field::dummies([
            "2147483647",
            "a",
            "2147483648",
            "b",
            "4294967296",
            "c",
            "99999999999999999999",
            "d",
            "-2147483648",
            "e",
        ]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "[a][b][c][d][e]");
        assert_eq!(
            errors,
            [
                Error::CountOutOfRange(&args[2]),
                Error::CountOutOfRange(&args[4]),
                Error::CountOutOfRange(&args[6]),
                Error::CountOutOfRange(&args[8]),
            ]
        );
    }

    #[test]
    fn apply_accepts_non_posix_escape_without_portable_option() {
        let format = Field::dummy("%b");
        let args = Field::dummies([r"\e"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "\x1B");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_rejects_non_posix_escape_with_portable_option() {
        let format = Field::dummy("%b%b");
        let args = Field::dummies([r"\n", r"\e"]);

        let (_, errors) = run(&format, &args, true);

        assert_eq!(
            errors,
            [Error::NonPortable {
                field: &args[1],
                reason: NonPortable::Escape
            }]
        );
    }

    #[test]
    fn apply_accepts_non_portable_arguments_without_portable_option() {
        let format = Field::dummy("[%c][%.1b][%d][%d][%u]");
        let args = Field::dummies(["", r"ab\c", " 1", "'", "-1"]);

        let (out, errors) = run(&format, &args, false);

        assert_eq!(out, "[][a");
        assert_eq!(errors, []);
    }

    #[test]
    fn apply_rejects_empty_char_with_portable_option() {
        let format = Field::dummy("%c%c");
        let args = Field::dummies([""]);

        let (_, errors) = run(&format, &args, true);

        let reason = NonPortable::EmptyChar;
        assert_eq!(
            errors,
            [
                Error::NonPortable {
                    field: &args[0],
                    reason
                },
                Error::NonPortable {
                    field: &format,
                    reason
                },
            ]
        );
    }

    #[test]
    fn apply_rejects_backslash_c_after_precision_with_portable_option() {
        let format = Field::dummy("%.2b");
        let args = Field::dummies([r"a\c", r"ab\c"]);

        let (_, errors) = run(&format, &args, true);

        // The first `\c` stops the output before the second argument.
        assert_eq!(errors, []);

        let args = Field::dummies([r"ab\c"]);
        let (_, errors) = run(&format, &args, true);
        assert_eq!(
            errors,
            [Error::NonPortable {
                field: &args[0],
                reason: NonPortable::BackslashCAfterPrecision
            }]
        );
    }

    #[test]
    fn apply_rejects_non_portable_integers_with_portable_option() {
        let format = Field::dummy("%d%d%u%x%d%u");
        let args = Field::dummies([" 1", "'", "-1", "-0x10", "-1", "-0"]);

        let (_, errors) = run(&format, &args, true);

        assert_eq!(
            errors,
            [
                Error::NonPortable {
                    field: &args[0],
                    reason: NonPortable::LeadingBlank
                },
                Error::NonPortable {
                    field: &args[1],
                    reason: NonPortable::LoneQuote
                },
                Error::NonPortable {
                    field: &args[2],
                    reason: NonPortable::NegativeUnsigned
                },
                Error::NonPortable {
                    field: &args[3],
                    reason: NonPortable::NegativeUnsigned
                },
            ]
        );
    }

    #[test]
    fn apply_rejects_arguments_without_conversions_with_portable_option() {
        let format = Field::dummy("foo%%");
        let args = Field::dummies(["a", "b"]);

        let (_, errors) = run(&format, &args, true);

        assert_eq!(
            errors,
            [Error::NonPortable {
                field: &args[0],
                reason: NonPortable::Unconsumed
            }]
        );
    }
}
