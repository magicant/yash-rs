// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2024 WATANABE Yuki
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

//! Variable name and default value constants

/// The name of the `CDPATH` variable
pub const CDPATH: &str = "CDPATH";

/// The name of the `ENV` variable
pub const ENV: &str = "ENV";

/// The name of the `HOME` variable
pub const HOME: &str = "HOME";

/// The name of the `IFS` variable
pub const IFS: &str = "IFS";

/// The initial value of the `IFS` variable (`" \t\n"`)
pub const IFS_INITIAL_VALUE: &str = " \t\n";

/// The name of the `LINENO` variable
pub const LINENO: &str = "LINENO";

/// The name of the `OLDPWD` variable
pub const OLDPWD: &str = "OLDPWD";

/// The name of the `OPTARG` variable
pub const OPTARG: &str = "OPTARG";

/// The name of the `OPTIND` variable
pub const OPTIND: &str = "OPTIND";

/// The initial value of the `OPTIND` variable (`1`)
pub const OPTIND_INITIAL_VALUE: &str = "1";

/// The name of the `PATH` variable
pub const PATH: &str = "PATH";

/// The name of the `PPID` variable
pub const PPID: &str = "PPID";

/// The name of the `PS1` variable
pub const PS1: &str = "PS1";

/// The initial value of the `PS1` variable for non-root user (`"$ "`)
pub const PS1_INITIAL_VALUE_NON_ROOT: &str = "$ ";

/// The initial value of the `PS1` variable for root user (`"# "`)
pub const PS1_INITIAL_VALUE_ROOT: &str = "# ";

/// The name of the `PS2` variable
pub const PS2: &str = "PS2";

/// The initial value of the `PS2` variable (`"> "`)
pub const PS2_INITIAL_VALUE: &str = "> ";

/// The name of the `PS4` variable
pub const PS4: &str = "PS4";

/// The initial value of the `PS4` variable (`"+ "`)
pub const PS4_INITIAL_VALUE: &str = "+ ";

/// The name of the `PWD` variable
pub const PWD: &str = "PWD";
