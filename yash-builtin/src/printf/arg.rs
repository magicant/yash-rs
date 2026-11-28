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

//! Conversion of arguments to numbers
//!
//! POSIX requires the arguments to be converted as if by `strtol` and `strtod`
//! in the C standard library. Each function in this module returns the
//! converted value along with an error, if any, because the built-in uses
//! the value even when the argument is not completely converted.

/// Error in converting an argument to a number
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumberError {
    /// The argument is not completely converted.
    Invalid,
    /// The value is out of the representable range.
    OutOfRange,
}

/// Removes leading white-space characters as `strtol` and `strtod` do.
pub fn skip_spaces(s: &str) -> &str {
    s.trim_start_matches([' ', '\t', '\n', '\x0B', '\x0C', '\r'])
}

/// Splits the leading sign off `s`.
///
/// Returns whether the sign is negative and the rest of `s`.
fn split_sign(s: &str) -> (bool, &str) {
    match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    }
}

/// Converts an argument to the magnitude of an integer.
///
/// Returns whether the integer is negative, its magnitude, and an error, if
/// any. The magnitude saturates at `u64::MAX` on overflow. The error is
/// `OutOfRange` if and only if the magnitude overflows, even when the argument
/// is not completely converted.
fn parse_magnitude(arg: &str) -> (bool, u64, Option<NumberError>) {
    // A leading quote makes the value of the following character.
    if let Some(rest) = arg.strip_prefix(['\'', '"']) {
        let mut chars = rest.chars();
        let value = chars.next().map_or(0, u64::from);
        let error = (!chars.as_str().is_empty()).then_some(NumberError::Invalid);
        return (false, value, error);
    }

    let (negative, unsigned) = split_sign(skip_spaces(arg));
    let hex = unsigned
        .strip_prefix("0x")
        .or_else(|| unsigned.strip_prefix("0X"))
        .filter(|digits| digits.starts_with(|c: char| c.is_ascii_hexdigit()));
    let (radix, digits) = match hex {
        Some(digits) => (16, digits),
        None if unsigned.starts_with('0') => (8, unsigned),
        None => (10, unsigned),
    };
    let end = digits
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(digits.len());
    if end == 0 {
        return (negative, 0, Some(NumberError::Invalid));
    }

    let magnitude = digits[..end].chars().try_fold(0u64, |value, c| {
        let digit = c.to_digit(radix).unwrap();
        value.checked_mul(radix.into())?.checked_add(digit.into())
    });
    // `parse_unsigned` relies on `OutOfRange` taking precedence to tell an
    // overflow from a magnitude of exactly `u64::MAX`.
    let error = if magnitude.is_none() {
        Some(NumberError::OutOfRange)
    } else if end < digits.len() {
        Some(NumberError::Invalid)
    } else {
        None
    };
    (negative, magnitude.unwrap_or(u64::MAX), error)
}

/// Converts an argument to a signed integer.
pub fn parse_signed(arg: &str) -> (i64, Option<NumberError>) {
    let (negative, magnitude, error) = parse_magnitude(arg);
    let limit = i64::MAX.unsigned_abs() + u64::from(negative);
    if magnitude > limit {
        let value = if negative { i64::MIN } else { i64::MAX };
        return (value, Some(NumberError::OutOfRange));
    }
    let value = magnitude as i64;
    (
        if negative {
            value.wrapping_neg()
        } else {
            value
        },
        error,
    )
}

/// Converts an argument to an unsigned integer.
///
/// As in `strtoumax`, a negative value wraps around.
pub fn parse_unsigned(arg: &str) -> (u64, Option<NumberError>) {
    let (negative, magnitude, error) = parse_magnitude(arg);
    if error == Some(NumberError::OutOfRange) {
        return (u64::MAX, error);
    }
    (
        if negative {
            magnitude.wrapping_neg()
        } else {
            magnitude
        },
        error,
    )
}

/// Returns the length of the longest prefix of `s` that matches `word`
/// ignoring case, or zero if `s` does not start with `word`.
fn match_ignore_case(s: &str, word: &str) -> usize {
    match s.get(..word.len()) {
        Some(prefix) if prefix.eq_ignore_ascii_case(word) => word.len(),
        _ => 0,
    }
}

/// Returns the length of the infinity or NaN notation at the start of `s`.
fn non_finite_len(s: &str) -> usize {
    let len = match_ignore_case(s, "infinity");
    if len > 0 {
        return len;
    }
    let len = match_ignore_case(s, "inf");
    if len > 0 {
        return len;
    }
    let len = match_ignore_case(s, "nan");
    if len == 0 {
        return 0;
    }
    // A NaN may be followed by a parenthesized sequence of alphanumerics.
    let Some(rest) = s[len..].strip_prefix('(') else {
        return len;
    };
    let inner = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    if rest[inner..].starts_with(')') {
        len + inner + 2
    } else {
        len
    }
}

/// Returns the length of the decimal floating-point notation at the start of
/// `s`.
fn decimal_len(s: &str) -> usize {
    let digits_len = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let integer = digits_len(s);
    let mut len = integer;
    if s[len..].starts_with('.') {
        let fraction = digits_len(&s[len + 1..]);
        if integer == 0 && fraction == 0 {
            return 0;
        }
        len += 1 + fraction;
    } else if integer == 0 {
        return 0;
    }

    // The exponent is part of the notation only if it has digits.
    if s[len..].starts_with(['e', 'E']) {
        let (_, exponent) = split_sign(&s[len + 1..]);
        let exponent_digits = digits_len(exponent);
        if exponent_digits > 0 {
            len = s.len() - exponent.len() + exponent_digits;
        }
    }
    len
}

/// Multiplies `value` by two to the power of `exponent`.
fn scale_by_power_of_two(mut value: f64, mut exponent: i64) -> f64 {
    // Scaling in steps keeps each factor representable.
    const STEP: i32 = 1000;
    while exponent > STEP.into() && value.is_finite() {
        value *= 2f64.powi(STEP);
        exponent -= i64::from(STEP);
    }
    while exponent < (-STEP).into() && value != 0.0 {
        value *= 2f64.powi(-STEP);
        exponent += i64::from(STEP);
    }
    value * 2f64.powi(exponent as i32)
}

/// Parses the hexadecimal floating-point notation after the `0x` prefix.
///
/// Returns the value, the length of the notation, and whether the value
/// underflows, or `None` if there is no hexadecimal digit.
fn parse_hex_float(s: &str) -> Option<(f64, usize, bool)> {
    // Digits beyond what fits in the mantissa only affect rounding, so they
    // are folded into a sticky bit.
    const MANTISSA_LIMIT: u64 = 1 << 60;
    let mut mantissa = 0u64;
    let mut sticky = false;
    let mut exponent = 0i64;
    let mut has_digit = false;
    let mut chars = s.char_indices().peekable();
    let mut push_digit = |digit: u32, exponent_delta: i64, exponent: &mut i64| {
        has_digit = true;
        if mantissa < MANTISSA_LIMIT {
            mantissa = mantissa * 16 + u64::from(digit);
            *exponent -= exponent_delta;
        } else {
            sticky |= digit != 0;
            *exponent += 4 - exponent_delta;
        }
    };
    while let Some(digit) = chars.next_if_map(|(i, c)| c.to_digit(16).ok_or((i, c))) {
        push_digit(digit, 0, &mut exponent);
    }
    if chars.next_if(|&(_, c)| c == '.').is_some() {
        while let Some(digit) = chars.next_if_map(|(i, c)| c.to_digit(16).ok_or((i, c))) {
            push_digit(digit, 4, &mut exponent);
        }
    }
    if !has_digit {
        return None;
    }
    let mut len = chars.peek().map_or(s.len(), |&(i, _)| i);

    if s[len..].starts_with(['p', 'P']) {
        let (negative, digits) = split_sign(&s[len + 1..]);
        let digits_len = digits
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(digits.len());
        if digits_len > 0 {
            let value = digits[..digits_len].bytes().fold(0i64, |value, b| {
                value.saturating_mul(10).saturating_add(i64::from(b - b'0'))
            });
            exponent = exponent.saturating_add(if negative { -value } else { value });
            len = s.len() - digits.len() + digits_len;
        }
    }

    // The exponent from the digits is at most four times their count, so
    // neither the saturation above nor this clamping affects a result other
    // than zero or infinity. Clamping keeps the arithmetic in `round_to_f64`
    // from overflowing.
    let exponent = exponent.clamp(-(1 << 20), 1 << 20);
    let (value, underflow) = round_to_f64(mantissa | u64::from(sticky), exponent);
    Some((value, len, underflow))
}

/// Rounds `mantissa` times two to the power of `exponent` to the nearest `f64`,
/// ties to even.
///
/// Returns the result and whether it underflows.
fn round_to_f64(mantissa: u64, exponent: i64) -> (f64, bool) {
    const MIN_NORMAL_BIT: i64 = f64::MIN_EXP as i64 - 1;
    const MIN_SUBNORMAL_BIT: i64 = MIN_NORMAL_BIT + 1 - f64::MANTISSA_DIGITS as i64;
    if mantissa == 0 {
        return (0.0, false);
    }

    // Rounding the mantissa here, rather than in both the conversion to `f64`
    // and the scaling, avoids double rounding when the result is subnormal.
    let highest_bit = exponent + i64::from(u64::BITS - 1 - mantissa.leading_zeros());
    let lowest_bit = (highest_bit + 1 - f64::MANTISSA_DIGITS as i64).max(MIN_SUBNORMAL_BIT);
    let shift = lowest_bit - exponent;
    if shift <= 0 {
        return (scale_by_power_of_two(mantissa as f64, exponent), false);
    }
    let (rounded, inexact) = if shift > 64 {
        (0, true)
    } else {
        let mantissa = u128::from(mantissa);
        let kept = mantissa >> shift;
        let dropped = mantissa - (kept << shift);
        let half = 1 << (shift - 1);
        let round_up = dropped > half || (dropped == half && kept % 2 == 1);
        (kept + u128::from(round_up), dropped != 0)
    };

    // As in IEEE 754, the result underflows if the exact value is below the
    // smallest normal number and the result is inexact.
    let underflow = inexact && highest_bit < MIN_NORMAL_BIT;
    (scale_by_power_of_two(rounded as f64, lowest_bit), underflow)
}

/// Converts an argument to a floating-point number.
pub fn parse_float(arg: &str) -> (f64, Option<NumberError>) {
    let trimmed = skip_spaces(arg);
    let (negative, unsigned) = split_sign(trimmed);
    let sign_len = trimmed.len() - unsigned.len();

    let non_finite = non_finite_len(unsigned);
    let (magnitude, len, underflow) = if non_finite > 0 {
        let value = if unsigned[..1].eq_ignore_ascii_case("n") {
            f64::NAN
        } else {
            f64::INFINITY
        };
        (value, non_finite, false)
    } else {
        let hex = unsigned
            .strip_prefix("0x")
            .or_else(|| unsigned.strip_prefix("0X"))
            .and_then(parse_hex_float);
        match hex {
            Some((value, len, underflow)) => (value, 2 + len, underflow),
            None => {
                let len = decimal_len(unsigned);
                if len == 0 {
                    return (0.0, Some(NumberError::Invalid));
                }
                let value: f64 = unsigned[..len].parse().unwrap();
                // A decimal number exactly representable as a subnormal
                // number has hundreds of digits, so any nonzero value below
                // the smallest normal number is taken as an underflow.
                let nonzero = unsigned[..len]
                    .bytes()
                    .take_while(|b| !b.eq_ignore_ascii_case(&b'e'))
                    .any(|b| matches!(b, b'1'..=b'9'));
                (value, len, nonzero && value < f64::MIN_POSITIVE)
            }
        }
    };

    let value = if negative { -magnitude } else { magnitude };
    let error = if sign_len + len < trimmed.len() {
        Some(NumberError::Invalid)
    } else if underflow || (non_finite == 0 && value.is_infinite()) {
        Some(NumberError::OutOfRange)
    } else {
        None
    };
    (value, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_signed_decimal() {
        assert_eq!(parse_signed("0"), (0, None));
        assert_eq!(parse_signed("123"), (123, None));
        assert_eq!(parse_signed("+45"), (45, None));
        assert_eq!(parse_signed("-678"), (-678, None));
    }

    #[test]
    fn parse_signed_octal_and_hexadecimal() {
        assert_eq!(parse_signed("010"), (8, None));
        assert_eq!(parse_signed("0x1fE"), (510, None));
        assert_eq!(parse_signed("-0X10"), (-16, None));
    }

    #[test]
    fn parse_signed_skips_leading_spaces() {
        assert_eq!(parse_signed(" \t\n12"), (12, None));
    }

    #[test]
    fn parse_signed_character_value() {
        assert_eq!(parse_signed("'a"), (97, None));
        assert_eq!(parse_signed("\"@"), (64, None));
        assert_eq!(parse_signed("'あ"), (0x3042, None));
        assert_eq!(parse_signed("'"), (0, None));
    }

    #[test]
    fn parse_signed_character_value_with_redundant_character() {
        assert_eq!(parse_signed("'45"), (52, Some(NumberError::Invalid)));
    }

    #[test]
    fn parse_signed_incompletely_converted() {
        assert_eq!(parse_signed("5a"), (5, Some(NumberError::Invalid)));
        assert_eq!(parse_signed("08"), (0, Some(NumberError::Invalid)));
        assert_eq!(parse_signed("0x"), (0, Some(NumberError::Invalid)));
        assert_eq!(parse_signed("12 "), (12, Some(NumberError::Invalid)));
    }

    #[test]
    fn parse_signed_not_a_number() {
        assert_eq!(parse_signed(""), (0, Some(NumberError::Invalid)));
        assert_eq!(parse_signed("foo"), (0, Some(NumberError::Invalid)));
        assert_eq!(parse_signed("-"), (0, Some(NumberError::Invalid)));
    }

    #[test]
    fn parse_signed_limits() {
        assert_eq!(parse_signed("9223372036854775807"), (i64::MAX, None));
        assert_eq!(parse_signed("-9223372036854775808"), (i64::MIN, None));
    }

    #[test]
    fn parse_signed_out_of_range() {
        assert_eq!(
            parse_signed("9223372036854775808"),
            (i64::MAX, Some(NumberError::OutOfRange))
        );
        assert_eq!(
            parse_signed("-99999999999999999999"),
            (i64::MIN, Some(NumberError::OutOfRange))
        );
    }

    #[test]
    fn parse_signed_out_of_range_with_redundant_character() {
        assert_eq!(
            parse_signed("9223372036854775808x"),
            (i64::MAX, Some(NumberError::OutOfRange))
        );
        assert_eq!(
            parse_signed("-99999999999999999999x"),
            (i64::MIN, Some(NumberError::OutOfRange))
        );
    }

    #[test]
    fn parse_unsigned_wraps_negative_value() {
        assert_eq!(parse_unsigned("-1"), (u64::MAX, None));
    }

    #[test]
    fn parse_unsigned_limits() {
        assert_eq!(parse_unsigned("18446744073709551615"), (u64::MAX, None));
        assert_eq!(
            parse_unsigned("18446744073709551616"),
            (u64::MAX, Some(NumberError::OutOfRange))
        );
        assert_eq!(
            parse_unsigned("-18446744073709551616"),
            (u64::MAX, Some(NumberError::OutOfRange))
        );
    }

    #[test]
    fn parse_unsigned_out_of_range_with_redundant_character() {
        assert_eq!(
            parse_unsigned("-18446744073709551616x"),
            (u64::MAX, Some(NumberError::OutOfRange))
        );
    }

    #[test]
    fn parse_float_decimal() {
        assert_eq!(parse_float("0"), (0.0, None));
        assert_eq!(parse_float("1.25"), (1.25, None));
        assert_eq!(parse_float("-.5"), (-0.5, None));
        assert_eq!(parse_float("+3."), (3.0, None));
        assert_eq!(parse_float(" 1e3"), (1000.0, None));
        assert_eq!(parse_float("25E-6"), (0.000025, None));
    }

    #[test]
    fn parse_float_ignores_exponent_without_digits() {
        assert_eq!(parse_float("1e"), (1.0, Some(NumberError::Invalid)));
        assert_eq!(parse_float("1e+"), (1.0, Some(NumberError::Invalid)));
    }

    #[test]
    fn parse_float_hexadecimal() {
        assert_eq!(parse_float("0x1p1"), (2.0, None));
        assert_eq!(parse_float("0x.8"), (0.5, None));
        assert_eq!(parse_float("-0XAP-2"), (-2.5, None));
        assert_eq!(parse_float("0x1.8p"), (1.5, Some(NumberError::Invalid)));
    }

    #[test]
    fn parse_float_hexadecimal_with_many_digits() {
        assert_eq!(
            parse_float("0x10000000000000000"),
            (18446744073709551616.0, None)
        );
        assert_eq!(
            parse_float("0x1.00000000000008000000001p0").0,
            1.0 + f64::EPSILON
        );
    }

    #[test]
    fn parse_float_hexadecimal_exponent_cancels_many_digits() {
        let zeros = "0".repeat(262_145);
        assert_eq!(parse_float(&format!("0x1{zeros}p-1048580")), (1.0, None));
        assert_eq!(
            parse_float(&format!("0x0.{}1p+1048580", &zeros[1..])),
            (1.0, None)
        );
        assert_eq!(
            parse_float(&format!("0x1{zeros}p-9999999999999999999999")),
            (0.0, Some(NumberError::OutOfRange))
        );
    }

    #[test]
    fn parse_float_prefix_without_hexadecimal_digits() {
        assert_eq!(parse_float("0x"), (0.0, Some(NumberError::Invalid)));
    }

    #[test]
    fn parse_float_infinity_and_nan() {
        assert_eq!(parse_float("inf"), (f64::INFINITY, None));
        assert_eq!(parse_float("-Infinity"), (f64::NEG_INFINITY, None));
        let (value, error) = parse_float("NaN(123)");
        assert!(value.is_nan());
        assert_eq!(error, None);
        assert_eq!(
            parse_float("infinit"),
            (f64::INFINITY, Some(NumberError::Invalid))
        );
    }

    #[test]
    fn parse_float_not_a_number() {
        assert_eq!(parse_float(""), (0.0, Some(NumberError::Invalid)));
        assert_eq!(parse_float("."), (0.0, Some(NumberError::Invalid)));
        assert_eq!(parse_float("'a"), (0.0, Some(NumberError::Invalid)));
    }

    #[test]
    fn parse_float_out_of_range() {
        assert_eq!(
            parse_float("1e400"),
            (f64::INFINITY, Some(NumberError::OutOfRange))
        );
        assert_eq!(
            parse_float("-0x1p2000"),
            (f64::NEG_INFINITY, Some(NumberError::OutOfRange))
        );
    }

    #[test]
    fn parse_float_decimal_underflow() {
        assert_eq!(parse_float("1e-999"), (0.0, Some(NumberError::OutOfRange)));
        assert_eq!(
            parse_float("-1e-310"),
            (-1e-310, Some(NumberError::OutOfRange))
        );
        assert_eq!(parse_float("0e-999"), (0.0, None));
        assert_eq!(
            parse_float("2.2250738585072014e-308"),
            (f64::MIN_POSITIVE, None)
        );
    }

    #[test]
    fn parse_float_hexadecimal_underflow() {
        assert_eq!(
            parse_float("0x1p-1080"),
            (0.0, Some(NumberError::OutOfRange))
        );
        assert_eq!(
            parse_float("0x1.8p-1074"),
            (2.0 * 5e-324, Some(NumberError::OutOfRange))
        );
        assert_eq!(
            parse_float("0x1.fffffffffffffp-1023"),
            (f64::MIN_POSITIVE, Some(NumberError::OutOfRange))
        );
        assert_eq!(parse_float("0x0p-2000"), (0.0, None));
        assert_eq!(parse_float("0x1p-1074"), (5e-324, None));
        assert_eq!(parse_float("0x1p-1022"), (f64::MIN_POSITIVE, None));
    }

    #[test]
    fn parse_float_hexadecimal_subnormal_rounds_once() {
        // Rounding to 53 bits first would make this a tie that rounds to zero.
        assert_eq!(
            parse_float("0x1.000000000000002p-1075"),
            (5e-324, Some(NumberError::OutOfRange))
        );
    }
}
