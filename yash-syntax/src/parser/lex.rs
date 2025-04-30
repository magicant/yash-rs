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

//! Lexical analyzer
//!
//! See the [parent module](super)'s documentation to learn how to use the
//! [lexer](Lexer).

mod core;

mod arith;
mod backquote;
mod braced_param;
mod command_subst;
mod dollar;
mod escape;
mod heredoc;
mod keyword;
mod misc;
mod modifier;
mod op;
mod raw_param;
mod text;
mod tilde;
mod token;
mod word;

pub use self::braced_param::is_name;
pub use self::braced_param::is_name_char;
pub use self::core::*;
pub use self::keyword::Keyword;
pub use self::keyword::ParseKeywordError;
pub use self::op::Operator;
pub use self::op::ParseOperatorError;
pub use self::op::TryFromOperatorError;
pub use self::op::is_operator_char;
pub use self::raw_param::is_portable_name;
pub use self::raw_param::is_portable_name_char;
pub use self::raw_param::is_single_char_name;
pub use self::raw_param::is_special_parameter_char;
pub use self::token::is_token_delimiter_char;
