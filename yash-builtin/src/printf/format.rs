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

//! Parser for the format operand

use enumset::{EnumSet, EnumSetType};
use itertools::Itertools as _;
use std::ffi::c_int;
use std::iter::Peekable;
use std::str::Chars;
use thiserror::Error;

/// Largest field width or precision accepted
///
/// This is `INT_MAX`. The C `printf` function takes a field width or precision
/// given by `*` as an `int`, so a C implementation cannot accept larger values
/// that way.
pub const MAX_COUNT: usize = c_int::MAX as usize;

/// Flag of a conversion specification
#[derive(Debug, EnumSetType)]
pub enum Flag {
    /// `-`: pad on the right instead of the left
    Left,
    /// `+`: always print the sign of a signed number
    Plus,
    /// ` `: print a space in place of a plus sign
    Space,
    /// `#`: use the alternative form
    Alternate,
    /// `0`: pad numbers with zeros instead of spaces
    Zero,
}

impl Flag {
    /// Returns the flag that the character represents.
    fn from_char(c: char) -> Option<Self> {
        match c {
            '-' => Some(Self::Left),
            '+' => Some(Self::Plus),
            ' ' => Some(Self::Space),
            '#' => Some(Self::Alternate),
            '0' => Some(Self::Zero),
            _ => None,
        }
    }
}

/// Set of flags of a conversion specification
pub type Flags = EnumSet<Flag>;

/// Field width or precision of a conversion specification
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Count {
    /// Number written in the format
    Fixed(usize),
    /// Number taken from an argument, written as `*` or `*n$`
    ///
    /// The value is the zero-based index of the argument for `*n$`, or `None`
    /// for `*`.
    Argument(Option<usize>),
}

/// Base of an unsigned integer conversion
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Radix {
    /// `o`
    Octal,
    /// `u`
    Decimal,
    /// `x`
    LowerHex,
    /// `X`
    UpperHex,
}

/// Notation of a floating-point conversion
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FloatStyle {
    /// `f` and `F`
    Fixed,
    /// `e` and `E`
    Exponent,
    /// `g` and `G`
    General,
}

/// What a conversion specification converts its argument into
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// `d` and `i`
    Signed,
    /// `o`, `u`, `x`, and `X`
    Unsigned(Radix),
    /// `e`, `E`, `f`, `F`, `g`, and `G`
    Float { style: FloatStyle, upper: bool },
    /// `c`
    Char,
    /// `s`
    String,
    /// `b`
    Escaped,
    /// `q`
    Quoted,
}

/// Conversion specification
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Conversion {
    /// Zero-based index of the argument given as `n$`
    pub position: Option<usize>,
    pub flags: Flags,
    pub width: Option<Count>,
    pub precision: Option<Count>,
    pub kind: Kind,
}

impl Conversion {
    /// Returns the argument positions this conversion consumes.
    ///
    /// Each item is `Some` index for a numbered argument and `None` for an
    /// unnumbered one.
    fn positions(&self) -> impl Iterator<Item = Option<usize>> {
        [self.width, self.precision]
            .into_iter()
            .filter_map(|count| match count {
                Some(Count::Argument(position)) => Some(position),
                _ => None,
            })
            .chain([self.position])
    }
}

/// Part of a parsed format
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Segment {
    /// Bytes printed as they are
    Literal(Vec<u8>),
    /// Conversion specification that formats arguments
    Conversion(Conversion),
}

/// Feature of the format that POSIX does not specify
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Extension {
    /// Backslash followed by the character, or `None` if the backslash ends the
    /// format
    Escape(Option<char>),
    /// `*` for a field width or precision
    Asterisk,
    /// Conversion specifier
    Conversion(char),
    /// Mixture of numbered and unnumbered argument conversions
    MixedNumbering,
    /// Precision for the `c` conversion specifier
    CharPrecision,
    /// Octal escape sequence whose value does not fit in a byte
    LargeOctal,
}

/// Parsed format operand
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Format {
    pub segments: Vec<Segment>,
    /// First feature of the format that POSIX does not specify
    pub extension: Option<Extension>,
}

/// Error in the format operand
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum FormatError {
    #[error("the conversion specification is incomplete")]
    Incomplete,
    #[error("{0:?} is not a valid conversion specifier")]
    InvalidSpecifier(char),
    #[error("flag {flag:?} cannot be used with conversion specifier {specifier:?}")]
    InvalidFlag { flag: char, specifier: char },
    #[error("`%%` cannot have a position, flags, field width, or precision")]
    DecoratedPercent,
    #[error("the argument position must be positive")]
    ZeroPosition,
    #[error("the number is too large")]
    TooLarge,
    #[error("U+{0:X} is not a valid character")]
    InvalidCharacter(u32),
}

/// Parses a decimal number.
///
/// Returns `None` if there is no digit.
fn parse_number(chars: &mut Peekable<Chars>) -> Result<Option<usize>, FormatError> {
    let mut number = None;
    while let Some(digit) = chars.next_if_map(|c| c.to_digit(10).ok_or(c)) {
        number = Some(
            number
                .unwrap_or(0usize)
                .checked_mul(10)
                .and_then(|n| n.checked_add(digit as usize))
                .filter(|&n| n <= MAX_COUNT)
                .ok_or(FormatError::TooLarge)?,
        );
    }
    Ok(number)
}

/// Parses an argument position (`n$`) if any.
///
/// Returns the zero-based index of the argument.
fn parse_position(chars: &mut Peekable<Chars>) -> Result<Option<usize>, FormatError> {
    let mut lookahead = chars.clone();
    let Some(number) = parse_number(&mut lookahead)? else {
        return Ok(None);
    };
    if lookahead.next_if_eq(&'$').is_none() {
        return Ok(None);
    }
    *chars = lookahead;
    match number.checked_sub(1) {
        Some(index) => Ok(Some(index)),
        None => Err(FormatError::ZeroPosition),
    }
}

/// Parses a field width or precision, which may be `*` or `*n$`.
fn parse_count(
    chars: &mut Peekable<Chars>,
    extension: &mut Option<Extension>,
) -> Result<Option<Count>, FormatError> {
    if chars.next_if_eq(&'*').is_some() {
        extension.get_or_insert(Extension::Asterisk);
        return Ok(Some(Count::Argument(parse_position(chars)?)));
    }
    Ok(parse_number(chars)?.map(Count::Fixed))
}

/// Parses a conversion specification after the leading `%`.
fn parse_conversion(
    chars: &mut Peekable<Chars>,
    extension: &mut Option<Extension>,
) -> Result<Conversion, FormatError> {
    let position = parse_position(chars)?;

    let mut flags = Flags::empty();
    while let Some(flag) = chars.next_if_map(|c| Flag::from_char(c).ok_or(c)) {
        flags |= flag;
    }

    let width = parse_count(chars, extension)?;
    let precision = if chars.next_if_eq(&'.').is_some() {
        Some(parse_count(chars, extension)?.unwrap_or(Count::Fixed(0)))
    } else {
        None
    };

    let specifier = chars.next().ok_or(FormatError::Incomplete)?;
    let kind = match specifier {
        'd' | 'i' => Kind::Signed,
        'o' => Kind::Unsigned(Radix::Octal),
        'u' => Kind::Unsigned(Radix::Decimal),
        'x' => Kind::Unsigned(Radix::LowerHex),
        'X' => Kind::Unsigned(Radix::UpperHex),
        'e' | 'E' | 'f' | 'F' | 'g' | 'G' => {
            let style = match specifier.to_ascii_lowercase() {
                'e' => FloatStyle::Exponent,
                'f' => FloatStyle::Fixed,
                _ => FloatStyle::General,
            };
            let upper = specifier.is_ascii_uppercase();
            Kind::Float { style, upper }
        }
        'c' => Kind::Char,
        's' => Kind::String,
        'b' => Kind::Escaped,
        'q' => Kind::Quoted,
        '%' => return Err(FormatError::DecoratedPercent),
        _ => return Err(FormatError::InvalidSpecifier(specifier)),
    };

    // The C standard leaves the results of these flags undefined.
    let alternate_allowed = matches!(
        kind,
        Kind::Unsigned(Radix::Octal | Radix::LowerHex | Radix::UpperHex) | Kind::Float { .. }
    );
    if !alternate_allowed && flags.contains(Flag::Alternate) {
        let flag = '#';
        return Err(FormatError::InvalidFlag { flag, specifier });
    }
    let zero_allowed = matches!(kind, Kind::Signed | Kind::Unsigned(_) | Kind::Float { .. });
    if !zero_allowed && flags.contains(Flag::Zero) {
        let flag = '0';
        return Err(FormatError::InvalidFlag { flag, specifier });
    }

    if matches!(kind, Kind::Float { .. } | Kind::Quoted) {
        extension.get_or_insert(Extension::Conversion(specifier));
    }
    if kind == Kind::Char && precision.is_some() {
        extension.get_or_insert(Extension::CharPrecision);
    }

    Ok(Conversion {
        position,
        flags,
        width,
        precision,
        kind,
    })
}

/// Parses up to `max_digits` hexadecimal digits.
///
/// Returns `None` if there is no digit.
fn parse_hex_digits(chars: &mut Peekable<Chars>, max_digits: usize) -> Option<u32> {
    (0..max_digits)
        .map_while(|_| chars.next_if_map(|c| c.to_digit(16).ok_or(c)))
        .reduce(|value, digit| value * 16 + digit)
}

/// Parses an escape sequence after the leading backslash and appends the
/// result to `out`.
fn parse_escape(
    chars: &mut Peekable<Chars>,
    out: &mut Vec<u8>,
    extension: &mut Option<Extension>,
) -> Result<(), FormatError> {
    let Some(c) = chars.next() else {
        extension.get_or_insert(Extension::Escape(None));
        out.push(b'\\');
        return Ok(());
    };

    let byte = match c {
        '\\' => b'\\',
        'a' => 0x07,
        'b' => 0x08,
        'f' => 0x0C,
        'n' => b'\n',
        'r' => b'\r',
        't' => b'\t',
        'v' => 0x0B,
        '0'..='7' => {
            let first = c.to_digit(8).unwrap();
            let value = (0..2)
                .map_while(|_| chars.next_if_map(|c| c.to_digit(8).ok_or(c)))
                .fold(first, |value, digit| value * 8 + digit);
            // POSIX specifies a one-byte value, so excess bits are discarded.
            if value > 0o377 {
                extension.get_or_insert(Extension::LargeOctal);
            }
            value as u8
        }
        _ => {
            extension.get_or_insert(Extension::Escape(Some(c)));
            return parse_extended_escape(chars, c, out);
        }
    };
    out.push(byte);
    Ok(())
}

/// Parses an escape sequence that POSIX does not specify and appends the
/// result to `out`.
///
/// `c` is the character following the backslash.
fn parse_extended_escape(
    chars: &mut Peekable<Chars>,
    c: char,
    out: &mut Vec<u8>,
) -> Result<(), FormatError> {
    let max_digits = match c {
        '"' | '\'' => {
            out.push(c as u8);
            return Ok(());
        }
        'e' => {
            out.push(0x1B);
            return Ok(());
        }
        'x' => 2,
        'u' => 4,
        'U' => 8,
        _ => 0,
    };
    match parse_hex_digits(chars, max_digits) {
        // An unknown escape sequence is printed as is.
        None => {
            out.push(b'\\');
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        }
        Some(value) if c == 'x' => out.push(value as u8),
        Some(value) => {
            let c = char::from_u32(value).ok_or(FormatError::InvalidCharacter(value))?;
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        }
    }
    Ok(())
}

/// Tests whether the format mixes numbered and unnumbered argument
/// conversions.
fn mixes_numbering(segments: &[Segment]) -> bool {
    !segments
        .iter()
        .filter_map(|segment| match segment {
            Segment::Conversion(conversion) => Some(conversion.positions()),
            Segment::Literal(_) => None,
        })
        .flatten()
        .map(|position| position.is_some())
        .all_equal()
}

/// Parses the format operand.
pub fn parse(format: &str) -> Result<Format, FormatError> {
    let mut chars = format.chars().peekable();
    let mut segments = Vec::new();
    let mut literal = Vec::new();
    let mut extension = None;

    while let Some(c) = chars.next() {
        match c {
            '\\' => parse_escape(&mut chars, &mut literal, &mut extension)?,
            '%' if chars.next_if_eq(&'%').is_some() => literal.push(b'%'),
            '%' => {
                let conversion = parse_conversion(&mut chars, &mut extension)?;
                if !literal.is_empty() {
                    segments.push(Segment::Literal(std::mem::take(&mut literal)));
                }
                segments.push(Segment::Conversion(conversion));
            }
            _ => literal.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }
    if !literal.is_empty() {
        segments.push(Segment::Literal(literal));
    }

    if mixes_numbering(&segments) {
        extension.get_or_insert(Extension::MixedNumbering);
    }

    Ok(Format {
        segments,
        extension,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: Conversion = Conversion {
        position: None,
        flags: Flags::empty(),
        width: None,
        precision: None,
        kind: Kind::String,
    };

    #[test]
    fn parse_literal() {
        let format = parse("foo 日本").unwrap();

        assert_eq!(
            format.segments,
            [Segment::Literal("foo 日本".as_bytes().to_vec())]
        );
        assert_eq!(format.extension, None);
    }

    #[test]
    fn parse_empty() {
        let format = parse("").unwrap();

        assert_eq!(format.segments, []);
    }

    #[test]
    fn parse_posix_escapes() {
        let format = parse(r"\\\a\b\f\n\r\t\v").unwrap();

        assert_eq!(
            format.segments,
            [Segment::Literal(b"\\\x07\x08\x0C\n\r\t\x0B".to_vec())]
        );
        assert_eq!(format.extension, None);
    }

    #[test]
    fn parse_octal_escapes() {
        let format = parse(r"\0\11\1000\0101\777\8").unwrap();

        assert_eq!(
            format.segments,
            [Segment::Literal(b"\x00\x09\x400\x08\x31\xFF\\8".to_vec())]
        );
        assert_eq!(format.extension, Some(Extension::LargeOctal));
    }

    #[test]
    fn parse_extended_escapes() {
        let format = parse(r#"\"\'\e\x41\x7e7あ\U0001F600"#).unwrap();

        assert_eq!(
            format.segments,
            [Segment::Literal(
                "\"'\x1BA~7あ\u{1F600}".as_bytes().to_vec()
            )]
        );
        assert_eq!(format.extension, Some(Extension::Escape(Some('"'))));
    }

    #[test]
    fn parse_hexadecimal_escape_without_digits() {
        let format = parse(r"\xg\u\U").unwrap();

        assert_eq!(format.segments, [Segment::Literal(br"\xg\u\U".to_vec())]);
        assert_eq!(format.extension, Some(Extension::Escape(Some('x'))));
    }

    #[test]
    fn parse_unknown_escape() {
        let format = parse(r"\q\c").unwrap();

        assert_eq!(format.segments, [Segment::Literal(br"\q\c".to_vec())]);
        assert_eq!(format.extension, Some(Extension::Escape(Some('q'))));
    }

    #[test]
    fn parse_trailing_backslash() {
        let format = parse("a\\").unwrap();

        assert_eq!(format.segments, [Segment::Literal(br"a\".to_vec())]);
        assert_eq!(format.extension, Some(Extension::Escape(None)));
    }

    #[test]
    fn parse_invalid_unicode_escape() {
        assert_eq!(parse(r"\uD800"), Err(FormatError::InvalidCharacter(0xD800)));
        assert_eq!(
            parse(r"\U00110000"),
            Err(FormatError::InvalidCharacter(0x110000))
        );
    }

    #[test]
    fn parse_percent() {
        let format = parse("%%a%%").unwrap();

        assert_eq!(format.segments, [Segment::Literal(b"%a%".to_vec())]);
    }

    #[test]
    fn parse_conversion_between_literals() {
        let format = parse("[%s]").unwrap();

        assert_eq!(
            format.segments,
            [
                Segment::Literal(b"[".to_vec()),
                Segment::Conversion(PLAIN),
                Segment::Literal(b"]".to_vec()),
            ]
        );
        assert_eq!(format.extension, None);
    }

    #[test]
    fn parse_conversion_specifiers() {
        let format = parse("%d%i%o%u%x%X%c%s%b").unwrap();

        let kinds = format
            .segments
            .iter()
            .map(|segment| match segment {
                Segment::Conversion(conversion) => conversion.kind,
                Segment::Literal(_) => panic!("unexpected literal"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                Kind::Signed,
                Kind::Signed,
                Kind::Unsigned(Radix::Octal),
                Kind::Unsigned(Radix::Decimal),
                Kind::Unsigned(Radix::LowerHex),
                Kind::Unsigned(Radix::UpperHex),
                Kind::Char,
                Kind::String,
                Kind::Escaped,
            ]
        );
        assert_eq!(format.extension, None);
    }

    #[test]
    fn parse_float_conversion_specifiers() {
        let format = parse("%e%E%f%F%g%G").unwrap();

        let kinds = format
            .segments
            .iter()
            .map(|segment| match segment {
                Segment::Conversion(conversion) => conversion.kind,
                Segment::Literal(_) => panic!("unexpected literal"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                Kind::Float {
                    style: FloatStyle::Exponent,
                    upper: false
                },
                Kind::Float {
                    style: FloatStyle::Exponent,
                    upper: true
                },
                Kind::Float {
                    style: FloatStyle::Fixed,
                    upper: false
                },
                Kind::Float {
                    style: FloatStyle::Fixed,
                    upper: true
                },
                Kind::Float {
                    style: FloatStyle::General,
                    upper: false
                },
                Kind::Float {
                    style: FloatStyle::General,
                    upper: true
                },
            ]
        );
        assert_eq!(format.extension, Some(Extension::Conversion('e')));
    }

    #[test]
    fn parse_quote_conversion_specifier() {
        let format = parse("%q").unwrap();

        assert_eq!(
            format.segments,
            [Segment::Conversion(Conversion {
                kind: Kind::Quoted,
                ..PLAIN
            })]
        );
        assert_eq!(format.extension, Some(Extension::Conversion('q')));
    }

    #[test]
    fn parse_flags_width_and_precision() {
        let format = parse("%-+ #012.34x").unwrap();

        assert_eq!(
            format.segments,
            [Segment::Conversion(Conversion {
                flags: Flags::all(),
                width: Some(Count::Fixed(12)),
                precision: Some(Count::Fixed(34)),
                kind: Kind::Unsigned(Radix::LowerHex),
                ..PLAIN
            })]
        );
    }

    #[test]
    fn parse_precision_without_digits() {
        let format = parse("%.s").unwrap();

        assert_eq!(
            format.segments,
            [Segment::Conversion(Conversion {
                precision: Some(Count::Fixed(0)),
                ..PLAIN
            })]
        );
    }

    #[test]
    fn parse_position() {
        let format = parse("%2$s%10$05d").unwrap();

        assert_eq!(
            format.segments,
            [
                Segment::Conversion(Conversion {
                    position: Some(1),
                    ..PLAIN
                }),
                Segment::Conversion(Conversion {
                    position: Some(9),
                    flags: Flags::only(Flag::Zero),
                    width: Some(Count::Fixed(5)),
                    kind: Kind::Signed,
                    ..PLAIN
                }),
            ]
        );
    }

    #[test]
    fn parse_asterisks() {
        let format = parse("%*.*s").unwrap();

        assert_eq!(
            format.segments,
            [Segment::Conversion(Conversion {
                width: Some(Count::Argument(None)),
                precision: Some(Count::Argument(None)),
                ..PLAIN
            })]
        );
        assert_eq!(format.extension, Some(Extension::Asterisk));
    }

    #[test]
    fn parse_numbered_asterisks() {
        let format = parse("%3$*1$.*2$s").unwrap();

        assert_eq!(
            format.segments,
            [Segment::Conversion(Conversion {
                position: Some(2),
                width: Some(Count::Argument(Some(0))),
                precision: Some(Count::Argument(Some(1))),
                ..PLAIN
            })]
        );
    }

    #[test]
    fn parse_incomplete_conversion() {
        assert_eq!(parse("%"), Err(FormatError::Incomplete));
        assert_eq!(parse("%-5.2"), Err(FormatError::Incomplete));
    }

    #[test]
    fn parse_invalid_conversion_specifier() {
        assert_eq!(parse("%y"), Err(FormatError::InvalidSpecifier('y')));
        assert_eq!(parse("%ld"), Err(FormatError::InvalidSpecifier('l')));
        assert_eq!(parse("%$d"), Err(FormatError::InvalidSpecifier('$')));
        assert_eq!(parse("%あ"), Err(FormatError::InvalidSpecifier('あ')));
    }

    #[test]
    fn parse_invalid_flags() {
        assert_eq!(
            parse("%#d"),
            Err(FormatError::InvalidFlag {
                flag: '#',
                specifier: 'd'
            })
        );
        assert_eq!(
            parse("%#s"),
            Err(FormatError::InvalidFlag {
                flag: '#',
                specifier: 's'
            })
        );
        assert_eq!(
            parse("%05b"),
            Err(FormatError::InvalidFlag {
                flag: '0',
                specifier: 'b'
            })
        );
    }

    #[test]
    fn parse_decorated_percent() {
        assert_eq!(parse("%5%"), Err(FormatError::DecoratedPercent));
        assert_eq!(parse("%1$%"), Err(FormatError::DecoratedPercent));
    }

    #[test]
    fn parse_zero_position() {
        assert_eq!(parse("%0$d"), Err(FormatError::ZeroPosition));
        assert_eq!(parse("%*0$d"), Err(FormatError::ZeroPosition));
    }

    #[test]
    fn parse_too_large_number() {
        let format = format!("%{}d", MAX_COUNT + 1);
        assert_eq!(parse(&format), Err(FormatError::TooLarge));
        assert_eq!(parse("%.99999999999999999999d"), Err(FormatError::TooLarge));
    }

    #[test]
    fn parse_mixed_numbering() {
        for format in ["%1$s%s", "%s%1$s%%"] {
            let format = parse(format).unwrap();
            assert_eq!(format.extension, Some(Extension::MixedNumbering));
        }
    }

    #[test]
    fn parse_char_precision() {
        let format = parse("%5c%.1c").unwrap();
        assert_eq!(format.extension, Some(Extension::CharPrecision));
    }

    #[test]
    fn parse_large_octal_escape() {
        let format = parse(r"\377").unwrap();
        assert_eq!(format.extension, None);

        let format = parse(r"\400").unwrap();
        assert_eq!(format.segments, [Segment::Literal(vec![0])]);
        assert_eq!(format.extension, Some(Extension::LargeOctal));
    }

    #[test]
    fn parse_mixed_numbering_keeps_earlier_extension() {
        let format = parse("%1$f%s").unwrap();
        assert_eq!(format.extension, Some(Extension::Conversion('f')));
    }

    #[test]
    fn parse_percent_does_not_affect_numbering() {
        let format = parse("%1$s%%").unwrap();

        assert_eq!(
            format.segments,
            [
                Segment::Conversion(Conversion {
                    position: Some(0),
                    ..PLAIN
                }),
                Segment::Literal(b"%".to_vec()),
            ]
        );
    }
}
