// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2026 WATANABE Yuki
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

//! Reporting job ID errors
//!
//! This module contains helpers shared by the built-ins that take a job ID:
//! `bg`, `fg`, `jobs`, `kill`, and `wait`.

use yash_env::job::id::ParseError;
use yash_env::source::pretty::{Footnote, FootnoteType};

/// Returns the footnotes for a job ID the `portable` shell option rejected.
///
/// The caller decides whether the option is what made `error` an error; the
/// note this function returns says so unconditionally. The suggestion is
/// included only for the lone `%`, which is the only job ID that has a portable
/// spelling to suggest.
#[must_use]
pub(crate) fn non_portable_footnotes(error: ParseError) -> Vec<Footnote<'static>> {
    let mut footnotes = Vec::with_capacity(2);
    if error == ParseError::LonePercent {
        footnotes.push(Footnote {
            r#type: FootnoteType::Suggestion,
            label: "use `%%` or `%+` instead".into(),
        });
    }
    footnotes.push(Footnote {
        r#type: FootnoteType::Note,
        label: "this error is reported because the `portable` shell option is enabled".into(),
    });
    footnotes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lone_percent_is_suggested_a_portable_spelling() {
        let footnotes = non_portable_footnotes(ParseError::LonePercent);

        assert_eq!(footnotes.len(), 2);
        assert_eq!(footnotes[0].r#type, FootnoteType::Suggestion);
        assert_eq!(footnotes[0].label, "use `%%` or `%+` instead");
        assert_eq!(footnotes[1].r#type, FootnoteType::Note);
        assert_eq!(
            footnotes[1].label,
            "this error is reported because the `portable` shell option is enabled",
        );
    }

    #[test]
    fn other_errors_have_no_suggestion() {
        let footnotes = non_portable_footnotes(ParseError::MissingPercent);

        assert_eq!(footnotes.len(), 1);
        assert_eq!(footnotes[0].r#type, FootnoteType::Note);
    }
}
