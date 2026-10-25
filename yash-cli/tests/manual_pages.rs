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

//! These tests check the manual pages under `docs/src/builtins` linked from
//! the help of the built-ins.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use yash_env::VirtualSystem;
use yash_env::system::Concurrent;

const BIN: &str = env!("CARGO_BIN_EXE_yash3");
const MANUAL_BASE_URL: &str = "https://magicant.github.io/yash-rs/builtins/";

/// Manual page linked from the help of a built-in
struct ManualPage {
    /// Path to the Markdown source of the page
    path: PathBuf,
    /// Whether the help lists the `--help` option
    lists_help_option: bool,
}

/// Returns the manual pages linked from the help of all the built-ins.
fn linked_manual_pages() -> Vec<ManualPage> {
    let names = yash_builtin::iter::<Rc<Concurrent<VirtualSystem>>>().map(|(name, _)| name);
    // The help built-in fails for built-ins that have no help, but still
    // prints the help of the others.
    let output = Command::new(BIN)
        .args(["-c", r#"help "$@""#, "yash"])
        .args(names)
        .output()
        .unwrap();
    let stdout = String::from_utf8(output.stdout).unwrap();

    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/src/builtins");
    let mut pages = Vec::new();
    let mut lists_help_option = false;
    for line in stdout.lines() {
        // Each help message ends with the link to the manual page.
        let Some(url) = line
            .strip_prefix("See <")
            .and_then(|line| line.strip_suffix(".html> for details."))
        else {
            lists_help_option |= line.trim_start().starts_with("--help ");
            continue;
        };
        let page = url
            .strip_prefix(MANUAL_BASE_URL)
            .unwrap_or_else(|| panic!("the URL in {line:?} should start with {MANUAL_BASE_URL}"));
        pages.push(ManualPage {
            path: docs.join(format!("{page}.md")),
            lists_help_option,
        });
        lists_help_option = false;
    }
    assert!(
        !pages.is_empty(),
        "the help should link to manual pages: {stdout:?}"
    );
    pages
}

#[test]
fn help_links_to_existing_manual_pages() {
    for page in linked_manual_pages() {
        assert!(page.path.is_file(), "{} should exist", page.path.display());
    }
}

#[test]
fn manual_pages_describe_help_option() {
    for page in linked_manual_pages() {
        if page.lists_help_option {
            let content = std::fs::read_to_string(&page.path).unwrap();
            assert!(
                content.contains("**`--help`**"),
                "{} should describe the --help option",
                page.path.display()
            );
        }
    }
}
