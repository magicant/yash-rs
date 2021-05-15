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

//! Lexical analyzer.
//!
//! TODO Elaborate

mod core;

mod arith;
mod backquote;
mod braced_param;
mod command_subst;
mod dollar;
mod heredoc;
mod keyword;
mod misc;
mod op;
mod raw_param;
mod text;
mod tilde;
mod token;
mod word;

pub use self::braced_param::is_name_char;
pub use self::core::*;
pub use self::heredoc::PartialHereDoc;
pub use self::keyword::Keyword;
pub use self::op::is_operator_char;
pub use self::op::Operator;
pub use self::raw_param::is_portable_name_char;
pub use self::raw_param::is_special_parameter_char;
pub use self::token::is_token_delimiter_char;
