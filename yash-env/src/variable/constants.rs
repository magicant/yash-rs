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
///
/// The `CDPATH` variable is used by the `cd` built-in to search for
/// directories. Its value is a colon-separated list of directories.
pub const CDPATH: &str = "CDPATH";

/// The name of the `ENV` variable
///
/// The `ENV` variable specifies the file to read for environment
/// variables when the shell is invoked.
pub const ENV: &str = "ENV";

/// The name of the `HOME` variable
///
/// The `HOME` variable stores the path to the user's home directory.
pub const HOME: &str = "HOME";

/// The name of the `IFS` variable
///
/// The `IFS` variable separator characters for field splitting.
/// The initial value is `" \t\n"`.
pub const IFS: &str = "IFS";

/// The initial value of the `IFS` variable (`" \t\n"`)
pub const IFS_INITIAL_VALUE: &str = " \t\n";

/// The name of the `LINENO` variable
///
/// The `LINENO` variable expands to the line number of the current command.
pub const LINENO: &str = "LINENO";

/// The name of the `OLDPWD` variable
///
/// The `cd` built-in sets the `OLDPWD` variable to the previous working directory.
pub const OLDPWD: &str = "OLDPWD";

/// The name of the `OPTARG` variable
///
/// The `OPTARG` variable is used by the `getopts` built-in to store the
/// argument of an option.
pub const OPTARG: &str = "OPTARG";

/// The name of the `OPTIND` variable
///
/// The `OPTIND` variable is used by the `getopts` built-in to store the
/// index of the next argument to be processed. The initial value is `1`.
pub const OPTIND: &str = "OPTIND";

/// The initial value of the `OPTIND` variable (`1`)
pub const OPTIND_INITIAL_VALUE: &str = "1";

/// The name of the `PATH` variable
///
/// The `PATH` variable stores the directories to search for executables.
pub const PATH: &str = "PATH";

/// The name of the `PPID` variable
///
/// The `PPID` variable stores the process ID of the parent process.
pub const PPID: &str = "PPID";

/// The name of the `PS1` variable
///
/// The `PS1` variable is the primary prompt string.
/// The initial value depends on the user's privilege level.
/// See [`PS1_INITIAL_VALUE_NON_ROOT`] and [`PS1_INITIAL_VALUE_ROOT`].
pub const PS1: &str = "PS1";

/// The initial value of the `PS1` variable for non-root user (`"$ "`)
pub const PS1_INITIAL_VALUE_NON_ROOT: &str = "$ ";

/// The initial value of the `PS1` variable for root user (`"# "`)
pub const PS1_INITIAL_VALUE_ROOT: &str = "# ";

/// The name of the `PS2` variable
///
/// The `PS2` variable is the secondary prompt string, which is shown
/// when the shell expects more input to complete a command.
/// The initial value is `"> "`.
pub const PS2: &str = "PS2";

/// The initial value of the `PS2` variable (`"> "`)
pub const PS2_INITIAL_VALUE: &str = "> ";

/// The name of the `PS4` variable
///
/// The `PS4` variable is used by the [`XTrace`](crate::option::XTrace) option
/// to show the command before it is executed. The value is prefixed to the
/// printed command. The initial value is `"+ "`.
pub const PS4: &str = "PS4";

/// The initial value of the `PS4` variable (`"+ "`)
pub const PS4_INITIAL_VALUE: &str = "+ ";

/// The name of the `PWD` variable
///
/// The `PWD` variable stores the current working directory.
pub const PWD: &str = "PWD";
