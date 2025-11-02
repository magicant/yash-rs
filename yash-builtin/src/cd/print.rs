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

//! Part of the cd built-in that prints the new working directory

use super::target::Origin;
use crate::common::report::prepare_report_message_and_divert;
use yash_env::Env;
use yash_env::io::Fd;
use yash_env::path::Path;
use yash_env::source::pretty::{Report, ReportType};
use yash_env::system::Errno;

impl Origin {
    /// Whether the built-in should print the target directory path.
    pub fn should_print_path(&self) -> bool {
        use Origin::*;
        match self {
            Oldpwd | Cdpath => true,
            Home | Literal => false,
        }
    }
}

/// Prints the new working directory path if needed.
pub async fn print_path(env: &mut Env, path: &Path, origin: &Origin) {
    if !origin.should_print_path() {
        return;
    }

    let line = format!("{}\n", path.display());
    match env.system.write_all(Fd::STDOUT, line.as_bytes()).await {
        Ok(_) => (),
        Err(errno) => handle_print_error(env, errno).await,
    }
}

/// Prints a warning message for a failed print.
///
/// The message is only a warning because it does not affect the exit status.
async fn handle_print_error(env: &mut Env, errno: Errno) {
    let mut report = Report::new();
    report.r#type = ReportType::Warning;
    report.title = format!("cannot print new $PWD: {errno}").into();
    let (message, _divert) = prepare_report_message_and_divert(env, report);
    env.system.print_error(&message).await;
}
