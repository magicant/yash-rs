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

//! Reading input

use thiserror::Error;
use yash_env::Env;
use yash_env::system::Errno;
use yash_semantics::expansion::attr::AttrChar;
use yash_semantics::expansion::attr::Origin;
#[allow(deprecated)]
use yash_syntax::source::pretty::{AnnotationType, Message};
use yash_syntax::source::pretty::{Report, ReportType};
use yash_syntax::syntax::Fd;

/// Error reading from the standard input
///
/// This error is returned by [`read`] when an error occurs while reading from
/// the standard input.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("error reading from the standard input: {errno}")]
pub struct Error {
    #[from]
    pub errno: Errno,
}

impl Error {
    /// Converts this error to a report.
    #[must_use]
    pub fn to_report(&self) -> Report<'_> {
        let mut report = Report::new();
        report.r#type = ReportType::Error;
        report.title = self.to_string().into();
        report
    }

    /// Converts this error to a message.
    #[allow(deprecated)]
    #[deprecated(note = "use `to_report` instead", since = "0.11.0")]
    #[must_use]
    pub fn to_message(&self) -> Message<'_> {
        Message {
            r#type: AnnotationType::Error,
            title: self.to_string().into(),
            annotations: vec![],
            footers: vec![],
        }
    }
}

impl<'a> From<&'a Error> for Report<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_report()
    }
}

#[allow(deprecated)]
impl<'a> From<&'a Error> for Message<'a> {
    #[inline]
    fn from(error: &'a Error) -> Self {
        error.to_message()
    }
}

fn quoted(value: char) -> AttrChar {
    AttrChar {
        value,
        origin: Origin::SoftExpansion,
        is_quoted: true,
        is_quoting: false,
    }
}

fn quoting(value: char) -> AttrChar {
    AttrChar {
        value,
        origin: Origin::SoftExpansion,
        is_quoted: false,
        is_quoting: true,
    }
}

fn plain(value: char) -> AttrChar {
    AttrChar {
        value,
        origin: Origin::SoftExpansion,
        is_quoted: false,
        is_quoting: false,
    }
}

/// Reads a line from the standard input.
///
/// This function reads a line from the standard input and returns a vector of
/// [`AttrChar`]s representing the line. The line is terminated by the specified
/// `delimiter` byte, which is not included in the returned vector.
///
/// If `is_raw` is `true`, the read line is not subject to backslash processing.
/// Otherwise, backslash-newline pairs are treated as line continuations, and
/// other backslashes are treated as quoting characters. On encountering a line
/// continuation, this function removes the backslash-newline pair and continues
/// reading the next line. When reading the second and subsequent lines, this
/// function displays the value of the `PS2` variable as a prompt if the shell
/// is interactive and the input is from a terminal. This requires the optional
/// `yash-prompt` feature.
///
/// If successful, this function returns a vector of [`AttrChar`]s representing
/// the line read and a boolean value indicating whether the line was terminated
/// by a delimiter. If the end of the input is reached before finding a
/// delimiter, the boolean value is `false`.
pub async fn read(
    env: &mut Env,
    delimiter: u8,
    is_raw: bool,
) -> Result<(Vec<AttrChar>, bool), Error> {
    let mut result = Vec::new();

    let newline_found = loop {
        // TODO Read in bulk if the standard input is seekable
        match read_char(env).await? {
            None => break false,
            Some(c) if c == delimiter.into() => break true,

            // Backslash escape
            Some('\\') if !is_raw => {
                let c = read_char(env).await?;
                if c == Some('\n') {
                    // Line continuation
                    print_prompt(env).await;
                    continue;
                }
                result.push(quoting('\\'));
                match c {
                    None => break false,
                    Some(c) => result.push(quoted(c)),
                }
            }

            // Plain character
            Some(c) => result.push(plain(c)),
        }
    };

    Ok((result, newline_found))
}

/// Reads one character from the standard input.
///
/// This function reads a single UTF-8-encoded character from the standard
/// input. If the standard input is empty, this function returns `Ok(None)`.
/// If the input is not a valid UTF-8 sequence, this function returns an error.
async fn read_char(env: &mut Env) -> Result<Option<char>, Error> {
    // Any character is at most 4 bytes in UTF-8.
    let mut buffer = [0; 4];
    let mut len = 0;
    loop {
        // Read from the standard input byte by byte so that we don't consume
        // more than one character.
        let byte = std::slice::from_mut(&mut buffer[len]);
        let count = env.system.read_async(Fd::STDIN, byte).await?;
        if count == 0 {
            // End of input
            return if len == 0 {
                Ok(None)
            } else {
                // The input ended in the middle of a UTF-8 sequence.
                Err(Errno::EILSEQ.into())
            };
        }
        debug_assert_eq!(count, 1);
        len += 1;

        match std::str::from_utf8(&buffer[..len]) {
            Ok(s) => {
                let mut chars = s.chars();
                // Since the buffer is not empty, there must be a character.
                let c = chars.next().unwrap();
                // And it must be the only character.
                debug_assert_eq!(chars.next(), None);
                return Ok(Some(c));
            }
            Err(e) => match e.error_len() {
                None => {
                    // The bytes in the buffer are incomplete for a UTF-8
                    // character. Read more bytes.
                    continue;
                }
                Some(_) => return Err(Errno::EILSEQ.into()),
            },
        }
    }
}

/// Prints the prompt string for the continuation line.
///
/// This function prints the value of the `PS2` variable as a prompt for the
/// continuation line. If the shell is not interactive or the standard input
/// is not a terminal, this function does nothing.
async fn print_prompt(env: &mut Env) {
    #[cfg(feature = "yash-prompt")]
    {
        use yash_env::System as _;
        if !env.is_interactive() || !env.system.isatty(Fd::STDIN) {
            return;
        }

        // Obtain the prompt string
        let mut context = yash_env::input::Context::default();
        context.set_is_first_line(false);
        let prompt = yash_prompt::fetch_posix(&env.variables, &context);
        let prompt = yash_prompt::expand_posix(env, &prompt, false).await;
        env.system.print_error(&prompt).await;
    }

    #[cfg(not(feature = "yash-prompt"))]
    {
        _ = env;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::SystemState;
    use yash_env_test_helper::in_virtual_system;

    fn set_stdin<B: Into<Vec<u8>>>(system: &RefCell<SystemState>, bytes: B) {
        let state = system.borrow_mut();
        let stdin = state.file_system.get("/dev/stdin").unwrap();
        stdin.borrow_mut().body = FileBody::new(bytes);
    }

    fn attr_chars(s: &str) -> Vec<AttrChar> {
        s.chars().map(plain).collect()
    }

    #[test]
    fn empty_input() {
        in_virtual_system(|mut env, _| async move {
            let result = read(&mut env, b'\n', false).await;
            assert_eq!(result, Ok((vec![], false)));
        })
    }

    #[test]
    fn non_empty_input() {
        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, "foo\nbar\n");

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(result, Ok((attr_chars("foo"), true)));

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(result, Ok((attr_chars("bar"), true)));

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(result, Ok((vec![], false)));
        })
    }

    #[test]
    fn input_without_newline() {
        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, "newline");

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(result, Ok((attr_chars("newline"), false)));

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(result, Ok((vec![], false)));
        })
    }

    #[test]
    fn multibyte_characters() {
        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, "©⁉😀\n");

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(result, Ok((attr_chars("©⁉😀"), true)));
        })
    }

    #[test]
    fn nul_byte_delimiter() {
        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, "foo\0bar\0");

            let result = read(&mut env, b'\0', false).await;
            assert_eq!(result, Ok((attr_chars("foo"), true)));

            let result = read(&mut env, b'\0', false).await;
            assert_eq!(result, Ok((attr_chars("bar"), true)));

            let result = read(&mut env, b'\0', false).await;
            assert_eq!(result, Ok((vec![], false)));
        })
    }

    #[test]
    fn alphabetic_delimiter() {
        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, "foo\nbar\n");

            let result = read(&mut env, b'a', false).await;
            assert_eq!(result, Ok((attr_chars("foo\nb"), true)));

            let result = read(&mut env, b'a', false).await;
            assert_eq!(result, Ok((attr_chars("r\n"), false)));
        })
    }

    #[test]
    fn raw_mode() {
        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, "\\foo\\\nbar\\\nbaz\n");

            let result = read(&mut env, b'\n', true).await;
            assert_eq!(result, Ok((attr_chars("\\foo\\"), true)));
        })
    }

    #[test]
    fn no_raw_mode() {
        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, "\\foo\\\nbar\\\nbaz\n");

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(
                result,
                Ok((
                    vec![
                        quoting('\\'),
                        quoted('f'),
                        plain('o'),
                        plain('o'),
                        plain('b'),
                        plain('a'),
                        plain('r'),
                        plain('b'),
                        plain('a'),
                        plain('z'),
                    ],
                    true,
                )),
            );
        })
    }

    #[test]
    fn orphan_backslash() {
        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, "foo\\");

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(
                result,
                Ok((
                    vec![plain('f'), plain('o'), plain('o'), quoting('\\')],
                    false,
                )),
            );
        })
    }

    #[test]
    fn broken_utf8() {
        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, *b"\xFF");

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(result, Err(Errno::EILSEQ.into()));
        });

        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, *b"\xCF\xD0");

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(result, Err(Errno::EILSEQ.into()));
        });

        in_virtual_system(|mut env, system| async move {
            set_stdin(&system, *b"\xCF");

            let result = read(&mut env, b'\n', false).await;
            assert_eq!(result, Err(Errno::EILSEQ.into()));
        });
    }

    // TODO Test PS2 prompt
}
