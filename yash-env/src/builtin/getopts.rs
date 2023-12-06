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

//! Definition of [`GetoptsState`]

/// Origin of the arguments parsed by the `getopts` built-in
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Origin {
    /// The arguments are passed directly to the built-in.
    DirectArgs,
    /// No arguments are passed to the built-in, so the built-in parses the
    /// positional parameters.
    PositionalParams,
}

/// State of the `getopts` built-in
///
/// This data is specifically designed for use by the `getopts` built-in. The
/// built-in is designed to be called multiple times with the same arguments,
/// assuming that the `$OPTIND` variable isn't altered externally. The built-in
/// stores the arguments and the `$OPTIND` value in this data, and verifies if
/// it receives the same arguments and `$OPTIND` value in subsequent calls.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct GetoptsState {
    /// Expected arguments to parse
    pub args: Vec<String>,
    /// Expected origin of the arguments
    pub origin: Origin,
    /// Expected value of `$OPTIND`
    pub optind: String,
}
