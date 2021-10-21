// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2021 WATANABE Yuki
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

//! Error handlers.

use crate::ExitStatus;
use crate::Handle;
use annotate_snippets::display_list::DisplayList;
use annotate_snippets::snippet::Snippet;
use async_trait::async_trait;
use std::ops::ControlFlow::Continue;
use yash_env::io::Fd;
use yash_env::Env;
use yash_syntax::source::pretty::Message;

#[async_trait(?Send)]
impl Handle for crate::expansion::Error {
    /// Prints an error message and sets the exit status to non-zero.
    ///
    /// This function handles an expansion error by printing an error message
    /// that describes the error to the standard error and setting the exit
    /// status to [`ExitStatus::ERROR`]. Note that other POSIX-compliant
    /// implementations may use different non-zero exit statuses.
    async fn handle(&self, env: &mut Env) -> super::Result {
        let m = Message::from(self);
        let mut s = Snippet::from(&m);
        s.opt.color = true;
        let f = format!("{}\n", DisplayList::from(s));
        let _ = env.system.write_all(Fd::STDERR, f.as_bytes()).await;

        env.exit_status = ExitStatus::ERROR;
        Continue(())
    }
}

#[async_trait(?Send)]
impl Handle for crate::redir::Error {
    /// Prints an error message and sets the exit status to non-zero.
    ///
    /// This function handles a redirection error by printing an error message
    /// that describes the error to the standard error and setting the exit
    /// status to [`ExitStatus::ERROR`]. Note that other POSIX-compliant
    /// implementations may use different non-zero exit statuses.
    async fn handle(&self, env: &mut Env) -> super::Result {
        let m = Message::from(self);
        let mut s = Snippet::from(&m);
        s.opt.color = true;
        let f = format!("{}\n", DisplayList::from(s));
        let _ = env.system.write_all(Fd::STDERR, f.as_bytes()).await;

        env.exit_status = ExitStatus::ERROR;
        Continue(())
    }
}
