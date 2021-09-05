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

//! Implementations of [`Handle`].

use crate::ExitStatus;
use crate::Handle;
use async_trait::async_trait;
use yash_env::Env;

#[async_trait(?Send)]
impl Handle<crate::expansion::Error> for Env {
    /// Prints an error message and sets the exit status to non-zero.
    ///
    /// This function handles an expansion error by printing an error message
    /// that describes the error to the standard error and setting the exit
    /// status to [`ExitStatus::ERROR`]. Note that other POSIX-compliant
    /// implementations may use different non-zero exit statuses.
    async fn handle(&mut self, error: crate::expansion::Error) -> super::Result {
        use crate::expansion::ErrorCause::*;
        // TODO Localize the message
        // TODO Pretty-print the error location
        match error.cause {
            Dummy(message) => {
                self.print_error(&format_args!("dummy error: {}", message))
                    .await
            }
        };
        self.exit_status = ExitStatus::ERROR;
        Ok(())
    }
}

#[async_trait(?Send)]
impl Handle<crate::assign::Error> for Env {
    /// Prints an error message and sets the exit status to non-zero.
    ///
    /// This function handles an assignment error by printing an error message
    /// that describes the error to the standard error and setting the exit
    /// status to [`ExitStatus::ERROR`]. Note that other POSIX-compliant
    /// implementations may use different non-zero exit statuses.
    async fn handle(&mut self, error: crate::assign::Error) -> super::Result {
        use crate::assign::ErrorCause::*;
        // TODO Localize the message
        // TODO Pretty-print the error location
        match error.cause {
            // TODO Print read-only location
            ReadOnly { name, .. } => {
                self.print_error(&format_args!(
                    "cannot assign to read-only variable {}",
                    name
                ))
                .await
            }
            Expansion(cause) => {
                let location = error.location;
                let error = crate::expansion::Error { cause, location };
                return self.handle(error).await;
            }
        }
        self.exit_status = ExitStatus::ERROR;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_executor::block_on;
    use std::rc::Rc;
    use yash_env::VirtualSystem;
    use yash_syntax::source::Location;

    #[test]
    fn handle_assign_error_read_only() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        let cause = crate::assign::ErrorCause::ReadOnly {
            name: "foo".to_string(),
            read_only_location: Location::dummy(""),
        };
        let location = Location::dummy("location");
        let error = crate::assign::Error { cause, location };
        block_on(env.handle(error)).unwrap();
        assert_eq!(env.exit_status, ExitStatus::ERROR);

        let state = state.borrow();
        let stderr = state.file_system.get("/dev/stderr").unwrap().borrow();
        let stderr = std::str::from_utf8(&stderr.content).unwrap();
        assert!(
            stderr.contains("foo"),
            "The error message should contain the variable name: {:?}",
            stderr
        );
    }
}
