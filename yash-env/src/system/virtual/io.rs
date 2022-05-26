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

use super::FileBody;
use super::INode;
use nix::errno::Errno;
use nix::libc::off_t;
use nix::unistd::Whence;
use std::cell::RefCell;
use std::fmt::Debug;
use std::rc::Rc;
use std::rc::Weak;

/// Abstract handle to perform I/O with.
pub trait OpenFileDescription: Debug {
    /// Returns true if you can read from this open file description.
    fn is_readable(&self) -> bool;

    /// Returns true if you can write to this open file description.
    fn is_writable(&self) -> bool;

    /// Returns true if you can read from this open file description without
    /// blocking.
    fn is_ready_for_reading(&self) -> bool;

    /// Returns true if you can write to this open file description without
    /// blocking.
    fn is_ready_for_writing(&self) -> bool;

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

    /// Returns the i-node this open file description is operating on.
    fn i_node(&self) -> Rc<RefCell<INode>>;
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
    /// Whether this file is opened for appending.
    pub is_appending: bool,
}

/// Compares two `OpenFile`s.
///
/// Two files are considered equal iff they have the same i-node, readability,
/// and writability.
impl PartialEq for OpenFile {
    fn eq(&self, rhs: &Self) -> bool {
        Rc::ptr_eq(&self.file, &rhs.file)
            && self.is_readable == rhs.is_readable
            && self.is_writable == rhs.is_writable
    }
}

impl Drop for OpenFile {
    fn drop(&mut self) {
        if let FileBody::Fifo {
            readers, writers, ..
        } = &mut self.file.borrow_mut().body
        {
            if self.is_readable {
                *readers -= 1;
            }
            if self.is_writable {
                *writers -= 1;
            }
        }
    }
}

impl OpenFileDescription for OpenFile {
    fn is_readable(&self) -> bool {
        self.is_readable
    }

    fn is_writable(&self) -> bool {
        self.is_writable
    }

    fn is_ready_for_reading(&self) -> bool {
        match &self.file.borrow().body {
            FileBody::Regular { .. } | FileBody::Directory { .. } => true,
            FileBody::Fifo {
                content, writers, ..
            } => !self.is_readable || !content.is_empty() || *writers == 0,
        }
    }

    fn is_ready_for_writing(&self) -> bool {
        // TODO In case of a fifo, check if the fifo is writable and not full
        true
    }

    fn read(&mut self, mut buffer: &mut [u8]) -> nix::Result<usize> {
        if !self.is_readable {
            return Err(Errno::EBADF);
        }
        match &mut self.file.borrow_mut().body {
            FileBody::Regular { content, .. } => {
                let len = content.len();
                if self.offset >= len {
                    return Ok(0);
                }
                let limit = len - self.offset;
                if buffer.len() > limit {
                    buffer = &mut buffer[..limit];
                }
                let count = buffer.len();
                let src = &content[self.offset..][..count];
                buffer.copy_from_slice(src);
                self.offset += count;
                Ok(count)
            }
            FileBody::Fifo {
                content, writers, ..
            } => {
                let limit = content.len();
                if limit == 0 && *writers > 0 {
                    return Err(Errno::EAGAIN);
                }
                let mut count = 0;
                for to in buffer {
                    if let Some(from) = content.pop_front() {
                        *to = from;
                        count += 1;
                    } else {
                        break;
                    }
                }
                Ok(count)
            }
            FileBody::Directory { .. } => Err(Errno::EISDIR),
        }
    }

    fn write(&mut self, mut buffer: &[u8]) -> nix::Result<usize> {
        if !self.is_writable {
            return Err(Errno::EBADF);
        }
        match &mut self.file.borrow_mut().body {
            FileBody::Regular { content, .. } => {
                let len = content.len();
                let count = buffer.len();
                if self.is_appending {
                    self.offset = len;
                }
                if self.offset > len {
                    let zeroes = self.offset - len;
                    content.reserve(zeroes + count);
                    content.resize_with(self.offset, u8::default);
                }
                let limit = count.min(content.len() - self.offset);
                let dst = &mut content[self.offset..][..limit];
                dst.copy_from_slice(&buffer[..limit]);
                content.reserve(count - limit);
                content.extend(&buffer[limit..]);
                self.offset += count;
                Ok(count)
            }
            FileBody::Fifo {
                content, readers, ..
            } => {
                if *readers == 0 {
                    // TODO SIGPIPE
                    return Err(Errno::EPIPE);
                }
                let room = Pipe::PIPE_SIZE - content.len();
                if room < buffer.len() {
                    if room == 0 || buffer.len() <= Pipe::PIPE_BUF {
                        return Err(Errno::EAGAIN);
                    }
                    buffer = &buffer[..room];
                }
                content.extend(buffer);
                debug_assert!(content.len() <= Pipe::PIPE_SIZE);
                Ok(buffer.len())
            }
            FileBody::Directory { .. } => Err(Errno::EISDIR),
        }
    }

    /// Moves the file offset and returns the new offset.
    ///
    /// The current implementation for `OpenFileDescription` does not support
    /// `Whence::SeekHole` or `Whence::SeekData`.
    fn seek(&mut self, offset: off_t, whence: Whence) -> nix::Result<off_t> {
        let len = match &self.file.borrow().body {
            FileBody::Regular { content, .. } => content.len(),
            FileBody::Directory { files, .. } => files.len(),
            FileBody::Fifo { .. } => return Err(Errno::ESPIPE),
        };
        let base = match whence {
            Whence::SeekSet => 0,
            Whence::SeekCur => self.offset,
            Whence::SeekEnd => len,
            #[allow(unreachable_patterns)]
            _ => return Err(Errno::EINVAL),
        };

        fn add(a: usize, b: off_t) -> Option<off_t> {
            off_t::try_from(a).ok()?.checked_add(b)
        }

        let new_offset = add(base, offset).ok_or(Errno::EOVERFLOW)?;
        self.offset = usize::try_from(new_offset).map_err(|_| Errno::EINVAL)?;
        Ok(new_offset)
    }

    fn i_node(&self) -> Rc<RefCell<INode>> {
        Rc::clone(&self.file)
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

    /// Maximum number of bytes guaranteed to be atomic when writing to a pipe.
    ///
    /// This value is for the virtual system implementation.
    /// The real system may have a different configuration.
    pub const PIPE_BUF: usize = 512;

    /// Maximum number of bytes a pipe can hold at a time.
    ///
    /// This value is for the virtual system implementation.
    /// The real system may have a different configuration.
    pub const PIPE_SIZE: usize = Self::PIPE_BUF * 2;
}

/// Reading end of a [`Pipe`].
#[derive(Clone, Debug)]
pub struct PipeReader {
    pub pipe: Rc<RefCell<Pipe>>,
}

/// Compares two `PipeReader`s.
///
/// Two readers are considered equal iff they reads from the same pipe.
impl PartialEq for PipeReader {
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
    fn is_ready_for_reading(&self) -> bool {
        let pipe = self.pipe.borrow();
        !pipe.content.is_empty() || Rc::weak_count(&self.pipe) == 0
    }
    fn is_ready_for_writing(&self) -> bool {
        false
    }
    fn read(&mut self, mut buffer: &mut [u8]) -> nix::Result<usize> {
        let mut pipe = self.pipe.borrow_mut();
        let limit = pipe.content.len();
        if limit == 0 && Rc::weak_count(&self.pipe) > 0 {
            return Err(Errno::EAGAIN);
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
        Err(Errno::EBADF)
    }
    fn seek(&mut self, _offset: off_t, _whence: Whence) -> nix::Result<off_t> {
        Err(Errno::ESPIPE)
    }
    fn i_node(&self) -> Rc<RefCell<INode>> {
        todo!("should return a pipe i-node")
    }
}

/// Writing end of a [`Pipe`].
#[derive(Clone, Debug)]
pub struct PipeWriter {
    pub pipe: Weak<RefCell<Pipe>>,
}

/// Compares two `PipeWriter`s.
///
/// Two writers are considered equal iff they writes to the same pipe.
impl PartialEq for PipeWriter {
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
    fn is_ready_for_reading(&self) -> bool {
        false
    }
    fn is_ready_for_writing(&self) -> bool {
        // TODO Should depend on whether the pipe is full
        true
    }
    fn read(&mut self, _buffer: &mut [u8]) -> nix::Result<usize> {
        Err(Errno::EBADF)
    }
    fn write(&mut self, mut buffer: &[u8]) -> nix::Result<usize> {
        let pipe = match self.pipe.upgrade() {
            // TODO SIGPIPE
            None => return Err(Errno::EPIPE),
            Some(pipe) => pipe,
        };
        let mut pipe = pipe.borrow_mut();
        let room = Pipe::PIPE_SIZE - pipe.content.len();
        if room < buffer.len() {
            if room == 0 || buffer.len() <= Pipe::PIPE_BUF {
                return Err(Errno::EAGAIN);
            }
            buffer = &buffer[..room];
        }
        pipe.content.extend(buffer);
        debug_assert!(pipe.content.len() <= Pipe::PIPE_SIZE);
        Ok(buffer.len())
    }
    fn seek(&mut self, _offset: off_t, _whence: Whence) -> nix::Result<off_t> {
        Err(Errno::ESPIPE)
    }
    fn i_node(&self) -> Rc<RefCell<INode>> {
        todo!("should return a pipe i-node")
    }
}

/// State of a file descriptor.
#[derive(Clone, Debug)]
pub struct FdBody {
    /// Underlying open file description.
    pub open_file_description: Rc<RefCell<dyn OpenFileDescription>>,
    /// True if this FD has the CLOEXEC flag set.
    pub cloexec: bool,
}

impl PartialEq for FdBody {
    fn eq(&self, rhs: &Self) -> bool {
        Rc::ptr_eq(&self.open_file_description, &rhs.open_file_description)
            && self.cloexec == rhs.cloexec
    }
}

impl Eq for FdBody {}

#[cfg(test)]
mod tests {
    use super::super::Mode;
    use super::*;
    use assert_matches::assert_matches;
    use std::collections::VecDeque;

    #[test]
    fn regular_file_read_unreadable() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([]))),
            offset: 0,
            is_readable: false,
            is_writable: false,
            is_appending: false,
        };

        let mut buffer = [0];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn regular_file_read_beyond_file_length() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([1]))),
            offset: 1,
            is_readable: true,
            is_writable: false,
            is_appending: false,
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
    fn regular_file_read_more_than_content() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([1, 2, 3]))),
            offset: 1,
            is_readable: true,
            is_writable: false,
            is_appending: false,
        };

        let mut buffer = [0; 3];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(2));
        assert_eq!(open_file.offset, 3);
        assert_eq!(buffer[..2], [2, 3]);
    }

    #[test]
    fn regular_file_read_less_than_content() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([1, 2, 3, 4, 5]))),
            offset: 1,
            is_readable: true,
            is_writable: false,
            is_appending: false,
        };

        let mut buffer = [0; 3];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(3));
        assert_eq!(open_file.offset, 4);
        assert_eq!(buffer, [2, 3, 4]);
    }

    #[test]
    fn regular_file_write_unwritable() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([]))),
            offset: 0,
            is_readable: false,
            is_writable: false,
            is_appending: false,
        };

        let result = open_file.write(&[0]);
        assert_eq!(result, Err(Errno::EBADF));
    }

    #[test]
    fn regular_file_write_less_than_content() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([1, 2, 3, 4, 5]))),
            offset: 1,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        let result = open_file.write(&[9, 8, 7]);
        assert_eq!(result, Ok(3));
        assert_eq!(open_file.offset, 4);
        assert_matches!(
            &open_file.file.borrow().body,
            FileBody::Regular { content, .. } => {
                assert_eq!(content[..], [1, 9, 8, 7, 5]);
            }
        );
    }

    #[test]
    fn regular_file_write_more_than_content() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([1, 2, 3]))),
            offset: 1,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        let result = open_file.write(&[9, 8, 7, 6]);
        assert_eq!(result, Ok(4));
        assert_eq!(open_file.offset, 5);
        assert_matches!(
            &open_file.file.borrow().body,
            FileBody::Regular { content, .. } => {
                assert_eq!(content[..], [1, 9, 8, 7, 6]);
            }
        );
    }

    #[test]
    fn regular_file_write_beyond_file_length() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([1]))),
            offset: 3,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        let result = open_file.write(&[2, 3]);
        assert_eq!(result, Ok(2));
        assert_eq!(open_file.offset, 5);
        assert_matches!(
            &open_file.file.borrow().body,
            FileBody::Regular { content, .. } => {
                assert_eq!(content[..], [1, 0, 0, 2, 3]);
            }
        );
    }

    #[test]
    fn regular_file_write_appending() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([1, 2, 3]))),
            offset: 1,
            is_readable: false,
            is_writable: true,
            is_appending: true,
        };

        let result = open_file.write(&[4, 5]);
        assert_eq!(result, Ok(2));
        assert_eq!(open_file.offset, 5);
        assert_matches!(
            &open_file.file.borrow().body,
            FileBody::Regular { content, .. } => {
                assert_eq!(content[..], [1, 2, 3, 4, 5]);
            }
        );
    }

    #[test]
    fn regular_file_seek_set() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([]))),
            offset: 3,
            is_readable: true,
            is_writable: true,
            is_appending: false,
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
        assert_eq!(result, Err(Errno::EINVAL));
        assert_eq!(open_file.offset, 3);
    }

    #[test]
    fn regular_file_seek_cur() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([]))),
            offset: 5,
            is_readable: true,
            is_writable: true,
            is_appending: false,
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
        assert_eq!(result, Err(Errno::EINVAL));
        assert_eq!(open_file.offset, 10);
    }

    #[test]
    fn regular_file_seek_end() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode::new([1, 2, 3]))),
            offset: 2,
            is_readable: true,
            is_writable: true,
            is_appending: false,
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
        assert_eq!(result, Err(Errno::EINVAL));
        assert_eq!(open_file.offset, 0);
    }

    #[test]
    fn fifo_reader_drop() {
        let file = Rc::new(RefCell::new(INode {
            body: FileBody::Fifo {
                content: VecDeque::new(),
                readers: 1,
                writers: 1,
            },
            permissions: Mode::default(),
        }));
        let open_file = OpenFile {
            file: Rc::clone(&file),
            offset: 0,
            is_readable: true,
            is_writable: false,
            is_appending: false,
        };
        drop(open_file);

        assert_matches!(&file.borrow().body, FileBody::Fifo { readers, writers, .. } => {
            assert_eq!(*readers, 0);
            assert_eq!(*writers, 1);
        });
    }

    #[test]
    fn fifo_writer_drop() {
        let file = Rc::new(RefCell::new(INode {
            body: FileBody::Fifo {
                content: VecDeque::new(),
                readers: 1,
                writers: 1,
            },
            permissions: Mode::default(),
        }));
        let open_file = OpenFile {
            file: Rc::clone(&file),
            offset: 0,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };
        drop(open_file);

        assert_matches!(&file.borrow().body, FileBody::Fifo { readers, writers, .. } => {
            assert_eq!(*readers, 1);
            assert_eq!(*writers, 0);
        });
    }

    #[test]
    fn fifo_read_empty() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode {
                body: FileBody::Fifo {
                    content: VecDeque::new(),
                    readers: 1,
                    writers: 0,
                },
                permissions: Mode::default(),
            })),
            offset: 0,
            is_readable: true,
            is_writable: false,
            is_appending: false,
        };

        let mut buffer = [100; 5];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn fifo_read_non_empty() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode {
                body: FileBody::Fifo {
                    content: VecDeque::from([1, 5, 7, 3, 42, 7, 6]),
                    readers: 1,
                    writers: 0,
                },
                permissions: Mode::default(),
            })),
            offset: 0,
            is_readable: true,
            is_writable: false,
            is_appending: false,
        };

        let mut buffer = [100; 4];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(4));
        assert_eq!(buffer, [1, 5, 7, 3]);

        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(3));
        assert_eq!(buffer[..3], [42, 7, 6]);

        let result = open_file.read(&mut buffer);
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn fifo_read_not_ready() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode {
                body: FileBody::Fifo {
                    content: VecDeque::new(),
                    readers: 1,
                    writers: 1,
                },
                permissions: Mode::default(),
            })),
            offset: 0,
            is_readable: true,
            is_writable: false,
            is_appending: false,
        };

        let mut buffer = [100; 5];
        let result = open_file.read(&mut buffer);
        assert_eq!(result, Err(Errno::EAGAIN));
    }

    #[test]
    fn fifo_write_vacant() {
        let file = Rc::new(RefCell::new(INode {
            body: FileBody::Fifo {
                content: VecDeque::new(),
                readers: 1,
                writers: 1,
            },
            permissions: Mode::default(),
        }));
        let mut open_file = OpenFile {
            file: Rc::clone(&file),
            offset: 0,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        let result = open_file.write(&[1, 1, 2, 3]);
        assert_eq!(result, Ok(4));

        let result = open_file.write(&[5, 8, 13]);
        assert_eq!(result, Ok(3));

        assert_matches!(&mut file.borrow_mut().body, FileBody::Fifo { content, .. } => {
            assert_eq!(content.make_contiguous(), [1, 1, 2, 3, 5, 8, 13]);
        });
    }

    #[test]
    fn fifo_write_full() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode {
                body: FileBody::Fifo {
                    content: VecDeque::new(),
                    readers: 1,
                    writers: 1,
                },
                permissions: Mode::default(),
            })),
            offset: 0,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        open_file.write(&[0; Pipe::PIPE_SIZE]).unwrap();

        // The pipe is full. No more can be written.
        let result = open_file.write(&[1; 1]);
        assert_eq!(result, Err(Errno::EAGAIN));
        let result = open_file.write(&[1; Pipe::PIPE_BUF + 1]);
        assert_eq!(result, Err(Errno::EAGAIN));

        // However, empty write should succeed.
        let result = open_file.write(&[1; 0]);
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn fifo_write_atomic_full() {
        let file = Rc::new(RefCell::new(INode {
            body: FileBody::Fifo {
                content: VecDeque::new(),
                readers: 1,
                writers: 1,
            },
            permissions: Mode::default(),
        }));
        let mut open_file = OpenFile {
            file: Rc::clone(&file),
            offset: 0,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        const LEN: usize = Pipe::PIPE_SIZE - Pipe::PIPE_BUF + 1;
        open_file.write(&[0; LEN]).unwrap();

        // The remaining room in the pipe is less than the length we're writing,
        // which is PIPE_BUF. Nothing is written in this case.
        let result = open_file.write(&[1; Pipe::PIPE_BUF]);
        assert_eq!(result, Err(Errno::EAGAIN));

        assert_matches!(&file.borrow().body, FileBody::Fifo { content, .. } => {
            assert_eq!(content.len(), LEN);
        });
    }

    #[test]
    fn fifo_write_non_atomic_full() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode {
                body: FileBody::Fifo {
                    content: VecDeque::new(),
                    readers: 1,
                    writers: 1,
                },
                permissions: Mode::default(),
            })),
            offset: 0,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        const LEN: usize = Pipe::PIPE_SIZE - Pipe::PIPE_BUF;
        open_file.write(&[0; LEN]).unwrap();

        // The remaining room in the pipe is less than the length we're writing,
        // which exceeds PIPE_BUF. Only as much as possible is written in this
        // case.
        let result = open_file.write(&[1; Pipe::PIPE_BUF + 1]);
        assert_eq!(result, Ok(Pipe::PIPE_BUF));
    }

    #[test]
    fn fifo_write_orphan() {
        let mut open_file = OpenFile {
            file: Rc::new(RefCell::new(INode {
                body: FileBody::Fifo {
                    content: VecDeque::new(),
                    readers: 0,
                    writers: 1,
                },
                permissions: Mode::default(),
            })),
            offset: 0,
            is_readable: false,
            is_writable: true,
            is_appending: false,
        };

        let result = open_file.write(&[1; 1]);
        assert_eq!(result, Err(Errno::EPIPE));
    }
}
