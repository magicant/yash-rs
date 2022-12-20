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

//! Printing expansion results

use std::fmt::Write;
use yash_env::option::OptionSet;
use yash_env::option::State;
use yash_env::Env;

/// Temporary buffer that accumulates expanded strings
///
/// An `XTrace` object is a string buffer that keeps words to be printed as a
/// trace. We print all the assignments and command line words expanded in a
/// single simple command in a single line of trace, so we use this object
/// to accumulate expansions until everything is ready.
///
/// To add words to the buffer, call methods of [`Write`] on the `XTrace`.
/// To print the collected words, call [`flush`](Self::flush).
#[derive(Clone, Debug, Default)]
pub struct XTrace {
    /// Accumulated trace
    buffer: String,
}

impl XTrace {
    /// Creates a new trace buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new trace buffer if the `xtrace` option is on.
    #[must_use]
    pub fn from_options(options: &OptionSet) -> Option<Self> {
        match options.get(yash_env::option::Option::XTrace) {
            State::On => Some(Self::new()),
            State::Off => None,
        }
    }

    /// Clears the buffer.
    pub fn clear(&mut self) {
        self.buffer.clear()
    }

    /// Prints and clears the buffer content.
    ///
    /// If the buffer is not empty, it is printed to the standard error,
    /// preceded by an expansion of the `$PS4` variable and followed by a
    /// newline.
    ///
    /// This function trims trailing spaces.
    ///
    /// This function ignores any error that may occur while printing, so there
    /// is no return value.
    pub async fn flush(&mut self, env: &mut Env) {
        self.buffer
            .truncate(self.buffer.trim_end_matches(' ').len());
        if self.buffer.is_empty() {
            return;
        }
        self.buffer.push('\n');
        env.print_error(&self.buffer).await;
        self.clear();
        // TODO Prefix $PS4
        // TODO Expand $PS4
        // TODO Prevent recursive tracing
    }
}

/// Convenience function for flushing an optional trace.
pub async fn flush(env: &mut Env, xtrace: Option<XTrace>) {
    if let Some(mut xtrace) = xtrace {
        xtrace.flush(env).await
    }
}

impl Write for XTrace {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.buffer.write_str(s)
    }
    fn write_char(&mut self, c: char) -> std::fmt::Result {
        self.buffer.write_char(c)
    }
    fn write_fmt(&mut self, args: std::fmt::Arguments<'_>) -> std::fmt::Result {
        self.buffer.write_fmt(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::assert_stderr;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::rc::Rc;
    use yash_env::system::r#virtual::SystemState;
    use yash_env::VirtualSystem;

    fn fixture() -> (XTrace, Env, Rc<RefCell<SystemState>>) {
        let xtrace = XTrace::new();
        let system = Box::new(VirtualSystem::new());
        let state = Rc::clone(&system.state);
        let env = Env::with_system(system);
        (xtrace, env, state)
    }

    #[test]
    fn empty_flush() {
        // TODO Check if $PS4 is ignored
        let (mut xtrace, mut env, state) = fixture();
        xtrace.flush(&mut env).now_or_never().unwrap();
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));

        // Trailing spaces are ignored, so it's still empty
        xtrace.write_str("   ").unwrap();
        xtrace.flush(&mut env).now_or_never().unwrap();
        assert_stderr(&state, |stderr| assert_eq!(stderr, ""));
    }

    #[test]
    fn non_empty_flush() {
        let (mut xtrace, mut env, state) = fixture();
        xtrace.write_str("foo").unwrap();
        xtrace.flush(&mut env).now_or_never().unwrap();
        assert_stderr(&state, |stderr| assert_eq!(stderr, "foo\n"));
    }

    #[test]
    fn flush_clears_buffer() {
        let (mut xtrace, mut env, state) = fixture();
        xtrace.write_str("foo").unwrap();
        xtrace.flush(&mut env).now_or_never().unwrap();
        // The first `flush` clears the buffer, so the second is a no-op.
        // Compare `non_empty_flush`
        xtrace.flush(&mut env).now_or_never().unwrap();
        assert_stderr(&state, |stderr| assert_eq!(stderr, "foo\n"));
    }

    #[test]
    fn trimming_trailing_spaces() {
        let (mut xtrace, mut env, state) = fixture();
        xtrace.write_str("foo   ").unwrap();
        xtrace.flush(&mut env).now_or_never().unwrap();
        assert_stderr(&state, |stderr| assert_eq!(stderr, "foo\n"));
    }
}
