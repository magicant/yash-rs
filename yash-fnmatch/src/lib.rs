// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki

//! This crate provides the `fnmatch` function that performs pattern matching
//! based on a globbing pattern.
//!
//! This implementation supports the following syntax in patterns:
//!
//! - Any single character (`?`)
//! - Any character sequence (`*`)
//! - Bracket expression (`[...]`)
//!     - Character literals
//!     - Character ranges (e.g. `a-z`)
//!     - Complement (`[!...]`)
//!     - Collating symbols (e.g. `[.ch.]`)
//!     - Equivalence classes (e.g. `[=a=]`)
//!     - Character classes (`[:alpha:]`)
//!
//! The current implementation does not support any locale-specific
//! characteristics. Especially, collating symbols and equivalent classes only
//! match the specified character sequence itself, and character classes only
//! match ASCII characters.
//!
//! This crate is very similar to the [`fnmatch-regex`] crate in that the both
//! perform matching by converting the pattern to a regular expression. The
//! `yash-fnmatch` crate tries to support the POSIX specification as much as
//! possible rather than introducing unique (non-portable) functionalities.
//!
//! [`fnmatch-regex`]: https://crates.io/crates/fnmatch-regex

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        let result = 2 + 2;
        assert_eq!(result, 4);
    }
}
