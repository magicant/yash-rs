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

//! Types and functions for parsing reserved words.

use std::fmt;

/// Token identifier for reserved words.
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

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Keyword::*;
        f.write_str(match self {
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
        })
    }
}

/// Provides a function for finding keywords.
pub trait AsKeyword {
    /// Determines if `self` is a reserved word.
    fn as_keyword(&self) -> Option<Keyword>;
}

impl AsKeyword for str {
    fn as_keyword(&self) -> Option<Keyword> {
        use Keyword::*;
        match self {
            "!" => Some(Bang),
            "[[" => Some(OpenBracketBracket),
            "case" => Some(Case),
            "do" => Some(Do),
            "done" => Some(Done),
            "elif" => Some(Elif),
            "else" => Some(Else),
            "esac" => Some(Esac),
            "fi" => Some(Fi),
            "for" => Some(For),
            "function" => Some(Function),
            "if" => Some(If),
            "in" => Some(In),
            "then" => Some(Then),
            "until" => Some(Until),
            "while" => Some(While),
            "{" => Some(OpenBrace),
            "}" => Some(CloseBrace),
            _ => None,
        }
    }
}
