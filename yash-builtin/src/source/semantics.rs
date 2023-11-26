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
use crate::common::report_failure;
use std::ffi::CStr;
use std::ffi::CString;
use std::num::NonZeroU64;
use std::ops::ControlFlow;
use yash_env::input::FdReader;
use yash_env::io::Fd;
use yash_env::semantics::Divert;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::stack::Frame;
use yash_env::system::Errno;
use yash_env::system::Mode;
use yash_env::system::OFlag;
use yash_env::system::System;
use yash_env::system::SystemEx as _;
use yash_env::Env;
use yash_semantics::ReadEvalLoop;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::pretty::Annotation;
use yash_syntax::source::pretty::AnnotationType;
use yash_syntax::source::pretty::Message;
use yash_syntax::source::Source;

impl Command {
    /// Executes the `.` built-in.
    ///
    /// If the file is not found or cannot be read, this method reports an error
    /// to the standard error and returns `ExitStatus::FAILURE.into()`.
    pub async fn execute(self, env: &mut Env) -> crate::Result {
        let env = &mut env.push_frame(Frame::DotScript);

        let fd = match find_and_open_file(env, &self.file) {
            Ok(fd) => fd,
            Err(errno) => return report_find_and_open_file_failure(env, &self.file, errno).await,
        };

        // TODO set positional parameters

        // Parse and execute the command script
        let input = Box::new(FdReader::new(fd, env.system.clone()));
        let start_line_number = NonZeroU64::new(1).unwrap();
        let source = Source::DotScript {
            name: self.file.value,
            origin: self.file.origin,
        };
        let mut lexer = Lexer::new(input, start_line_number, source);
        let divert = ReadEvalLoop::new(env, &mut lexer).run().await;

        _ = env.system.close(fd);

        let (exit_status, divert) = consume_return(divert);
        let exit_status = exit_status.unwrap_or(env.exit_status);
        crate::Result::with_exit_status_and_divert(exit_status, divert)
    }
}

/// Finds and opens the file to be executed.
fn find_and_open_file(env: &mut Env, name: &Field) -> Result<Fd, Errno> {
    // TODO Search PATH
    let path = CString::new(name.value.as_bytes()).map_err(|_| Errno::EILSEQ)?;
    open_file(&mut env.system, &path)
}

/// Opens the file to be executed.
///
/// The returned file descriptor is opened with the `O_CLOEXEC` flag and is at
/// least [`MIN_INTERNAL_FD`](yash_env::io::MIN_INTERNAL_FD).
fn open_file<S: System>(system: &mut S, path: &CStr) -> Result<Fd, Errno> {
    system
        .open(path, OFlag::O_RDONLY | OFlag::O_CLOEXEC, Mode::empty())
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
    let message = Message {
        r#type: AnnotationType::Error,
        title: "cannot open script file".into(),
        annotations: vec![Annotation::new(
            AnnotationType::Error,
            format!("`{}` ({errno})", name.value).into(),
            &name.origin,
        )],
    };
    report_failure(env, message).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use futures_util::FutureExt as _;
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;
    use yash_env::io::MIN_INTERNAL_FD;
    use yash_env::system::r#virtual::INode;
    use yash_env::system::FdFlag;
    use yash_env::VirtualSystem;

    fn system_with_file<P: AsRef<Path>, C: Into<Vec<u8>>>(path: P, content: C) -> VirtualSystem {
        let system = VirtualSystem::new();
        let mut state = system.state.borrow_mut();
        let content = Rc::new(RefCell::new(INode::new(content)));
        state.file_system.save(path, content).unwrap();
        drop(state);
        system
    }

    #[test]
    fn open_file_result_lower_bound() {
        let mut system = system_with_file("/foo/file", "");
        let path = CString::new("/foo/file").unwrap();
        let result = open_file(&mut system, &path);
        assert_matches!(result, Ok(fd) if fd >= MIN_INTERNAL_FD);
    }

    #[test]
    fn open_file_result_cloexec() {
        let mut system = system_with_file("/foo/file", "");
        let path = CString::new("/foo/file").unwrap();

        let fd = open_file(&mut system, &path).unwrap();

        let process = system.current_process();
        let fd_body = process.get_fd(fd).unwrap();
        assert_eq!(fd_body.flag, FdFlag::FD_CLOEXEC);
    }

    #[test]
    fn fd_is_closed_after_execute() {
        let system = system_with_file("/foo/file", "");
        let mut env = Env::with_system(Box::new(system.clone()));
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
