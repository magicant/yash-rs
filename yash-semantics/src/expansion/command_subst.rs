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

//! Command substitution semantics.

use super::Env;
use super::Error;
use super::ErrorCause;
use super::Expand;
use super::Expansion;
use super::Origin;
use super::Output;
use super::Result;
use crate::read_eval_loop;
use crate::Handle;
use async_trait::async_trait;
use std::ops::ControlFlow::Continue;
use yash_env::exec::ExitStatus;
use yash_env::io::Fd;
use yash_env::system::Errno;
use yash_env::System;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Location;
use yash_syntax::source::Source;

/// Reference to a `CommandSubst` or `Backquote`.
pub struct CommandSubstRef<'a> {
    content: &'a str,
    location: &'a Location,
}

impl<'a> CommandSubstRef<'a> {
    pub fn new(content: &'a str, location: &'a Location) -> Self {
        CommandSubstRef { content, location }
    }
}

#[async_trait(?Send)]
impl Expand for CommandSubstRef<'_> {
    async fn expand<E: Env>(&self, env: &mut E, output: &mut Output<'_>) -> Result {
        // TODO return exit_status
        let (result, _exit_status) =
            expand_command_substitution(env, self.content, self.location).await?;
        output.push_str(&result, Origin::SoftExpansion, false, false);
        Ok(())
    }
}

/// Expands a command substitution to a string.
///
/// This function evaluates the code in a subshell whose standard output is
/// captured by the current shell. After capturing all output and waiting for
/// the subshell to finish, this function returns the captured string and exit
/// status. Trailing newlines are removed before the result is returned.
pub async fn expand_command_substitution<E: Env>(
    env: &mut E,
    code: &str,
    location: &Location,
) -> Result<(String, ExitStatus)> {
    expand_command_substitution_inner(env, code.to_owned(), location)
        .await
        .map_err(|errno| Error {
            cause: ErrorCause::CommandSubstError(errno),
            location: location.clone(),
        })
}

async fn expand_command_substitution_inner<E: Env>(
    env: &mut E,
    code: String,
    location: &Location,
) -> std::result::Result<(String, ExitStatus), Errno> {
    let original = location.clone();
    let (reader, writer) = env.pipe()?;

    // Start a subshell to run the command
    let subshell_result = env
        .start_subshell(move |env| {
            Box::pin(async move {
                env.system.close(reader).ok();
                if writer != Fd::STDOUT {
                    if let Err(errno) = env.system.dup2(writer, Fd::STDOUT) {
                        let error = Error {
                            cause: ErrorCause::CommandSubstError(errno),
                            location: original,
                        };
                        return error.handle(env).await;
                    }
                    env.system.close(writer).ok();
                }

                let mut lexer = Lexer::from_memory(&code, Source::CommandSubst { original });
                read_eval_loop(env, &mut lexer).await;
                Continue(())
            })
        })
        .await;
    let pid = match subshell_result {
        Ok(pid) => pid,
        Err(errno) => {
            env.close(reader).ok();
            env.close(writer).ok();
            return Err(errno);
        }
    };

    env.close(writer).ok();

    // Read the output from the subshell
    let mut result = Vec::new();
    let mut buffer = [0; 1024];
    while let Ok(count) = env.read_async(reader, &mut buffer).await {
        if count == 0 {
            break;
        }
        result.extend(&buffer[..count]);
    }
    env.close(reader).ok();

    // Wait for the subshell
    // TODO What if the child process suspends?
    use yash_env::job::WaitStatus::*;
    let exit_status = match env.wait_for_subshell(pid).await? {
        Exited(_pid, exit_status) => ExitStatus(exit_status),
        Signaled(_pid, signal, _core_dumped) => ExitStatus::from(signal),
        status => todo!("unhandled wait status {:?}", status),
    };

    let mut result = String::from_utf8(result)
        .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).to_string());

    // Remove trailing newlines
    let len = result.trim_end_matches('\n').len();
    result.truncate(len);

    Ok((result, exit_status))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::in_virtual_system;
    use crate::tests::return_builtin;
    use futures_executor::block_on;
    use yash_syntax::source::Location;

    #[test]
    fn empty_substitution() {
        in_virtual_system(|mut env, _pid, _state| async move {
            let result = expand_command_substitution(&mut env, "", &Location::dummy("")).await;
            assert_eq!(result, Ok(("".to_string(), ExitStatus::SUCCESS)));
        })
    }

    #[test]
    fn one_line_substitution() {
        in_virtual_system(|mut env, _pid, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            let result =
                expand_command_substitution(&mut env, "echo ok", &Location::dummy("")).await;
            assert_eq!(result, Ok(("ok".to_string(), ExitStatus::SUCCESS)));
        })
    }

    #[test]
    fn many_line_substitution() {
        in_virtual_system(|mut env, _pid, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            let result = expand_command_substitution(
                &mut env,
                "echo 1; echo 2; echo; echo 3; echo; echo",
                &Location::dummy(""),
            )
            .await;
            assert_eq!(result, Ok(("1\n2\n\n3".to_string(), ExitStatus::SUCCESS)));
        })
    }

    #[test]
    fn exit_status_of_substitution() {
        in_virtual_system(|mut env, _pid, _state| async move {
            env.builtins.insert("return", return_builtin());
            let result =
                expand_command_substitution(&mut env, "return -n 100", &Location::dummy("")).await;
            assert_eq!(result, Ok(("".to_string(), ExitStatus(100))));
        })
    }

    #[test]
    fn error_in_substitution() {
        let mut env = yash_env::Env::new_virtual();
        let location = Location::dummy("foo");
        let result = block_on(expand_command_substitution(&mut env, "", &location));
        let error = result.unwrap_err();
        assert_eq!(error.cause, ErrorCause::CommandSubstError(Errno::ENOSYS));
        assert_eq!(error.location, location);
    }
}
