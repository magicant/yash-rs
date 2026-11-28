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

//! Formatting of numbers as the C `printf` function does

use super::format::{Flag, Flags, FloatStyle, Radix};
use std::fmt::Write as _;

/// Default precision of floating-point conversions
const DEFAULT_FLOAT_PRECISION: usize = 6;

/// Appends a formatted number to `out`, padding it to `width`.
///
/// `prefix` is the sign and radix prefix, between which and `digits` zeros are
/// inserted if `zero_pad` is true.
fn pad_number(
    out: &mut Vec<u8>,
    prefix: &str,
    digits: &str,
    flags: Flags,
    width: usize,
    zero_pad: bool,
) {
    let padding = width.saturating_sub(prefix.len() + digits.len());
    let (left, zeros, right) = if flags.contains(Flag::Left) {
        (0, 0, padding)
    } else if zero_pad {
        (0, padding, 0)
    } else {
        (padding, 0, 0)
    };
    out.extend(std::iter::repeat_n(b' ', left));
    out.extend_from_slice(prefix.as_bytes());
    out.extend(std::iter::repeat_n(b'0', zeros));
    out.extend_from_slice(digits.as_bytes());
    out.extend(std::iter::repeat_n(b' ', right));
}

/// Returns the sign prefix of a non-negative number.
fn positive_sign(flags: Flags) -> &'static str {
    if flags.contains(Flag::Plus) {
        "+"
    } else if flags.contains(Flag::Space) {
        " "
    } else {
        ""
    }
}

/// Formats the digits of an integer with the minimum number of digits.
fn integer_digits(digits: String, precision: Option<usize>) -> String {
    match precision {
        // The C standard prints no digits for zero with zero precision.
        Some(0) if digits == "0" => String::new(),
        Some(precision) if digits.len() < precision => {
            format!("{digits:0>precision$}")
        }
        _ => digits,
    }
}

/// Appends a signed integer formatted as `%d`.
pub fn format_signed(
    out: &mut Vec<u8>,
    value: i64,
    flags: Flags,
    width: usize,
    precision: Option<usize>,
) {
    let sign = if value < 0 { "-" } else { positive_sign(flags) };
    let digits = integer_digits(value.unsigned_abs().to_string(), precision);
    let zero_pad = flags.contains(Flag::Zero) && precision.is_none();
    pad_number(out, sign, &digits, flags, width, zero_pad);
}

/// Appends an unsigned integer formatted as `%o`, `%u`, `%x`, or `%X`.
pub fn format_unsigned(
    out: &mut Vec<u8>,
    value: u64,
    radix: Radix,
    flags: Flags,
    width: usize,
    precision: Option<usize>,
) {
    let digits = match radix {
        Radix::Octal => format!("{value:o}"),
        Radix::Decimal => value.to_string(),
        Radix::LowerHex => format!("{value:x}"),
        Radix::UpperHex => format!("{value:X}"),
    };
    let mut digits = integer_digits(digits, precision);
    let mut prefix = "";
    if flags.contains(Flag::Alternate) {
        match radix {
            Radix::Octal if !digits.starts_with('0') => digits.insert(0, '0'),
            Radix::LowerHex if value != 0 => prefix = "0x",
            Radix::UpperHex if value != 0 => prefix = "0X",
            _ => {}
        }
    }
    let zero_pad = flags.contains(Flag::Zero) && precision.is_none();
    pad_number(out, prefix, &digits, flags, width, zero_pad);
}

/// Formats a non-negative finite number in the exponent notation.
fn exponent_notation(value: f64, precision: usize, upper: bool, alternate: bool) -> String {
    let formatted = format!("{value:.precision$e}");
    let (mantissa, exponent) = formatted.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    let mut result = mantissa.to_owned();
    if alternate && precision == 0 {
        result.push('.');
    }
    result.push(if upper { 'E' } else { 'e' });
    result.push(if exponent < 0 { '-' } else { '+' });
    write!(result, "{:02}", exponent.unsigned_abs()).unwrap();
    result
}

/// Formats a non-negative finite number in the fixed-point notation.
fn fixed_notation(value: f64, precision: usize, alternate: bool) -> String {
    let mut result = format!("{value:.precision$}");
    if alternate && precision == 0 {
        result.push('.');
    }
    result
}

/// Removes trailing zeros in the fractional part, and the decimal point if
/// the fractional part becomes empty.
fn remove_trailing_zeros(number: &mut String) {
    let mantissa_end = number.find(['e', 'E']).unwrap_or(number.len());
    let mantissa = &number[..mantissa_end];
    if !mantissa.contains('.') {
        return;
    }
    let trimmed = mantissa.trim_end_matches('0').trim_end_matches('.').len();
    number.replace_range(trimmed..mantissa_end, "");
}

/// Formats a non-negative finite number as `%g` does.
fn general_notation(value: f64, precision: usize, upper: bool, alternate: bool) -> String {
    let precision = precision.max(1);
    // The exponent is the one the number would have after rounding to the
    // precision.
    let exponent: i32 = if value == 0.0 {
        0
    } else {
        let formatted = format!("{:.*e}", precision - 1, value);
        formatted.split_once('e').unwrap().1.parse().unwrap()
    };

    let mut result = if -4 <= exponent && (exponent as i64) < precision as i64 {
        let fraction_digits = (precision as i64 - 1 - exponent as i64) as usize;
        fixed_notation(value, fraction_digits, alternate)
    } else {
        exponent_notation(value, precision - 1, upper, alternate)
    };
    if !alternate {
        remove_trailing_zeros(&mut result);
    }
    result
}

/// Appends a floating-point number formatted as `%e`, `%f`, `%g`, or their
/// uppercase variants.
pub fn format_float(
    out: &mut Vec<u8>,
    value: f64,
    style: FloatStyle,
    upper: bool,
    flags: Flags,
    width: usize,
    precision: Option<usize>,
) {
    let sign = if value.is_sign_negative() {
        "-"
    } else {
        positive_sign(flags)
    };
    let magnitude = value.abs();

    if !magnitude.is_finite() {
        let digits = match (magnitude.is_nan(), upper) {
            (false, false) => "inf",
            (false, true) => "INF",
            (true, false) => "nan",
            (true, true) => "NAN",
        };
        pad_number(out, sign, digits, flags, width, false);
        return;
    }

    let precision = precision.unwrap_or(DEFAULT_FLOAT_PRECISION);
    let alternate = flags.contains(Flag::Alternate);
    let digits = match style {
        FloatStyle::Fixed => fixed_notation(magnitude, precision, alternate),
        FloatStyle::Exponent => exponent_notation(magnitude, precision, upper, alternate),
        FloatStyle::General => general_notation(magnitude, precision, upper, alternate),
    };
    pad_number(out, sign, &digits, flags, width, flags.contains(Flag::Zero));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(value: i64, flags: Flags, width: usize, precision: Option<usize>) -> String {
        let mut out = Vec::new();
        format_signed(&mut out, value, flags, width, precision);
        String::from_utf8(out).unwrap()
    }

    fn unsigned(
        value: u64,
        radix: Radix,
        flags: Flags,
        width: usize,
        precision: Option<usize>,
    ) -> String {
        let mut out = Vec::new();
        format_unsigned(&mut out, value, radix, flags, width, precision);
        String::from_utf8(out).unwrap()
    }

    fn float(
        value: f64,
        style: FloatStyle,
        upper: bool,
        flags: Flags,
        width: usize,
        precision: Option<usize>,
    ) -> String {
        let mut out = Vec::new();
        format_float(&mut out, value, style, upper, flags, width, precision);
        String::from_utf8(out).unwrap()
    }

    const NO_FLAGS: Flags = Flags::empty();

    #[test]
    fn signed_plain() {
        assert_eq!(signed(0, NO_FLAGS, 0, None), "0");
        assert_eq!(signed(-1278, NO_FLAGS, 0, None), "-1278");
        assert_eq!(signed(i64::MIN, NO_FLAGS, 0, None), "-9223372036854775808");
    }

    #[test]
    fn signed_sign_flags() {
        let plus = Flags::only(Flag::Plus);
        let space = Flags::only(Flag::Space);
        let both = Flag::Plus | Flag::Space;
        assert_eq!(signed(1, plus, 0, None), "+1");
        assert_eq!(signed(1, space, 0, None), " 1");
        assert_eq!(signed(1, both, 0, None), "+1");
        assert_eq!(signed(-1, space, 0, None), "-1");
    }

    #[test]
    fn signed_width() {
        let left = Flags::only(Flag::Left);
        let zero = Flags::only(Flag::Zero);
        let left_zero = Flag::Left | Flag::Zero;
        assert_eq!(signed(-12, NO_FLAGS, 5, None), "  -12");
        assert_eq!(signed(-12, left, 5, None), "-12  ");
        assert_eq!(signed(-12, zero, 5, None), "-0012");
        assert_eq!(signed(-12, left_zero, 5, None), "-12  ");
        assert_eq!(signed(-12345, NO_FLAGS, 3, None), "-12345");
    }

    #[test]
    fn signed_precision() {
        let zero = Flags::only(Flag::Zero);
        assert_eq!(signed(20, NO_FLAGS, 0, Some(3)), "020");
        assert_eq!(signed(-1278, NO_FLAGS, 0, Some(3)), "-1278");
        assert_eq!(signed(7, zero, 5, Some(2)), "   07");
        assert_eq!(signed(0, NO_FLAGS, 0, Some(0)), "");
        assert_eq!(signed(0, NO_FLAGS, 2, Some(0)), "  ");
    }

    #[test]
    fn unsigned_radixes() {
        assert_eq!(unsigned(510, Radix::Octal, NO_FLAGS, 0, None), "776");
        assert_eq!(unsigned(510, Radix::Decimal, NO_FLAGS, 0, None), "510");
        assert_eq!(unsigned(510, Radix::LowerHex, NO_FLAGS, 0, None), "1fe");
        assert_eq!(unsigned(510, Radix::UpperHex, NO_FLAGS, 0, None), "1FE");
        assert_eq!(
            unsigned(u64::MAX, Radix::Decimal, NO_FLAGS, 0, None),
            "18446744073709551615"
        );
    }

    #[test]
    fn unsigned_ignores_sign_flags() {
        let flags = Flag::Plus | Flag::Space;
        assert_eq!(unsigned(1, Radix::Decimal, flags, 0, None), "1");
    }

    #[test]
    fn unsigned_alternate_form() {
        let alternate = Flags::only(Flag::Alternate);
        let alternate_zero = Flag::Alternate | Flag::Zero;
        assert_eq!(unsigned(8, Radix::Octal, alternate, 0, None), "010");
        assert_eq!(unsigned(0, Radix::Octal, alternate, 0, None), "0");
        assert_eq!(unsigned(0, Radix::Octal, alternate, 0, Some(0)), "0");
        assert_eq!(unsigned(8, Radix::Octal, alternate, 0, Some(3)), "010");
        assert_eq!(unsigned(255, Radix::LowerHex, alternate, 0, None), "0xff");
        assert_eq!(unsigned(255, Radix::UpperHex, alternate, 0, None), "0XFF");
        assert_eq!(unsigned(0, Radix::LowerHex, alternate, 0, None), "0");
        assert_eq!(
            unsigned(255, Radix::LowerHex, alternate_zero, 6, None),
            "0x00ff"
        );
    }

    #[test]
    fn float_fixed() {
        let f = |value, precision| float(value, FloatStyle::Fixed, false, NO_FLAGS, 0, precision);
        assert_eq!(f(0.0, None), "0.000000");
        assert_eq!(f(1.25, None), "1.250000");
        assert_eq!(f(-1000.0, Some(3)), "-1000.000");
        assert_eq!(f(1.25, Some(0)), "1");
        assert_eq!(f(2.5, Some(0)), "2");
        assert_eq!(f(0.125, Some(2)), "0.12");
        assert_eq!(f(-0.0, Some(1)), "-0.0");
        assert_eq!(f(1e20, Some(0)), "100000000000000000000");
    }

    #[test]
    fn float_fixed_alternate_form() {
        let alternate = Flags::only(Flag::Alternate);
        assert_eq!(
            float(1.0, FloatStyle::Fixed, false, alternate, 0, Some(0)),
            "1."
        );
    }

    #[test]
    fn float_exponent() {
        let e = |value, upper, precision| {
            float(value, FloatStyle::Exponent, upper, NO_FLAGS, 0, precision)
        };
        assert_eq!(e(0.0, false, None), "0.000000e+00");
        assert_eq!(e(1234.5, false, None), "1.234500e+03");
        assert_eq!(e(-0.00025, true, Some(1)), "-2.5E-04");
        assert_eq!(e(1e100, false, Some(0)), "1e+100");
        assert_eq!(e(5e-324, false, Some(2)), "4.94e-324");
    }

    #[test]
    fn float_exponent_alternate_form() {
        let alternate = Flags::only(Flag::Alternate);
        assert_eq!(
            float(1.0, FloatStyle::Exponent, false, alternate, 0, Some(0)),
            "1.e+00"
        );
    }

    #[test]
    fn float_general() {
        let g = |value, precision| float(value, FloatStyle::General, false, NO_FLAGS, 0, precision);
        assert_eq!(g(0.0, None), "0");
        assert_eq!(g(1.25, None), "1.25");
        assert_eq!(g(-1000.0, None), "-1000");
        assert_eq!(g(0.000025, None), "2.5e-05");
        assert_eq!(g(0.0001, None), "0.0001");
        assert_eq!(g(123456.0, None), "123456");
        assert_eq!(g(1234567.0, None), "1.23457e+06");
        assert_eq!(g(999999.5, None), "1e+06");
        assert_eq!(g(-1000.0, Some(3)), "-1e+03");
        assert_eq!(g(1.5, Some(0)), "2");
    }

    #[test]
    fn float_general_alternate_form() {
        let alternate = Flags::only(Flag::Alternate);
        let g = |value, precision| float(value, FloatStyle::General, true, alternate, 0, precision);
        assert_eq!(g(1.0, Some(3)), "1.00");
        assert_eq!(g(-1000.0, Some(3)), "-1.00E+03");
        assert_eq!(g(0.000025, Some(3)), "2.50E-05");
    }

    #[test]
    fn float_padding() {
        let zero = Flag::Zero | Flag::Plus;
        let left = Flags::only(Flag::Left);
        assert_eq!(
            float(1.0, FloatStyle::Fixed, false, zero, 15, None),
            "+0000001.000000"
        );
        assert_eq!(
            float(-1.0, FloatStyle::Exponent, false, left, 15, None),
            "-1.000000e+00  "
        );
    }

    #[test]
    fn float_infinity_and_nan() {
        let zero = Flags::only(Flag::Zero);
        assert_eq!(
            float(f64::INFINITY, FloatStyle::Fixed, false, zero, 5, None),
            "  inf"
        );
        assert_eq!(
            float(
                f64::NEG_INFINITY,
                FloatStyle::Exponent,
                true,
                NO_FLAGS,
                0,
                None
            ),
            "-INF"
        );
        assert_eq!(
            float(f64::NAN, FloatStyle::General, false, NO_FLAGS, 0, None),
            "nan"
        );
    }
}
