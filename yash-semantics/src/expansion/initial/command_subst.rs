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

//! Expansion of command substitution

use super::super::attr::AttrChar;
use super::super::attr::Origin;
use super::super::phrase::Phrase;
use super::Env;
use super::Error;
use crate::Handle as _;
use crate::Runtime;
use crate::expansion::ErrorCause;
use crate::read_eval_loop;
use crate::trap::run_exit_trap;
use std::cell::RefCell;
use yash_env::io::Fd;
use yash_env::job::Pid;
use yash_env::job::ProcessResult;
use yash_env::semantics::ExitStatus;
use yash_env::subshell::Config;
use yash_env::subshell::JobControl;
use yash_env::system::concurrency::ReadAll;
use yash_env::system::concurrency::WaitForSignals;
use yash_env::system::{Close, Errno, Wait};
use yash_env::trap::SignalSystem;
use yash_syntax::parser::lex::Lexer;
use yash_syntax::source::Location;
use yash_syntax::source::Source;

/// Performs command substitution
pub async fn expand<C, S>(
    command: C,
    location: Location,
    env: &mut Env<'_, S>,
) -> Result<Phrase, Error>
where
    C: AsRef<str> + 'static,
    S: Runtime + 'static,
{
    let original = location.clone();

    // Open a pipe to read the output from the command
    let (reader, writer) = match env.inner.system.pipe() {
        Ok(pipes) => pipes,
        Err(errno) => {
            return Err(Error {
                cause: ErrorCause::CommandSubstError(errno),
                location,
            });
        }
    };

    // Start a subshell to run the command
    let subshell_result = Config::new()
        .start(env.inner, async move |env, _job_control| {
            let result = subshell_body(env, reader, writer, original, command).await;
            env.apply_result(result);
            run_exit_trap(env).await;
        })
        .await;

    expand_common(reader, writer, subshell_result, location, env).await
}

async fn subshell_body<C, S>(
    env: &mut yash_env::Env<S>,
    reader: Fd,
    writer: Fd,
    original: Location,
    command: C,
) -> yash_env::semantics::Result
where
    C: AsRef<str>,
    S: Runtime + 'static,
{
    // Arrange the file descriptors
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

    // Run the command
    let mut lexer = Lexer::from_memory(command.as_ref(), Source::CommandSubst { original });
    read_eval_loop(&RefCell::new(env), &mut lexer).await
}

/// The second half of [`expand`] that does not depend on type parameter `C`.
async fn expand_common<S>(
    reader: Fd,
    writer: Fd,
    subshell_result: Result<(Pid, Option<JobControl>), Errno>,
    location: Location,
    env: &mut Env<'_, S>,
) -> Result<Phrase, Error>
where
    S: Close + ReadAll + SignalSystem + Wait + WaitForSignals,
{
    // See if the subshell has successfully started
    let pid = match subshell_result {
        Ok((pid, job_control)) => {
            debug_assert_eq!(job_control, None);
            pid
        }
        Err(errno) => {
            env.inner.system.close(reader).ok();
            env.inner.system.close(writer).ok();
            return Err(Error {
                cause: ErrorCause::CommandSubstError(errno),
                location,
            });
        }
    };

    env.inner.system.close(writer).ok();

    // Read the output from the subshell
    // (In case of an error, we will use whatever we have read so far and ignore the error,
    // just like bash does.)
    let mut result = Vec::new();
    env.inner.system.read_all_to(reader, &mut result).await.ok();
    env.inner.system.close(reader).ok();

    // Wait for the subshell to terminate (ignoring intermediate stopped states)
    let process_result = loop {
        match env.inner.wait_for_subshell_to_halt(pid).await {
            Ok((_pid, result)) if result.is_stopped() => continue,
            Ok((_pid, result)) => break result,
            Err(errno) => {
                return Err(Error {
                    cause: ErrorCause::CommandSubstError(errno),
                    location,
                });
            }
        }
    };

    let exit_status = ExitStatus::from(process_result);
    env.last_command_subst_exit_status = Some(exit_status);

    // If the subshell was killed by SIGINT in an interactive shell with the
    // default SIGINT disposition, propagate the interrupt.
    if let ProcessResult::Signaled { signal, .. } = process_result
        && signal == S::SIGINT
        && env.inner.is_interactive()
        && env.inner.sigint_has_default_action()
    {
        return Err(Error {
            cause: ErrorCause::Interrupted(exit_status),
            location,
        });
    }

    // TODO Reject invalid UTF-8 sequence if strict POSIX mode is on
    let mut result = String::from_utf8(result)
        .unwrap_or_else(|e| String::from_utf8_lossy(&e.into_bytes()).into());

    // Remove trailing newlines
    let len = result.trim_end_matches('\n').len();
    result.truncate(len);

    let chars = result
        .chars()
        .map(|value| AttrChar {
            value,
            origin: Origin::SoftExpansion,
            is_quoted: false,
            is_quoting: false,
        })
        .collect();
    Ok(Phrase::Field(chars))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures_util::FutureExt as _;
    use std::pin::Pin;
    use yash_env::builtin::Builtin;
    use yash_env::option::Option::Interactive;
    use yash_env::option::State::On;
    use yash_env::semantics::ExitStatus;
    use yash_env::semantics::Field;
    use yash_env::system::r#virtual::SIGINT;
    use yash_env::system::{GetPid, SendSignal};
    use yash_env::test_helper::in_virtual_system;
    use yash_env::trap::Action;

    #[test]
    fn empty_substitution() {
        in_virtual_system(|mut env, _state| async move {
            let command = "".to_string();
            let location = Location::dummy("");
            let mut env = Env::new(&mut env);
            let result = expand(command, location, &mut env).await;
            assert_eq!(result, Ok(Phrase::one_empty_field()));
        })
    }

    #[test]
    fn one_line_substitution() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            let command = "echo ok".to_string();
            let location = Location::dummy("");
            let mut env = Env::new(&mut env);
            let result = expand(command, location, &mut env).await;

            let o = AttrChar {
                value: 'o',
                origin: Origin::SoftExpansion,
                is_quoted: false,
                is_quoting: false,
            };
            let k = AttrChar { value: 'k', ..o };
            assert_eq!(result, Ok(Phrase::Field(vec![o, k])));
        })
    }

    #[test]
    fn many_line_substitution() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("echo", echo_builtin());
            let command = "echo 1; echo 2; echo; echo 3; echo; echo".to_string();
            let location = Location::dummy("");
            let mut env = Env::new(&mut env);
            let result = expand(command, location, &mut env).await;
            let chars = "1\n2\n\n3"
                .chars()
                .map(|value| AttrChar {
                    value,
                    origin: Origin::SoftExpansion,
                    is_quoted: false,
                    is_quoting: false,
                })
                .collect();
            assert_eq!(result, Ok(Phrase::Field(chars)));
        })
    }

    #[test]
    fn exit_status_of_command_substitution() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert("return", return_builtin());
            let command = "return -n 100".to_string();
            let location = Location::dummy("");
            let mut env = Env::new(&mut env);
            let result = expand(command, location, &mut env).await;
            assert_eq!(result, Ok(Phrase::one_empty_field()));
            assert_eq!(env.last_command_subst_exit_status, Some(ExitStatus(100)));
        })
    }

    #[test]
    fn error_in_command_substitution() {
        let command = "".to_string();
        let location = Location::dummy("foo");
        let mut env = yash_env::Env::new_virtual();
        let mut env = Env::new(&mut env);
        let result = expand(command, location.clone(), &mut env)
            .now_or_never()
            .unwrap();
        let cause = ErrorCause::CommandSubstError(Errno::ENOSYS);
        assert_eq!(result, Err(Error { cause, location }));
    }

    fn kill_self_with_sigint_main<S: GetPid + SendSignal>(
        env: &mut yash_env::Env<S>,
        _args: Vec<Field>,
    ) -> Pin<Box<dyn Future<Output = yash_env::builtin::Result> + '_>> {
        Box::pin(async move {
            let pid = env.system.getpid();
            env.system.kill(pid, Some(SIGINT)).await.ok();
            yash_env::builtin::Result::default()
        })
    }

    #[test]
    fn interrupt_interactive_shell_when_command_subst_killed_by_sigint() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert(
                "kill_self",
                Builtin::new(
                    yash_env::builtin::Type::Mandatory,
                    kill_self_with_sigint_main,
                ),
            );
            env.options.set(Interactive, On);
            // SIGINT trap action is Action::Default by default

            let command = "kill_self".to_string();
            let location = Location::dummy("loc");
            let mut expansion_env = Env::new(&mut env);
            let result = expand(command, location.clone(), &mut expansion_env).await;

            assert_eq!(
                result,
                Err(Error {
                    cause: ErrorCause::Interrupted(ExitStatus::from(SIGINT)),
                    location,
                })
            );
        })
    }

    #[test]
    fn no_interrupt_for_non_interactive_shell_when_command_subst_killed_by_sigint() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert(
                "kill_self",
                Builtin::new(
                    yash_env::builtin::Type::Mandatory,
                    kill_self_with_sigint_main,
                ),
            );
            // Not setting Interactive option

            let command = "kill_self".to_string();
            let location = Location::dummy("loc");
            let mut expansion_env = Env::new(&mut env);
            let result = expand(command, location, &mut expansion_env).await;

            assert_eq!(result, Ok(Phrase::one_empty_field()));
            assert_eq!(
                expansion_env.last_command_subst_exit_status,
                Some(ExitStatus::from(SIGINT))
            );
        })
    }

    #[test]
    fn no_interrupt_when_sigint_trapped_and_command_subst_killed_by_sigint() {
        in_virtual_system(|mut env, _state| async move {
            env.builtins.insert(
                "kill_self",
                Builtin::new(
                    yash_env::builtin::Type::Mandatory,
                    kill_self_with_sigint_main,
                ),
            );
            env.options.set(Interactive, On);
            env.traps
                .set_action(
                    &env.system,
                    SIGINT,
                    Action::Command("echo trapped".into()),
                    Location::dummy(""),
                    false,
                )
                .await
                .unwrap();

            let command = "kill_self".to_string();
            let location = Location::dummy("loc");
            let mut expansion_env = Env::new(&mut env);
            let result = expand(command, location, &mut expansion_env).await;

            assert_eq!(result, Ok(Phrase::one_empty_field()));
            assert_eq!(
                expansion_env.last_command_subst_exit_status,
                Some(ExitStatus::from(SIGINT))
            );
        })
    }
}
