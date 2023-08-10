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

//! Part of the cd built-in that simplifies the target path
//!
//! This module implements step 9 of the cd built-in, which simplifies the
//! target path by removing its leading components that match `$PWD`.
//! For example, if the current directory is `/home/yash` and the target path is
//! `/home/yash/src`, the target path is simplified to `src`.
//! This step returns the path intact if the `-P` option is effective or the
//! target path does not start with `$PWD`.

use super::Mode;
use std::path::Path;

/// Simplifies the target path.
///
/// See the [module-level documentation](self) for details.
#[must_use]
pub fn shorten<'a>(target: &'a Path, pwd: &Path, mode: Mode) -> &'a Path {
    match mode {
        Mode::Physical => target,
        Mode::Logical => {
            // TODO The current Rust implementation of `strip_prefix` regards
            // "/" and "//" as the same path, which is not POSIXly correct, but
            // Rust is not yet ported to platforms where this matters. We may
            // need to revisit this when Rust supports such a platform, notably
            // Cygwin.
            let result = target.strip_prefix(pwd).unwrap_or(target);
            if result == Path::new("") {
                Path::new(".")
            } else {
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_not_starting_with_pwd() {
        let result = shorten(Path::new("/foo/bar"), Path::new("/foo/baz"), Mode::Logical);
        assert_eq!(result, Path::new("/foo/bar"));
    }

    #[test]
    fn target_starting_with_pwd() {
        let result = shorten(Path::new("/foo/bar"), Path::new("/foo"), Mode::Logical);
        assert_eq!(result, Path::new("bar"));

        let result = shorten(Path::new("/foo/bar"), Path::new("/"), Mode::Logical);
        assert_eq!(result, Path::new("foo/bar"));
    }

    #[test]
    fn target_equals_pwd() {
        let result = shorten(Path::new("/foo"), Path::new("/foo"), Mode::Logical);
        assert_eq!(result, Path::new("."));

        let result = shorten(Path::new("/one/two"), Path::new("/one/two"), Mode::Logical);
        assert_eq!(result, Path::new("."));
    }

    #[test]
    fn physical_mode() {
        let result = shorten(Path::new("/foo/bar"), Path::new("/foo"), Mode::Physical);
        assert_eq!(result, Path::new("/foo/bar"));
    }
}
