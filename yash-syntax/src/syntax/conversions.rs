// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2020 WATANABE Yuki
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

use super::*;
use std::fmt;
use thiserror::Error;

/// Result of [`Unquote::write_unquoted`]
///
/// If there is some quotes to be removed, the result will be `Ok(true)`. If no
/// quotes, `Ok(false)`. On error, `Err(Error)`.
type UnquoteResult = Result<bool, fmt::Error>;

/// Removing quotes from syntax without performing expansion.
///
/// This trail will be useful only in a limited number of use cases. In the
/// normal word expansion process, quote removal is done after other kinds of
/// expansions like parameter expansion, so this trait is not used.
pub trait Unquote {
    /// Converts `self` to a string with all quotes removed and writes to `w`.
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult;

    /// Converts `self` to a string with all quotes removed.
    ///
    /// Returns a tuple of a string and a bool. The string is an unquoted version
    /// of `self`. The bool tells whether there is any quotes contained in
    /// `self`.
    fn unquote(&self) -> (String, bool) {
        let mut unquoted = String::new();
        let is_quoted = self
            .write_unquoted(&mut unquoted)
            .expect("`write_unquoted` should not fail");
        (unquoted, is_quoted)
    }
}

/// Error indicating that a syntax element is not a literal
///
/// This error value is returned by [`MaybeLiteral::extend_literal`] when the
/// syntax element is not a literal.
#[derive(Debug, Error)]
#[error("not a literal")]
pub struct NotLiteral;

/// Possibly literal syntax element
///
/// A syntax element is _literal_ if it is not quoted and does not contain any
/// expansions. Such an element may be considered as a constant string, and is
/// a candidate for a keyword or identifier.
///
/// ```
/// # use yash_syntax::syntax::MaybeLiteral;
/// # use yash_syntax::syntax::Text;
/// # use yash_syntax::syntax::TextUnit::Literal;
/// let text = Text(vec![Literal('f'), Literal('o'), Literal('o')]);
/// let expanded = text.to_string_if_literal().unwrap();
/// assert_eq!(expanded, "foo");
/// ```
///
/// ```
/// # use yash_syntax::syntax::MaybeLiteral;
/// # use yash_syntax::syntax::Text;
/// # use yash_syntax::syntax::TextUnit::Backslashed;
/// let backslashed = Text(vec![Backslashed('a')]);
/// assert_eq!(backslashed.to_string_if_literal(), None);
/// ```
pub trait MaybeLiteral {
    /// Appends the literal representation of `self` to an extendable object.
    ///
    /// If `self` is literal, the literal representation is appended to `result`
    /// and `Ok(())` is returned. Otherwise, `Err(NotLiteral)` is returned and
    /// `result` may contain some characters that have been appended.
    fn extend_literal<T: Extend<char>>(&self, result: &mut T) -> Result<(), NotLiteral>;

    /// Checks if `self` is literal and, if so, converts to a string.
    fn to_string_if_literal(&self) -> Option<String> {
        let mut result = String::new();
        self.extend_literal(&mut result).ok()?;
        Some(result)
    }
}

impl<T: Unquote> Unquote for [T] {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        self.iter()
            .try_fold(false, |quoted, item| Ok(quoted | item.write_unquoted(w)?))
    }
}

impl<T: MaybeLiteral> MaybeLiteral for [T] {
    fn extend_literal<R: Extend<char>>(&self, result: &mut R) -> Result<(), NotLiteral> {
        self.iter().try_for_each(|item| item.extend_literal(result))
    }
}

impl SpecialParam {
    /// Returns the character representing the special parameter.
    #[must_use]
    pub const fn as_char(self) -> char {
        use SpecialParam::*;
        match self {
            At => '@',
            Asterisk => '*',
            Number => '#',
            Question => '?',
            Hyphen => '-',
            Dollar => '$',
            Exclamation => '!',
            Zero => '0',
        }
    }

    /// Returns the special parameter that corresponds to the given character.
    ///
    /// If the character does not represent any special parameter, `None` is
    /// returned.
    #[must_use]
    pub const fn from_char(c: char) -> Option<SpecialParam> {
        use SpecialParam::*;
        match c {
            '@' => Some(At),
            '*' => Some(Asterisk),
            '#' => Some(Number),
            '?' => Some(Question),
            '-' => Some(Hyphen),
            '$' => Some(Dollar),
            '!' => Some(Exclamation),
            '0' => Some(Zero),
            _ => None,
        }
    }
}

/// Error that occurs when a character cannot be parsed as a special parameter
///
/// This error value is returned by the `TryFrom<char>` and `FromStr`
/// implementations for [`SpecialParam`].
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("not a special parameter")]
pub struct NotSpecialParam;

impl TryFrom<char> for SpecialParam {
    type Error = NotSpecialParam;
    fn try_from(c: char) -> Result<SpecialParam, NotSpecialParam> {
        SpecialParam::from_char(c).ok_or(NotSpecialParam)
    }
}

impl FromStr for SpecialParam {
    type Err = NotSpecialParam;
    fn from_str(s: &str) -> Result<SpecialParam, NotSpecialParam> {
        // If `s` contains a single character and nothing else, parse it as a
        // special parameter.
        let mut chars = s.chars();
        chars
            .next()
            .filter(|_| chars.as_str().is_empty())
            .and_then(SpecialParam::from_char)
            .ok_or(NotSpecialParam)
    }
}

impl From<SpecialParam> for ParamType {
    fn from(special: SpecialParam) -> ParamType {
        ParamType::Special(special)
    }
}

impl Param {
    /// Constructs a `Param` value representing a named parameter.
    ///
    /// This function assumes that the argument is a valid name for a variable.
    /// The returned `Param` value will have the `Variable` type regardless of
    /// the argument.
    #[must_use]
    pub fn variable<I: Into<String>>(id: I) -> Param {
        let id = id.into();
        let r#type = ParamType::Variable;
        Param { id, r#type }
    }
}

/// Constructs a `Param` value representing a special parameter.
impl From<SpecialParam> for Param {
    fn from(special: SpecialParam) -> Param {
        Param {
            id: special.to_string(),
            r#type: special.into(),
        }
    }
}

/// Constructs a `Param` value from a positional parameter index.
impl From<usize> for Param {
    fn from(index: usize) -> Param {
        Param {
            id: index.to_string(),
            r#type: ParamType::Positional(index),
        }
    }
}

impl Unquote for Switch {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        write!(w, "{}{}", self.condition, self.action)?;
        self.word.write_unquoted(w)
    }
}

impl Unquote for Trim {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        write!(w, "{}", self.side)?;
        match self.length {
            TrimLength::Shortest => (),
            TrimLength::Longest => write!(w, "{}", self.side)?,
        }
        self.pattern.write_unquoted(w)
    }
}

impl Unquote for BracedParam {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        use Modifier::*;
        match self.modifier {
            None => {
                write!(w, "${{{}}}", self.param)?;
                Ok(false)
            }
            Length => {
                write!(w, "${{#{}}}", self.param)?;
                Ok(false)
            }
            Switch(ref switch) => {
                write!(w, "${{{}", self.param)?;
                let quoted = switch.write_unquoted(w)?;
                w.write_char('}')?;
                Ok(quoted)
            }
            Trim(ref trim) => {
                write!(w, "${{{}", self.param)?;
                let quoted = trim.write_unquoted(w)?;
                w.write_char('}')?;
                Ok(quoted)
            }
        }
    }
}

impl Unquote for BackquoteUnit {
    fn write_unquoted<W: std::fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        match self {
            BackquoteUnit::Literal(c) => {
                w.write_char(*c)?;
                Ok(false)
            }
            BackquoteUnit::Backslashed(c) => {
                w.write_char(*c)?;
                Ok(true)
            }
        }
    }
}

impl Unquote for TextUnit {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        match self {
            Literal(c) => {
                w.write_char(*c)?;
                Ok(false)
            }
            Backslashed(c) => {
                w.write_char(*c)?;
                Ok(true)
            }
            RawParam { param, .. } => {
                write!(w, "${param}")?;
                Ok(false)
            }
            BracedParam(param) => param.write_unquoted(w),
            // We don't remove quotes contained in the commands in command
            // substitutions. Existing shells disagree with each other.
            CommandSubst { content, .. } => {
                write!(w, "$({content})")?;
                Ok(false)
            }
            Backquote { content, .. } => {
                w.write_char('`')?;
                let quoted = content.write_unquoted(w)?;
                w.write_char('`')?;
                Ok(quoted)
            }
            Arith { content, .. } => {
                w.write_str("$((")?;
                let quoted = content.write_unquoted(w)?;
                w.write_str("))")?;
                Ok(quoted)
            }
        }
    }
}

impl MaybeLiteral for TextUnit {
    /// If `self` is `Literal`, appends the character to `result`.
    fn extend_literal<T: Extend<char>>(&self, result: &mut T) -> Result<(), NotLiteral> {
        if let Literal(c) = self {
            // TODO Use Extend::extend_one
            result.extend(std::iter::once(*c));
            Ok(())
        } else {
            Err(NotLiteral)
        }
    }
}

impl Text {
    /// Creates a text from an iterator of literal chars.
    #[must_use]
    pub fn from_literal_chars<I: IntoIterator<Item = char>>(i: I) -> Text {
        Text(i.into_iter().map(Literal).collect())
    }
}

impl Unquote for Text {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        self.0.write_unquoted(w)
    }
}

impl MaybeLiteral for Text {
    fn extend_literal<T: Extend<char>>(&self, result: &mut T) -> Result<(), NotLiteral> {
        self.0.extend_literal(result)
    }
}

/// Converts an escape unit into the string represented by the escape sequence.
///
/// Produces an empty string if the escape unit does not represent a valid
/// Unicode scalar value.
impl Unquote for EscapeUnit {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        match self {
            Self::Literal(c) => {
                w.write_char(*c)?;
                Ok(false)
            }
            Self::DoubleQuote => {
                w.write_char('"')?;
                Ok(true)
            }
            Self::SingleQuote => {
                w.write_char('\'')?;
                Ok(true)
            }
            Self::Backslash => {
                w.write_char('\\')?;
                Ok(true)
            }
            Self::Question => {
                w.write_char('?')?;
                Ok(true)
            }
            Self::Alert => {
                w.write_char('\x07')?;
                Ok(true)
            }
            Self::Backspace => {
                w.write_char('\x08')?;
                Ok(true)
            }
            Self::Escape => {
                w.write_char('\x1B')?;
                Ok(true)
            }
            Self::FormFeed => {
                w.write_char('\x0C')?;
                Ok(true)
            }
            Self::Newline => {
                w.write_char('\n')?;
                Ok(true)
            }
            Self::CarriageReturn => {
                w.write_char('\r')?;
                Ok(true)
            }
            Self::Tab => {
                w.write_char('\t')?;
                Ok(true)
            }
            Self::VerticalTab => {
                w.write_char('\x0B')?;
                Ok(true)
            }
            Self::Control(c) | Self::Octal(c) | Self::Hex(c) => {
                // TODO: `c` should be treated as a raw byte rather than a
                // Unicode scalar value. However, std::fmt::Write only supports
                // UTF-8 strings.
                w.write_char(*c as char)?;
                Ok(true)
            }
            Self::Unicode(c) => {
                w.write_char(*c)?;
                Ok(true)
            }
        }
    }
}

impl MaybeLiteral for EscapeUnit {
    fn extend_literal<T: Extend<char>>(&self, result: &mut T) -> Result<(), NotLiteral> {
        if let Self::Literal(c) = self {
            result.extend(std::iter::once(*c));
            Ok(())
        } else {
            Err(NotLiteral)
        }
    }
}

/// Converts an escaped string into the string represented by the escape
/// sequences.
///
/// [Escape units](EscapeUnit) that do not represent valid Unicode scalar values
/// are ignored.
impl Unquote for EscapedString {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        self.0.write_unquoted(w)
    }
}

impl MaybeLiteral for EscapedString {
    fn extend_literal<T: Extend<char>>(&self, result: &mut T) -> Result<(), NotLiteral> {
        self.0.extend_literal(result)
    }
}

impl Unquote for WordUnit {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        match self {
            Unquoted(inner) => inner.write_unquoted(w),
            SingleQuote(inner) => {
                w.write_str(inner)?;
                Ok(true)
            }
            DoubleQuote(inner) => inner.write_unquoted(w),
            DollarSingleQuote(inner) => inner.write_unquoted(w),
            Tilde { name, .. } => {
                write!(w, "~{name}")?;
                Ok(false)
            }
        }
    }
}

impl MaybeLiteral for WordUnit {
    /// If `self` is `Unquoted(Literal(_))`, appends the character to `result`.
    fn extend_literal<T: Extend<char>>(&self, result: &mut T) -> Result<(), NotLiteral> {
        if let Unquoted(inner) = self {
            inner.extend_literal(result)
        } else {
            Err(NotLiteral)
        }
    }
}

impl Unquote for Word {
    fn write_unquoted<W: fmt::Write>(&self, w: &mut W) -> UnquoteResult {
        self.units.write_unquoted(w)
    }
}

impl MaybeLiteral for Word {
    fn extend_literal<T: Extend<char>>(&self, result: &mut T) -> Result<(), NotLiteral> {
        self.units.extend_literal(result)
    }
}

/// Fallible conversion from a word into an assignment
impl TryFrom<Word> for Assign {
    type Error = Word;
    /// Converts a word into an assignment.
    ///
    /// For a successful conversion, the word must be of the form `name=value`,
    /// where `name` is a non-empty [literal](Word::to_string_if_literal) word,
    /// `=` is an unquoted equal sign, and `value` is a word. If the input word
    /// does not match this syntax, it is returned intact in `Err`.
    fn try_from(mut word: Word) -> Result<Assign, Word> {
        if let Some(eq) = word.units.iter().position(|u| u == &Unquoted(Literal('='))) {
            if eq > 0 {
                if let Some(name) = word.units[..eq].to_string_if_literal() {
                    assert!(!name.is_empty());
                    word.units.drain(..=eq);
                    word.parse_tilde_everywhere();
                    let location = word.location.clone();
                    let value = Scalar(word);
                    return Ok(Assign {
                        name,
                        value,
                        location,
                    });
                }
            }
        }

        Err(word)
    }
}

impl From<RawFd> for Fd {
    fn from(raw_fd: RawFd) -> Fd {
        Fd(raw_fd)
    }
}

impl TryFrom<Operator> for RedirOp {
    type Error = TryFromOperatorError;
    fn try_from(op: Operator) -> Result<RedirOp, TryFromOperatorError> {
        use Operator::*;
        use RedirOp::*;
        match op {
            Less => Ok(FileIn),
            LessGreater => Ok(FileInOut),
            Greater => Ok(FileOut),
            GreaterGreater => Ok(FileAppend),
            GreaterBar => Ok(FileClobber),
            LessAnd => Ok(FdIn),
            GreaterAnd => Ok(FdOut),
            GreaterGreaterBar => Ok(Pipe),
            LessLessLess => Ok(String),
            _ => Err(TryFromOperatorError {}),
        }
    }
}

impl From<RedirOp> for Operator {
    fn from(op: RedirOp) -> Operator {
        use Operator::*;
        use RedirOp::*;
        match op {
            FileIn => Less,
            FileInOut => LessGreater,
            FileOut => Greater,
            FileAppend => GreaterGreater,
            FileClobber => GreaterBar,
            FdIn => LessAnd,
            FdOut => GreaterAnd,
            Pipe => GreaterGreaterBar,
            String => LessLessLess,
        }
    }
}

impl<T: Into<Rc<HereDoc>>> From<T> for RedirBody {
    fn from(t: T) -> Self {
        RedirBody::HereDoc(t.into())
    }
}

impl TryFrom<Operator> for CaseContinuation {
    type Error = TryFromOperatorError;

    /// Converts an operator into a case continuation.
    ///
    /// The `SemicolonBar` and `SemicolonSemicolonAnd` operators are converted
    /// into `Continue`; you cannot distinguish between the two from the return
    /// value.
    fn try_from(op: Operator) -> Result<CaseContinuation, TryFromOperatorError> {
        use CaseContinuation::*;
        use Operator::*;
        match op {
            SemicolonSemicolon => Ok(Break),
            SemicolonAnd => Ok(FallThrough),
            SemicolonBar | SemicolonSemicolonAnd => Ok(Continue),
            _ => Err(TryFromOperatorError {}),
        }
    }
}

impl From<CaseContinuation> for Operator {
    /// Converts a case continuation into an operator.
    ///
    /// The `Continue` variant is converted into `SemicolonBar`.
    fn from(cc: CaseContinuation) -> Operator {
        use CaseContinuation::*;
        use Operator::*;
        match cc {
            Break => SemicolonSemicolon,
            FallThrough => SemicolonAnd,
            Continue => SemicolonBar,
        }
    }
}

impl TryFrom<Operator> for AndOr {
    type Error = TryFromOperatorError;
    fn try_from(op: Operator) -> Result<AndOr, TryFromOperatorError> {
        match op {
            Operator::AndAnd => Ok(AndOr::AndThen),
            Operator::BarBar => Ok(AndOr::OrElse),
            _ => Err(TryFromOperatorError {}),
        }
    }
}

impl From<AndOr> for Operator {
    fn from(op: AndOr) -> Operator {
        match op {
            AndOr::AndThen => Operator::AndAnd,
            AndOr::OrElse => Operator::BarBar,
        }
    }
}

#[allow(clippy::bool_assert_comparison)]
#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;

    #[test]
    fn special_param_from_str() {
        assert_eq!("@".parse(), Ok(SpecialParam::At));
        assert_eq!("*".parse(), Ok(SpecialParam::Asterisk));
        assert_eq!("#".parse(), Ok(SpecialParam::Number));
        assert_eq!("?".parse(), Ok(SpecialParam::Question));
        assert_eq!("-".parse(), Ok(SpecialParam::Hyphen));
        assert_eq!("$".parse(), Ok(SpecialParam::Dollar));
        assert_eq!("!".parse(), Ok(SpecialParam::Exclamation));
        assert_eq!("0".parse(), Ok(SpecialParam::Zero));

        assert_eq!(SpecialParam::from_str(""), Err(NotSpecialParam));
        assert_eq!(SpecialParam::from_str("##"), Err(NotSpecialParam));
        assert_eq!(SpecialParam::from_str("1"), Err(NotSpecialParam));
        assert_eq!(SpecialParam::from_str("00"), Err(NotSpecialParam));
    }

    #[test]
    fn switch_unquote() {
        let switch = Switch {
            action: SwitchAction::Default,
            condition: SwitchCondition::UnsetOrEmpty,
            word: "foo bar".parse().unwrap(),
        };
        let (unquoted, is_quoted) = switch.unquote();
        assert_eq!(unquoted, ":-foo bar");
        assert_eq!(is_quoted, false);

        let switch = Switch {
            action: SwitchAction::Error,
            condition: SwitchCondition::Unset,
            word: r"e\r\ror".parse().unwrap(),
        };
        let (unquoted, is_quoted) = switch.unquote();
        assert_eq!(unquoted, "?error");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn trim_unquote() {
        let trim = Trim {
            side: TrimSide::Prefix,
            length: TrimLength::Shortest,
            pattern: "".parse().unwrap(),
        };
        let (unquoted, is_quoted) = trim.unquote();
        assert_eq!(unquoted, "#");
        assert_eq!(is_quoted, false);

        let trim = Trim {
            side: TrimSide::Prefix,
            length: TrimLength::Longest,
            pattern: "'yes'".parse().unwrap(),
        };
        let (unquoted, is_quoted) = trim.unquote();
        assert_eq!(unquoted, "##yes");
        assert_eq!(is_quoted, true);

        let trim = Trim {
            side: TrimSide::Suffix,
            length: TrimLength::Shortest,
            pattern: r"\no".parse().unwrap(),
        };
        let (unquoted, is_quoted) = trim.unquote();
        assert_eq!(unquoted, "%no");
        assert_eq!(is_quoted, true);

        let trim = Trim {
            side: TrimSide::Suffix,
            length: TrimLength::Longest,
            pattern: "?".parse().unwrap(),
        };
        let (unquoted, is_quoted) = trim.unquote();
        assert_eq!(unquoted, "%%?");
        assert_eq!(is_quoted, false);
    }

    #[test]
    fn braced_param_unquote() {
        let param = BracedParam {
            param: Param::variable("foo"),
            modifier: Modifier::None,
            location: Location::dummy(""),
        };
        let (unquoted, is_quoted) = param.unquote();
        assert_eq!(unquoted, "${foo}");
        assert_eq!(is_quoted, false);

        let param = BracedParam {
            modifier: Modifier::Length,
            ..param
        };
        let (unquoted, is_quoted) = param.unquote();
        assert_eq!(unquoted, "${#foo}");
        assert_eq!(is_quoted, false);

        let switch = Switch {
            action: SwitchAction::Assign,
            condition: SwitchCondition::UnsetOrEmpty,
            word: "'bar'".parse().unwrap(),
        };
        let param = BracedParam {
            modifier: Modifier::Switch(switch),
            ..param
        };
        let (unquoted, is_quoted) = param.unquote();
        assert_eq!(unquoted, "${foo:=bar}");
        assert_eq!(is_quoted, true);

        let trim = Trim {
            side: TrimSide::Suffix,
            length: TrimLength::Shortest,
            pattern: "baz' 'bar".parse().unwrap(),
        };
        let param = BracedParam {
            modifier: Modifier::Trim(trim),
            ..param
        };
        let (unquoted, is_quoted) = param.unquote();
        assert_eq!(unquoted, "${foo%baz bar}");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn backquote_unit_unquote() {
        let literal = BackquoteUnit::Literal('A');
        let (unquoted, is_quoted) = literal.unquote();
        assert_eq!(unquoted, "A");
        assert_eq!(is_quoted, false);

        let backslashed = BackquoteUnit::Backslashed('X');
        let (unquoted, is_quoted) = backslashed.unquote();
        assert_eq!(unquoted, "X");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn text_from_literal_chars() {
        let text = Text::from_literal_chars(['a', '1'].iter().copied());
        assert_eq!(text.0, [Literal('a'), Literal('1')]);
    }

    #[test]
    fn text_unquote_without_quotes() {
        let empty = Text(vec![]);
        let (unquoted, is_quoted) = empty.unquote();
        assert_eq!(unquoted, "");
        assert_eq!(is_quoted, false);

        let nonempty = Text(vec![
            Literal('W'),
            RawParam {
                param: Param::variable("X"),
                location: Location::dummy(""),
            },
            CommandSubst {
                content: "Y".into(),
                location: Location::dummy(""),
            },
            Backquote {
                content: vec![BackquoteUnit::Literal('Z')],
                location: Location::dummy(""),
            },
            Arith {
                content: Text(vec![Literal('0')]),
                location: Location::dummy(""),
            },
        ]);
        let (unquoted, is_quoted) = nonempty.unquote();
        assert_eq!(unquoted, "W$X$(Y)`Z`$((0))");
        assert_eq!(is_quoted, false);
    }

    #[test]
    fn text_unquote_with_quotes() {
        let quoted = Text(vec![
            Literal('a'),
            Backslashed('b'),
            Literal('c'),
            Arith {
                content: Text(vec![Literal('d')]),
                location: Location::dummy(""),
            },
            Literal('e'),
        ]);
        let (unquoted, is_quoted) = quoted.unquote();
        assert_eq!(unquoted, "abc$((d))e");
        assert_eq!(is_quoted, true);

        let content = vec![BackquoteUnit::Backslashed('X')];
        let location = Location::dummy("");
        let quoted = Text(vec![Backquote { content, location }]);
        let (unquoted, is_quoted) = quoted.unquote();
        assert_eq!(unquoted, "`X`");
        assert_eq!(is_quoted, true);

        let content = Text(vec![Backslashed('X')]);
        let location = Location::dummy("");
        let quoted = Text(vec![Arith { content, location }]);
        let (unquoted, is_quoted) = quoted.unquote();
        assert_eq!(unquoted, "$((X))");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn text_to_string_if_literal_success() {
        let empty = Text(vec![]);
        let s = empty.to_string_if_literal().unwrap();
        assert_eq!(s, "");

        let nonempty = Text(vec![Literal('f'), Literal('o'), Literal('o')]);
        let s = nonempty.to_string_if_literal().unwrap();
        assert_eq!(s, "foo");
    }

    #[test]
    fn text_to_string_if_literal_failure() {
        let backslashed = Text(vec![Backslashed('a')]);
        assert_eq!(backslashed.to_string_if_literal(), None);
    }

    #[test]
    fn escape_unit_unquote() {
        assert_eq!(EscapeUnit::Literal('A').unquote(), ("A".to_string(), false));
        assert_eq!(EscapeUnit::DoubleQuote.unquote(), ("\"".to_string(), true));
        assert_eq!(EscapeUnit::SingleQuote.unquote(), ("'".to_string(), true));
        assert_eq!(EscapeUnit::Backslash.unquote(), ("\\".to_string(), true));
        assert_eq!(EscapeUnit::Question.unquote(), ("?".to_string(), true));
        assert_eq!(EscapeUnit::Alert.unquote(), ("\x07".to_string(), true));
        assert_eq!(EscapeUnit::Backspace.unquote(), ("\x08".to_string(), true));
        assert_eq!(EscapeUnit::Escape.unquote(), ("\x1B".to_string(), true));
        assert_eq!(EscapeUnit::FormFeed.unquote(), ("\x0C".to_string(), true));
        assert_eq!(EscapeUnit::Newline.unquote(), ("\n".to_string(), true));
        assert_eq!(
            EscapeUnit::CarriageReturn.unquote(),
            ("\r".to_string(), true)
        );
        assert_eq!(EscapeUnit::Tab.unquote(), ("\t".to_string(), true));
        assert_eq!(
            EscapeUnit::VerticalTab.unquote(),
            ("\x0B".to_string(), true)
        );
        assert_eq!(
            EscapeUnit::Control(0x01).unquote(),
            ("\x01".to_string(), true)
        );
        assert_eq!(
            EscapeUnit::Control(0x1E).unquote(),
            ("\x1E".to_string(), true)
        );
        assert_eq!(
            EscapeUnit::Control(0x7F).unquote(),
            ("\x7F".to_string(), true)
        );
        assert_eq!(EscapeUnit::Octal(0o123).unquote(), ("S".to_string(), true));
        assert_eq!(EscapeUnit::Hex(0x41).unquote(), ("A".to_string(), true));
        assert_eq!(
            EscapeUnit::Unicode('🦀').unquote(),
            ("🦀".to_string(), true)
        );
    }

    #[test]
    fn word_unquote() {
        let mut word = Word::from_str(r#"~a/b\c'd'"e""#).unwrap();
        let (unquoted, is_quoted) = word.unquote();
        assert_eq!(unquoted, "~a/bcde");
        assert_eq!(is_quoted, true);

        word.parse_tilde_front();
        let (unquoted, is_quoted) = word.unquote();
        assert_eq!(unquoted, "~a/bcde");
        assert_eq!(is_quoted, true);
    }

    #[test]
    fn word_to_string_if_literal_success() {
        let empty = Word::from_str("").unwrap();
        let s = empty.to_string_if_literal().unwrap();
        assert_eq!(s, "");

        let nonempty = Word::from_str("~foo").unwrap();
        let s = nonempty.to_string_if_literal().unwrap();
        assert_eq!(s, "~foo");
    }

    #[test]
    fn word_to_string_if_literal_failure() {
        let location = Location::dummy("foo");
        let backslashed = Unquoted(Backslashed('?'));
        let word = Word {
            units: vec![backslashed],
            location,
        };
        assert_eq!(word.to_string_if_literal(), None);

        let word = Word {
            units: vec![Tilde {
                name: "foo".to_string(),
                followed_by_slash: false,
            }],
            ..word
        };
        assert_eq!(word.to_string_if_literal(), None);
    }

    #[test]
    fn assign_try_from_word_without_equal() {
        let word = Word::from_str("foo").unwrap();
        let result = Assign::try_from(word.clone());
        assert_eq!(result.unwrap_err(), word);
    }

    #[test]
    fn assign_try_from_word_with_empty_name() {
        let word = Word::from_str("=foo").unwrap();
        let result = Assign::try_from(word.clone());
        assert_eq!(result.unwrap_err(), word);
    }

    #[test]
    fn assign_try_from_word_with_non_literal_name() {
        let mut word = Word::from_str("night=foo").unwrap();
        word.units.insert(0, Unquoted(Backslashed('k')));
        let result = Assign::try_from(word.clone());
        assert_eq!(result.unwrap_err(), word);
    }

    #[test]
    fn assign_try_from_word_with_literal_name() {
        let word = Word::from_str("night=foo").unwrap();
        let location = word.location.clone();
        let assign = Assign::try_from(word).unwrap();
        assert_eq!(assign.name, "night");
        assert_matches!(assign.value, Scalar(value) => {
            assert_eq!(value.to_string(), "foo");
            assert_eq!(value.location, location);
        });
        assert_eq!(assign.location, location);
    }

    #[test]
    fn assign_try_from_word_tilde() {
        let word = Word::from_str("a=~:~b").unwrap();
        let assign = Assign::try_from(word).unwrap();
        assert_matches!(assign.value, Scalar(value) => {
            assert_eq!(
                value.units,
                [
                    WordUnit::Tilde{
                        name: "".to_string(),
                        followed_by_slash: false,
                    },
                    WordUnit::Unquoted(TextUnit::Literal(':')),
                    WordUnit::Tilde {
                        name: "b".to_string(),
                        followed_by_slash: false,
                    },
                ]
            );
        });
    }

    #[test]
    fn redir_op_conversions() {
        use RedirOp::*;
        for op in &[
            FileIn,
            FileInOut,
            FileOut,
            FileAppend,
            FileClobber,
            FdIn,
            FdOut,
            Pipe,
            String,
        ] {
            let op2 = RedirOp::try_from(Operator::from(*op));
            assert_eq!(op2, Ok(*op));
        }
    }

    #[test]
    fn case_continuation_conversions() {
        use CaseContinuation::*;
        for cc in &[Break, FallThrough, Continue] {
            let cc2 = CaseContinuation::try_from(Operator::from(*cc));
            assert_eq!(cc2, Ok(*cc));
        }
        assert_eq!(
            CaseContinuation::try_from(Operator::SemicolonSemicolonAnd),
            Ok(Continue)
        );
    }

    #[test]
    fn and_or_conversions() {
        for op in &[AndOr::AndThen, AndOr::OrElse] {
            let op2 = AndOr::try_from(Operator::from(*op));
            assert_eq!(op2, Ok(*op));
        }
    }
}
