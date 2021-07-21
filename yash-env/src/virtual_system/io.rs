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

//! I/O within a virtual system.

use super::INode;
use nix::errno::Errno;
use nix::libc::off_t;
use nix::unistd::Whence;
use std::cell::RefCell;
use std::convert::TryFrom;
use std::rc::Rc;
use std::rc::Weak;

/// Abstract handle to perform I/O with.
pub trait OpenFileDescription {
    /// Returns true if you can read from this open file description.
    fn is_readable(&self) -> bool;

    /// Returns true if you can write to this open file description.
    fn is_writable(&self) -> bool;

    /// Reads from this open file description.
    ///
    /// Returns the number of bytes successfully read.
    fn read(&mut self, buffer: &mut [u8]) -> nix::Result<usize>;

    /// Writes to this open file description.
    ///
    /// Returns the number of bytes successfully written.
    fn write(&mut self, buffer: &[u8]) -> nix::Result<usize>;

    /// Moves the file offset and returns the new offset.
    fn seek(&mut self, offset: off_t, whence: Whence) -> nix::Result<off_t>;
}

/// State of a file opened for reading and/or writing.
#[derive(Clone, Debug, Eq)]
pub struct OpenFile {
    /// The file.
    pub file: Rc<RefCell<INode>>,
    /// Position in bytes to perform next I/O operation at.
    pub offset: usize,
    /// Whether this file is opened for reading.
    pub is_readable: bool,
    /// Whether this file is opened for writing.
    pub is_writable: bool,
}

impl PartialEq for OpenFile {
    /// Compares two `OpenFile`s.
    ///
    /// Returns true if the two files are the same i-node and shares the same
    /// readability and writability.
    fn eq(&self, rhs: &Self) -> bool {
        Rc::ptr_eq(&self.file, &rhs.file)
            && self.is_readable == rhs.is_readable
            && self.is_writable == rhs.is_writable
    }
}

impl OpenFileDescription for OpenFile {
    fn is_readable(&self) -> bool {
        self.is_readable
    }

    fn is_writable(&self) -> bool {
        self.is_writable
    }

    fn read(&mut self, mut buffer: &mut [u8]) -> nix::Result<usize> {
        if !self.is_readable {
            return Err(Errno::EBADF.into());
        }
        let file = self.file.borrow();
        let len = file.content.len();
        if self.offset >= len {
            return Ok(0);
        }
        let limit = len - self.offset;
        if buffer.len() > limit {
            buffer = &mut buffer[..limit];
        }
        let count = buffer.len();
        let src = &file.content[self.offset..][..count];
        buffer.copy_from_slice(src);
        self.offset += count;
        Ok(count)
    }

    fn write(&mut self, buffer: &[u8]) -> nix::Result<usize> {
        if !self.is_writable {
            return Err(Errno::EBADF.into());
        }
        let mut file = self.file.borrow_mut();
        let len = file.content.len();
        let count = buffer.len();
        if self.offset > len {
            let zeroes = self.offset - len;
            file.content.reserve(zeroes + count);
            file.content.resize_with(self.offset, u8::default);
        }
        let limit = count.min(file.content.len() - self.offset);
        let dst = &mut file.content[self.offset..][..limit];
        dst.copy_from_slice(&buffer[..limit]);
        file.content.reserve(count - limit);
        file.content.extend(&buffer[limit..]);
        self.offset += count;
        Ok(count)
    }

    /// Moves the file offset and returns the new offset.
    ///
    /// The current implementation for `OpenFileDescription` does not support
    /// `Whence::SeekHole` or `Whence::SeekData`.
    fn seek(&mut self, offset: off_t, whence: Whence) -> nix::Result<off_t> {
        let base = match whence {
            Whence::SeekSet => 0,
            Whence::SeekCur => self.offset,
            Whence::SeekEnd => self.file.borrow().content.len(),
            #[allow(unreachable_patterns)]
            _ => return Err(Errno::EINVAL.into()),
        };

        fn add(a: usize, b: off_t) -> Option<off_t> {
            off_t::try_from(a).ok()?.checked_add(b)
        }

        let new_offset = add(base, offset).ok_or(nix::Error::Sys(Errno::EOVERFLOW))?;
        self.offset = usize::try_from(new_offset).map_err(|_| nix::Error::Sys(Errno::EINVAL))?;
        Ok(new_offset)
    }
}

/// Unnamed FIFO byte channel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pipe {
    /// Bytes that have been written to (but not yet read from) the pipe.
    pub content: Vec<u8>,
}

impl Pipe {
    /// Creates a new pipe.
    pub fn new() -> Pipe {
        Pipe::default()
    }
}

/// Reading end of a [`Pipe`].
#[derive(Clone, Debug)]
pub struct PipeReader {
    pub pipe: Rc<RefCell<Pipe>>,
}

impl PartialEq for PipeReader {
    /// Compares two `PipeReader`s.
    ///
    /// Returns true if the two readers are reading from the same pipe.
    fn eq(&self, rhs: &Self) -> bool {
        Rc::ptr_eq(&self.pipe, &rhs.pipe)
    }
}

impl Eq for PipeReader {}

impl OpenFileDescription for PipeReader {
    fn is_readable(&self) -> bool {
        true
    }
    fn is_writable(&self) -> bool {
        false
    }
    fn read(&mut self, mut buffer: &mut [u8]) -> nix::Result<usize> {
        let mut pipe = self.pipe.borrow_mut();
        let limit = pipe.content.len();
        if limit == 0 && Rc::weak_count(&self.pipe) > 0 {
            return Err(Errno::EAGAIN.into());
        }
        if buffer.len() > limit {
            buffer = &mut buffer[..limit];
        }
        let count = buffer.len();
        buffer.copy_from_slice(&pipe.content[..count]);
        pipe.content.drain(..count);
        Ok(count)
    }
    fn write(&mut self, _buffer: &[u8]) -> nix::Result<usize> {
        Err(Errno::EBADF.into())
    }
    fn seek(&mut self, _offset: off_t, _whence: Whence) -> nix::Result<off_t> {
        Err(Errno::ESPIPE.into())
    }
}

/// Writing end of a [`Pipe`].
#[derive(Clone, Debug)]
pub struct PipeWriter {
    pub pipe: Weak<RefCell<Pipe>>,
}

impl PartialEq for PipeWriter {
    /// Compares two `PipeWriter`s.
    ///
    /// Returns true if the two readers are reading from the same pipe.
    fn eq(&self, rhs: &Self) -> bool {
        Weak::ptr_eq(&self.pipe, &rhs.pipe)
    }
}

impl Eq for PipeWriter {}

impl OpenFileDescription for PipeWriter {
    fn is_readable(&self) -> bool {
        false
    }
    fn is_writable(&self) -> bool {
        true
    }
    fn read(&mut self, _buffer: &mut [u8]) -> nix::Result<usize> {
        Err(Errno::EBADF.into())
    }
    fn write(&mut self, buffer: &[u8]) -> nix::Result<usize> {
        let pipe = match self.pipe.upgrade() {
            // TODO SIGPIPE
            None => return Err(Errno::EPIPE.into()),
            Some(pipe) => pipe,
        };
        let mut pipe = pipe.borrow_mut();
        // TODO EAGAIN if full
        pipe.content.extend(buffer);
        Ok(buffer.len())
    }
    fn seek(&mut self, _offset: off_t, _whence: Whence) -> nix::Result<off_t> {
        Err(Errno::ESPIPE.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_file_read_unreadable() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new())),
            offset: 0,
            is_readable: false,
            is_writable: false,
        };

        let mut buffer = [0];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Err(Errno::EBADF.into()));
    }

    #[test]
    fn open_file_read_beyond_file_length() {
        let mut inode = INode::new();
        inode.content.push(1);
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(inode)),
            offset: 1,
            is_readable: true,
            is_writable: false,
        };

        let mut buffer = [0];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(0));
        assert_eq!(open_file.offset, 1);

        open_file.offset = 2;
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(0));
        assert_eq!(open_file.offset, 2);
    }

    #[test]
    fn open_file_read_more_than_content() {
        let mut inode = INode::new();
        inode.content = vec![1, 2, 3];
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(inode)),
            offset: 1,
            is_readable: true,
            is_writable: false,
        };

        let mut buffer = [0; 3];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(2));
        assert_eq!(open_file.offset, 3);
        assert_eq!(buffer[..2], [2, 3]);
    }

    #[test]
    fn open_file_read_less_than_content() {
        let mut inode = INode::new();
        inode.content = vec![1, 2, 3, 4, 5];
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(inode)),
            offset: 1,
            is_readable: true,
            is_writable: false,
        };

        let mut buffer = [0; 3];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(3));
        assert_eq!(open_file.offset, 4);
        assert_eq!(buffer, [2, 3, 4]);
    }

    #[test]
    fn open_file_write_unwritable() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new())),
            offset: 0,
            is_readable: false,
            is_writable: false,
        };

        let result = open_file.write(&[0]);
        assert_eq!(result, Err(Errno::EBADF.into()));
    }

    #[test]
    fn open_file_write_less_than_content() {
        let mut inode = INode::new();
        inode.content = vec![1, 2, 3, 4, 5];
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(inode)),
            offset: 1,
            is_readable: false,
            is_writable: true,
        };

        let result = open_file.write(&[9, 8, 7]);
        assert_eq!(result, Ok(3));
        assert_eq!(open_file.offset, 4);
        assert_eq!(open_file.file.borrow().content, [1, 9, 8, 7, 5]);
    }

    #[test]
    fn open_file_write_more_than_content() {
        let mut inode = INode::new();
        inode.content = vec![1, 2, 3];
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(inode)),
            offset: 1,
            is_readable: false,
            is_writable: true,
        };

        let result = open_file.write(&[9, 8, 7, 6]);
        assert_eq!(result, Ok(4));
        assert_eq!(open_file.offset, 5);
        assert_eq!(open_file.file.borrow().content, [1, 9, 8, 7, 6]);
    }

    #[test]
    fn open_file_write_beyond_file_length() {
        let mut inode = INode::new();
        inode.content.push(1);
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(inode)),
            offset: 3,
            is_readable: false,
            is_writable: true,
        };

        let result = open_file.write(&[2, 3]);
        assert_eq!(result, Ok(2));
        assert_eq!(open_file.offset, 5);
        assert_eq!(open_file.file.borrow().content, [1, 0, 0, 2, 3]);
    }

    #[test]
    fn open_file_seek_set() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new())),
            offset: 3,
            is_readable: true,
            is_writable: true,
        };

        let result = open_file.seek(10, Whence::SeekSet);
        assert_eq!(result, Ok(10));
        assert_eq!(open_file.offset, 10);

        let result = open_file.seek(0, Whence::SeekSet);
        assert_eq!(result, Ok(0));
        assert_eq!(open_file.offset, 0);

        let result = open_file.seek(3, Whence::SeekSet);
        assert_eq!(result, Ok(3));
        assert_eq!(open_file.offset, 3);

        let result = open_file.seek(-1, Whence::SeekSet);
        assert_eq!(result, Err(Errno::EINVAL.into()));
        assert_eq!(open_file.offset, 3);
    }

    #[test]
    fn open_file_seek_cur() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new())),
            offset: 5,
            is_readable: true,
            is_writable: true,
        };

        let result = open_file.seek(10, Whence::SeekCur);
        assert_eq!(result, Ok(15));
        assert_eq!(open_file.offset, 15);

        let result = open_file.seek(0, Whence::SeekCur);
        assert_eq!(result, Ok(15));
        assert_eq!(open_file.offset, 15);

        let result = open_file.seek(-5, Whence::SeekCur);
        assert_eq!(result, Ok(10));
        assert_eq!(open_file.offset, 10);

        let result = open_file.seek(-11, Whence::SeekCur);
        assert_eq!(result, Err(Errno::EINVAL.into()));
        assert_eq!(open_file.offset, 10);
    }

    #[test]
    fn open_file_seek_end() {
        let mut inode = INode::new();
        inode.content = vec![1, 2, 3];
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(inode)),
            offset: 2,
            is_readable: true,
            is_writable: true,
        };

        let result = open_file.seek(7, Whence::SeekEnd);
        assert_eq!(result, Ok(10));
        assert_eq!(open_file.offset, 10);

        let result = open_file.seek(0, Whence::SeekEnd);
        assert_eq!(result, Ok(3));
        assert_eq!(open_file.offset, 3);

        let result = open_file.seek(-3, Whence::SeekEnd);
        assert_eq!(result, Ok(0));
        assert_eq!(open_file.offset, 0);

        let result = open_file.seek(-4, Whence::SeekEnd);
        assert_eq!(result, Err(Errno::EINVAL.into()));
        assert_eq!(open_file.offset, 0);
    }

    #[test]
    fn pipe_read_write() {
        let pipe = Rc::new(RefCell::new(Pipe::new()));
        let mut writer = PipeWriter {
            pipe: Rc::downgrade(&pipe),
        };
        let mut reader = PipeReader { pipe };

        let mut buffer = [100; 5];
        let result = reader.read(&mut buffer);
        assert_eq!(result, Err(Errno::EAGAIN.into()));

        let result = writer.write(&[1, 2, 3, 4, 5, 9, 8]);
        assert_eq!(result, Ok(7));

        let result = reader.read(&mut buffer);
        assert_eq!(result, Ok(5));
        assert_eq!(buffer, [1, 2, 3, 4, 5]);

        let result = writer.write(&[0, 1]);
        assert_eq!(result, Ok(2));

        let result = reader.read(&mut buffer);
        assert_eq!(result, Ok(4));
        assert_eq!(buffer[..4], [9, 8, 0, 1]);

        let result = reader.read(&mut buffer);
        assert_eq!(result, Err(Errno::EAGAIN.into()));

        drop(writer);

        let result = reader.read(&mut buffer);
        assert_eq!(result, Ok(0));
    }
}
