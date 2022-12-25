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

//! Here-documents

use super::Error;
use super::ErrorCause;
use super::FdSpec;
use crate::expansion::expand_text;
use std::path::Path;
use yash_env::io::Fd;
use yash_env::semantics::ExitStatus;
use yash_env::system::Errno;
use yash_env::Env;
use yash_env::System;
use yash_syntax::source::Location;
use yash_syntax::syntax::HereDoc;

async fn fill_content(env: &mut Env, fd: Fd, content: &str) -> Result<(), Errno> {
    env.system.write_all(fd, content.as_bytes()).await?;
    env.system.lseek(fd, std::io::SeekFrom::Start(0))?;
    Ok(())
}

/// Opens a here-document.
///
/// This function writes the here-document content to an anonymous temporary
/// file and returns a file descriptor to the file you can read the content
/// from.
pub(super) async fn open_fd(env: &mut Env, content: String) -> Result<Fd, ErrorCause> {
    // TODO Use a pipe for short content
    let fd = match env.system.open_tmpfile(Path::new("/tmp")) {
        Ok(fd) => fd,
        Err(errno) => return Err(ErrorCause::TemporaryFileUnavailable(errno)),
    };
    match fill_content(env, fd, &content).await {
        Ok(()) => Ok(fd),
        Err(errno) => {
            let _ = env.system.close(fd);
            Err(ErrorCause::TemporaryFileUnavailable(errno))
        }
    }
}

/// Opens a here-document.
///
/// This function expands the here-document content to an anonymous temporary
/// file and returns a file descriptor to the file you can read the content
/// from.
#[allow(clippy::await_holding_refcell_ref)]
pub(super) async fn open(
    env: &mut Env,
    here_doc: &HereDoc,
) -> Result<(FdSpec, Location, Option<ExitStatus>), Error> {
    let (content, exit_status) = expand_text(env, &here_doc.content.borrow()).await?;
    let location = here_doc.delimiter.location.clone();
    match open_fd(env, content).await {
        Ok(fd) => Ok((FdSpec::Owned(fd), location, exit_status)),
        Err(cause) => Err(Error { cause, location }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use yash_env::System;
    use yash_syntax::syntax::Text;

    #[test]
    fn open_fd_and_read_from_it() {
        let text = "Here document content\n";
        let mut env = Env::new_virtual();
        let fd = open_fd(&mut env, text.to_owned())
            .now_or_never()
            .unwrap()
            .unwrap();

        let mut buffer = [0; 30];
        let count = env.system.read(fd, &mut buffer).unwrap();
        assert_eq!(std::str::from_utf8(&buffer[..count]), Ok(text));
    }

    #[test]
    fn empty_here_doc() {
        let mut env = Env::new_virtual();
        let here_doc = HereDoc {
            delimiter: "END".parse().unwrap(),
            remove_tabs: false,
            content: RefCell::new(Text(vec![])),
        };

        let (fd_spec, location, exit_status) =
            open(&mut env, &here_doc).now_or_never().unwrap().unwrap();
        assert_matches!(fd_spec, FdSpec::Owned(fd) => {
            let mut buffer = [0; 1];
            let read_count = env.system.read(fd, &mut buffer).unwrap();
            assert_eq!(read_count, 0);
        });
        assert_eq!(location, here_doc.delimiter.location);
        assert_eq!(exit_status, None);
    }

    #[test]
    fn here_doc_with_command_substitution() {
        in_virtual_system(|mut env, _pid, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            env.builtins.insert("return", return_builtin());
            let here_doc = HereDoc {
                delimiter: "EOF".parse().unwrap(),
                remove_tabs: false,
                content: RefCell::new("$(echo foo)$(echo bar; return -n 42)\n".parse().unwrap()),
            };

            let (fd_spec, location, exit_status) = open(&mut env, &here_doc).await.unwrap();
            assert_matches!(fd_spec, FdSpec::Owned(fd) => {
                let mut buffer = [0; 8];
                let read_count = env.system.read(fd, &mut buffer).unwrap();
                assert_eq!(read_count, 7);
                assert_eq!(&buffer[..7], b"foobar\n");
            });
            assert_eq!(location, here_doc.delimiter.location);
            assert_eq!(exit_status, Some(ExitStatus(42)));
        })
    }
}
