// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2022 WATANABE Yuki
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

//! Jobs built-in
//!
//! TODO Elaborate

use crate::common::Print;
use std::fmt::Write;
use std::future::Future;
use std::ops::ControlFlow::Continue;
use std::pin::Pin;
use yash_env::builtin::Result;
use yash_env::job::WaitStatus;
use yash_env::semantics::Field;
use yash_env::Env;

/// Implementation of the jobs built-in.
pub async fn builtin_body(env: &mut Env, _args: Vec<Field>) -> Result {
    // Print jobs.
    let mut print = String::new();
    let current_job_index = env.jobs.current_job().map(|(index, _)| index);
    let previous_job_index = env.jobs.previous_job().map(|(index, _)| index);
    env.jobs.report_jobs(|index, job| {
        let number = index + 1;
        let mark = if Some(index) == current_job_index {
            '+'
        } else if Some(index) == previous_job_index {
            '-'
        } else {
            ' '
        };
        let status = match job.status {
            WaitStatus::Stopped(_, signal) => signal.as_str(),
            WaitStatus::Exited(_, _) => "Done",
            WaitStatus::Signaled(_, signal, _) => signal.as_str(),
            _ => "Running",
        };
        let name = &job.name;
        writeln!(print, "[{number}] {mark} {status:10} {name}").unwrap();
        true
    });

    // Remove terminated jobs
    env.jobs.retain_jobs(|_, job| {
        matches!(
            job.status,
            WaitStatus::StillAlive | WaitStatus::Continued(_)
        )
    });

    (env.print(&print).await, Continue(()))
}

/// Wrapper of [`builtin_body`] that returns the future in a pinned box.
pub fn builtin_main(env: &mut Env, args: Vec<Field>) -> Pin<Box<dyn Future<Output = Result> + '_>> {
    Box::pin(builtin_body(env, args))
}
