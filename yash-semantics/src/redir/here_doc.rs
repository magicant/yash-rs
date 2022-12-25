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

use super::ErrorCause;
use std::path::Path;
use yash_env::io::Fd;
use yash_env::system::Errno;
use yash_env::Env;
use yash_env::System;

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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;
    use yash_env::System;

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
}
