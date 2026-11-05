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

//! Help of the typeset built-in family

use super::syntax::OptionSpec;
use crate::common::help::{Help, append_manual_link, append_option_table, append_usage};
use std::borrow::Cow;

/// Help of the typeset built-in and the built-ins that share its parser
///
/// This is like [`BuiltinHelp`](crate::common::help::BuiltinHelp), but takes
/// the option specs of the typeset built-in family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TypesetHelp {
    /// One-line summary of the built-in
    pub summary: &'static str,
    /// Usage lines, each showing a form of the command line
    pub usage: &'static [&'static str],
    /// Option specs of the built-in
    pub options: &'static [OptionSpec<'static>],
    /// Name of the manual page without the `.html` extension
    pub manual_page_name: &'static str,
}

fn option_name(spec: &OptionSpec) -> String {
    match spec.short {
        Some(short) => format!("-{short}, --{}", spec.long),
        None => format!("    --{}", spec.long),
    }
}

impl Help for TypesetHelp {
    fn summary(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.summary)
    }

    fn message(&self) -> Cow<'_, str> {
        let mut message = String::new();
        append_usage(&mut message, self.usage);

        if !self.options.is_empty() {
            message.push_str("Options:\n");
            let rows = self
                .options
                .iter()
                .map(|spec| (option_name(spec), spec.description))
                .collect::<Vec<_>>();
            append_option_table(&mut message, &rows);
            message.push('\n');
        }

        append_manual_link(&mut message, self.manual_page_name);
        Cow::Owned(message)
    }
}

/// Help information of the `typeset` built-in
pub const HELP: &dyn Help = &TypesetHelp {
    summary: "define variables and functions with attributes",
    usage: &[
        "typeset [-grx] [+rx] NAME[=VALUE]...",
        "typeset -p [-grx] [+rx] [NAME...]",
        "typeset [-grx] [+rx]",
        "typeset -f [-r] [+r] NAME...",
        "typeset -fp [-r] [+r] [NAME...]",
        "typeset -f [-r] [+r]",
    ],
    options: super::syntax::ALL_OPTIONS,
    manual_page_name: "typeset",
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typeset::syntax::{HELP_OPTION, PRINT_OPTION};

    #[test]
    fn help_message() {
        static HELP: TypesetHelp = TypesetHelp {
            summary: "do something",
            usage: &["foo [-p]"],
            options: &[PRINT_OPTION, HELP_OPTION],
            manual_page_name: "foo",
        };

        assert_eq!(
            HELP.message(),
            "\
Usage:
  foo [-p]

Options:
  -p, --print  print the current definitions
      --help   print this help

See <https://magicant.github.io/yash-rs/builtins/foo.html> for details.
"
        );
    }
}
