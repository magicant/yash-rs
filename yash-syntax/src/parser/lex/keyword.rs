// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Types and functions for parsing reserved words

use std::fmt;
use std::str::FromStr;
use thiserror::Error;

/// Error value indicating that a string is not a keyword
///
/// This error is returned by [`Keyword::from_str`] when the input string is not
/// a keyword.
#[derive(Clone, Debug, Eq, Error, Hash, PartialEq)]
pub struct ParseKeywordError;

impl fmt::Display for ParseKeywordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a keyword")
    }
}

/// Token identifier for reserved words
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Keyword {
    Bang,
    /// `[[`
    OpenBracketBracket,
    Case,
    Do,
    Done,
    Elif,
    Else,
    Esac,
    Fi,
    For,
    Function,
    If,
    In,
    Then,
    Until,
    While,
    /// `{`
    OpenBrace,
    /// `}`
    CloseBrace,
}

impl Keyword {
    /// Returns the literal string representation of the keyword.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        use Keyword::*;
        match self {
            Bang => "!",
            OpenBracketBracket => "[[",
            Case => "case",
            Do => "do",
            Done => "done",
            Elif => "elif",
            Else => "else",
            Esac => "esac",
            Fi => "fi",
            For => "for",
            Function => "function",
            If => "if",
            In => "in",
            Then => "then",
            Until => "until",
            While => "while",
            OpenBrace => "{",
            CloseBrace => "}",
        }
    }

    /// Determines if this token can be a delimiter of a clause.
    ///
    /// This function returns `true` for `Do`, `Done`, `Elif`, `Else`, `Esac`,
    /// `Fi`, `Then`, and `CloseBrace`, and `false` for others.
    #[must_use]
    pub const fn is_clause_delimiter(self) -> bool {
        use Keyword::*;
        match self {
            Do | Done | Elif | Else | Esac | Fi | Then | CloseBrace => true,
            Bang | OpenBracketBracket | Case | For | Function | If | In | Until | While
            | OpenBrace => false,
        }
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Keyword {
    type Err = ParseKeywordError;
    fn from_str(s: &str) -> Result<Keyword, ParseKeywordError> {
        use Keyword::*;
        match s {
            "!" => Ok(Bang),
            "[[" => Ok(OpenBracketBracket),
            "case" => Ok(Case),
            "do" => Ok(Do),
            "done" => Ok(Done),
            "elif" => Ok(Elif),
            "else" => Ok(Else),
            "esac" => Ok(Esac),
            "fi" => Ok(Fi),
            "for" => Ok(For),
            "function" => Ok(Function),
            "if" => Ok(If),
            "in" => Ok(In),
            "then" => Ok(Then),
            "until" => Ok(Until),
            "while" => Ok(While),
            "{" => Ok(OpenBrace),
            "}" => Ok(CloseBrace),
            _ => Err(ParseKeywordError),
        }
    }
}
