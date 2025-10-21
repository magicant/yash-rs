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

//! Implementation of the main behavior of the `.` built-in

use super::Command;
use crate::common::report::report_failure;
use std::cell::RefCell;
use std::ffi::CStr;
use std::ffi::CString;
use std::ops::ControlFlow;
use std::rc::Rc;
use yash_env::Env;
use yash_env::input::Echo;
use yash_env::input::FdReader;
use yash_env::io::Fd;
use yash_env::path::PathBuf;
use yash_env::semantics::{Divert, ExitStatus, Field, RunReadEvalLoop};
use yash_env::stack::Frame;
use yash_env::system::Errno;
use yash_env::system::Mode;
use yash_env::system::OfdAccess;
use yash_env::system::OpenFlag;
use yash_env::system::System;
use yash_env::system::SystemEx as _;
use yash_env::variable::PATH;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Source;
use yash_syntax::source::pretty::{Report, ReportType, Snippet};

impl Command {
    /// Executes the `.` built-in.
    ///
    /// If the file is not found or cannot be read, this method reports an error
    /// to the standard error and returns `ExitStatus::FAILURE.into()`.
    pub async fn execute(self, env: &mut Env) -> crate::Result {
        let env = &mut *env.push_frame(Frame::DotScript);

        let fd = match find_and_open_file(env, &self.file.value) {
            Ok(fd) => fd,
            Err(errno) => return report_find_and_open_file_failure(env, &self.file, errno).await,
        };

        // TODO set positional parameters

        // Parse and execute the command script
        let run_read_eval_loop = env
            .any
            .get::<RunReadEvalLoop>()
            .cloned()
            .expect("`source` built-in requires `RunReadEvalLoop` in `Env::any`");
        let system = env.system.clone();
        let ref_env = RefCell::new(&mut *env);
        let mut config = Lexer::config();
        config.source = Some(Rc::new(Source::DotScript {
            name: self.file.value,
            origin: self.file.origin,
        }));
        let input = Box::new(Echo::new(FdReader::new(fd, system), &ref_env));
        let mut lexer = config.input(input);
        let divert = run_read_eval_loop.0(&ref_env, &mut { lexer }).await;

        _ = env.system.close(fd);

        let (exit_status, divert) = consume_return(divert);
        let exit_status = exit_status.unwrap_or(env.exit_status);
        crate::Result::with_exit_status_and_divert(exit_status, divert)
    }
}

/// Finds and opens the file to be executed.
///
/// If the name does not contain a slash, this function searches the file in the
/// `$PATH` variable.
fn find_and_open_file(env: &mut Env, filename: &str) -> Result<Fd, Errno> {
    let dirs: Box<dyn Iterator<Item = &str>> = if filename.contains('/') {
        Box::new(std::iter::once("."))
    } else {
        env.variables
            .get(PATH)
            .and_then(|v| v.value.as_ref())
            .map_or(Box::new(std::iter::empty()), |v| Box::new(v.split()))
        // TODO If not in POSIX mode, search in the current working directory too
    };

    // Iterate over the directories trying to open the file in each directory
    // and return the first successfully opened file descriptor.
    dirs.filter_map(|dir| {
        let path = PathBuf::from_iter([dir, filename])
            .into_unix_string()
            .into_vec();
        let c_path = CString::new(path).ok()?;
        open_file(&mut env.system, &c_path).ok()
    })
    .next()
    .ok_or(Errno::ENOENT)
}

/// Opens the file to be executed.
///
/// The returned file descriptor is opened with the `O_CLOEXEC` flag and is at
/// least [`MIN_INTERNAL_FD`](yash_env::io::MIN_INTERNAL_FD).
fn open_file<S: System>(system: &mut S, path: &CStr) -> Result<Fd, Errno> {
    system
        .open(
            path,
            OfdAccess::ReadOnly,
            OpenFlag::CloseOnExec.into(),
            Mode::empty(),
        )
        .and_then(|fd| system.move_fd_internal(fd))
}

/// Handles the result of the `return` built-in possibly executed in the
/// command script.
///
/// This function returns an optional exit status and a possibly modified
/// divert. The exit status should override the current exit status if it is
/// `Some`. The divert should be passed to the caller of the `.` built-in.
fn consume_return(divert: ControlFlow<Divert>) -> (Option<ExitStatus>, ControlFlow<Divert>) {
    match divert {
        ControlFlow::Break(Divert::Return(exit_status)) => (exit_status, ControlFlow::Continue(())),
        other => (None, other),
    }
}

/// Reports an error that occurred while preparing the file descriptor to read
/// from.
async fn report_find_and_open_file_failure(
    env: &mut Env,
    name: &Field,
    errno: Errno,
) -> crate::Result {
    let mut report = Report::new();
    report.r#type = ReportType::Error;
    report.title = "cannot open script file".into();
    report.snippets = Snippet::with_primary_span(&name.origin, format!("`{name}`: {errno}").into());
    report_failure(env, report).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use enumset::EnumSet;
    use futures_util::FutureExt as _;
    use std::cell::RefCell;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_env::io::MIN_INTERNAL_FD;
    use yash_env::path::Path;
    use yash_env::system::FdFlag;
    use yash_env::system::r#virtual::Inode;
    use yash_env::variable::Scope;

    fn system_with_file<P: AsRef<Path>, C: Into<Vec<u8>>>(path: P, content: C) -> VirtualSystem {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        let content = Rc::new(RefCell::new(Inode::new(content)));
        state.file_system.save(path, content).unwrap();
        drop(state);
        system
    }

    #[test]
    fn no_path_search_with_pathname_containing_slash() {
        let system = VirtualSystem::new();
        let inode = Rc::new(RefCell::new(Inode::new("")));
        {
            let mut state = system.state.borrow_mut();
            let content = Rc::new(RefCell::new(Inode::new("")));
            state.file_system.save("/bar/file", content).unwrap();
            let content = Rc::new(RefCell::new(Inode::new("")));
            state.file_system.save("/baz/file", content).unwrap();
            let content = Rc::clone(&inode);
            state.file_system.save("/file", content).unwrap();
        }
        let mut env = Env::with_system(Box::new(system.clone()));
        env.variables
            .get_or_new(PATH, Scope::Global)
            .assign("/foo:/bar:/baz", None)
            .unwrap();

        // The pathname parameter contains a slash, so the file is not searched
        // in the $PATH variable.
        let result = find_and_open_file(&mut env, "./file");

        // The expected file is "/file" since the default working directory is
        // "/".
        let fd = result.unwrap();
        _ = system.with_open_file_description(fd, |ofd| {
            assert!(Rc::ptr_eq(ofd.inode(), &inode));
            Ok(())
        });
    }

    #[test]
    fn file_found_in_path() {
        let system = VirtualSystem::new();
        let inode = Rc::new(RefCell::new(Inode::new("")));
        {
            let mut state = system.state.borrow_mut();
            let content = Rc::clone(&inode);
            state.file_system.save("/bar/file", content).unwrap();
            let content = Rc::new(RefCell::new(Inode::new("")));
            state.file_system.save("/baz/file", content).unwrap();
            let content = Rc::new(RefCell::new(Inode::new("")));
            state.file_system.save("/file", content).unwrap();
        }
        let mut env = Env::with_system(Box::new(system.clone()));
        env.variables
            .get_or_new(PATH, Scope::Global)
            .assign("/foo:/bar:/baz", None)
            .unwrap();

        // The pathname parameter does not contain a slash, so the file is
        // searched in the $PATH variable.
        let result = find_and_open_file(&mut env, "file");

        // The expected file is "/bar/file".
        let fd = result.unwrap();
        _ = system.with_open_file_description(fd, |ofd| {
            assert!(Rc::ptr_eq(ofd.inode(), &inode));
            Ok(())
        });
    }

    #[test]
    fn open_file_result_lower_bound() {
        let mut system = system_with_file("/foo/file", "");
        let result = open_file(&mut system, c"/foo/file");
        assert_matches!(result, Ok(fd) if fd >= MIN_INTERNAL_FD);
    }

    #[test]
    fn open_file_result_cloexec() {
        let mut system = system_with_file("/foo/file", "");
        let fd = open_file(&mut system, c"/foo/file").unwrap();

        let process = system.current_process();
        let fd_body = process.get_fd(fd).unwrap();
        assert_eq!(fd_body.flags, EnumSet::only(FdFlag::CloseOnExec));
    }

    #[test]
    fn fd_is_closed_after_execute() {
        let system = system_with_file("/foo/file", "");
        let mut env = Env::with_system(Box::new(system.clone()));
        env.any.insert(Box::new(RunReadEvalLoop(|_env, _lexer| {
            Box::pin(async { ControlFlow::Continue(()) })
        })));
        let command = Command {
            file: Field::dummy("/foo/file"),
            params: vec![],
        };

        _ = command.execute(&mut env).now_or_never().unwrap();

        let process = system.current_process();
        for fd in 3..50 {
            assert_matches!(process.get_fd(Fd(fd)), None, "fd={fd}");
        }
    }
}
