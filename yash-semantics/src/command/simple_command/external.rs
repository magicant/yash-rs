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

//! Simple command semantics for external utilities

use super::perform_assignments;
use crate::Handle;
use crate::job::add_job_if_suspended;
use crate::redir::RedirGuard;
use crate::xtrace::XTrace;
use crate::xtrace::print;
use crate::xtrace::trace_fields;
use itertools::Itertools;
use std::ffi::CString;
use std::ops::ControlFlow::Continue;
use yash_env::Env;
use yash_env::System;
use yash_env::io::print_error;
use yash_env::semantics::ExitStatus;
use yash_env::semantics::Field;
use yash_env::semantics::Result;
use yash_env::subshell::JobControl;
use yash_env::subshell::Subshell;
use yash_env::system::Errno;
use yash_env::variable::Context;
use yash_syntax::source::Location;
use yash_syntax::syntax::Assign;
use yash_syntax::syntax::Redir;

pub async fn execute_external_utility(
    env: &mut Env,
    path: CString,
    assigns: &[Assign],
    fields: Vec<Field>,
    redirs: &[Redir],
) -> Result {
    let mut xtrace = XTrace::from_options(&env.options);

    let env = &mut RedirGuard::new(env);
    if let Err(e) = env.perform_redirs(redirs, xtrace.as_mut()).await {
        return e.handle(env).await;
    };

    let mut env = env.push_context(Context::Volatile);
    perform_assignments(&mut env, assigns, true, xtrace.as_mut()).await?;

    trace_fields(xtrace.as_mut(), &fields);
    print(&mut env, xtrace).await;

    if path.to_bytes().is_empty() {
        let name = &fields[0];
        print_error(
            &mut env,
            format!("cannot execute external utility {:?}", name.value).into(),
            "utility not found".into(),
            &name.origin,
        )
        .await;
        env.exit_status = ExitStatus::NOT_FOUND;
        return Continue(());
    }

    env.exit_status = start_external_utility_in_subshell_and_wait(&mut env, path, fields).await?;

    Continue(())
}

/// Starts an external utility in a subshell and waits for it to finish.
///
/// `path` is the path to the external utility. `fields` are the command line
/// words of the utility. The first field must exist and be the name of the
/// utility as it is used for error messages.
///
/// This function starts the utility in a subshell and waits for it to finish.
/// The subshell is a foreground job if job control is enabled.
///
/// This function returns the exit status of the utility. In case of an error,
/// it prints an error message to the standard error before returning an
/// appropriate exit status.
pub async fn start_external_utility_in_subshell_and_wait(
    env: &mut Env,
    path: CString,
    fields: Vec<Field>,
) -> Result<ExitStatus> {
    let name = fields[0].clone();
    let location = name.origin.clone();

    let job_name = if env.controls_jobs() {
        to_job_name(&fields)
    } else {
        String::new()
    };
    let args = to_c_strings(fields);
    let subshell = Subshell::new(move |env, _job_control| {
        Box::pin(replace_current_process(env, path, args, location))
    })
    .job_control(JobControl::Foreground);

    match subshell.start_and_wait(env).await {
        Ok((pid, result)) => add_job_if_suspended(env, pid, result, || job_name),
        Err(errno) => {
            print_error(
                env,
                format!("cannot execute external utility {:?}", name.value).into(),
                errno.to_string().into(),
                &name.origin,
            )
            .await;
            Continue(ExitStatus::NOEXEC)
        }
    }
}

fn to_job_name(fields: &[Field]) -> String {
    fields
        .iter()
        .format_with(" ", |field, f| f(&format_args!("{}", field.value)))
        .to_string()
}

/// Converts fields to C strings.
pub fn to_c_strings(s: Vec<Field>) -> Vec<CString> {
    s.into_iter()
        .filter_map(|f| {
            let bytes = f.value.into_bytes();
            // TODO Return NulError if the field contains a null byte
            CString::new(bytes).ok()
        })
        .collect()
}

/// Substitutes the currently executing shell process with the external utility.
///
/// This function performs the very last step of the simple command execution.
/// It disables the internal signal dispositions and calls the `execve` system
/// call. If the call fails, it prints an error message to the standard error
/// and updates `env.exit_status`, in which case the caller should immediately
/// exit the current process with the exit status.
///
/// If the `execve` call fails with `ENOEXEC`, this function falls back on
/// invoking the shell with the given arguments, so that the shell can interpret
/// the script. The path to the shell executable is taken from
/// [`System::shell_path`].
pub async fn replace_current_process(
    env: &mut Env,
    path: CString,
    args: Vec<CString>,
    location: Location,
) {
    env.traps
        .disable_internal_dispositions(&mut env.system)
        .ok();

    let envs = env.variables.env_c_strings();
    let result = env.system.execve(path.as_c_str(), &args, &envs);
    // TODO Prefer into_err to unwrap_err
    let errno = result.unwrap_err();
    match errno {
        Errno::ENOEXEC => {
            fall_back_on_sh(&mut env.system, path.clone(), args, envs);
            env.exit_status = ExitStatus::NOEXEC;
        }
        Errno::ENOENT | Errno::ENOTDIR => {
            env.exit_status = ExitStatus::NOT_FOUND;
        }
        _ => {
            env.exit_status = ExitStatus::NOEXEC;
        }
    }
    print_error(
        env,
        format!("cannot execute external utility {path:?}").into(),
        errno.to_string().into(),
        &location,
    )
    .await;
}

/// Invokes the shell with the given arguments.
fn fall_back_on_sh<S: System>(
    system: &mut S,
    mut script_path: CString,
    mut args: Vec<CString>,
    envs: Vec<CString>,
) {
    // Prevent the path to be regarded as an option
    if script_path.as_bytes().starts_with("-".as_bytes()) {
        let mut bytes = script_path.into_bytes();
        bytes.splice(0..0, "./".bytes());
        script_path = CString::new(bytes).unwrap();
    }

    args.insert(1, script_path);

    // Some shells change their behavior depending on args[0].
    // We set it to "sh" for the maximum portability.
    c"sh".clone_into(&mut args[0]);

    let sh_path = system.shell_path();
    let _ = system.execve(&sh_path, &args, &envs);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    use assert_matches::assert_matches;
    use futures_util::FutureExt;
    use std::cell::RefCell;
    use std::ops::ControlFlow::Continue;
    use std::rc::Rc;
    use std::str::from_utf8;
    use yash_env::option::State::On;
    use yash_env::system::Mode;
    use yash_env::system::r#virtual::FileBody;
    use yash_env::system::r#virtual::Inode;
    use yash_env::variable::Scope;
    use yash_env::variable::Value;
    use yash_env_test_helper::assert_stderr;
    use yash_env_test_helper::in_virtual_system;
    use yash_env_test_helper::stub_tty;
    use yash_syntax::syntax;

    #[test]
    fn simple_command_calls_execve_with_correct_arguments() {
        in_virtual_system(|mut env, state| async move {
            let mut content = Inode::default();
            content.body = FileBody::Regular {
                content: Vec::new(),
                is_native_executable: true,
            };
            content.permissions.set(Mode::USER_EXEC, true);
            let content = Rc::new(RefCell::new(content));
            state
                .borrow_mut()
                .file_system
                .save("/some/file", content)
                .unwrap();

            let mut var = env.variables.get_or_new("env", Scope::Global);
            var.assign("scalar", None).unwrap();
            var.export(true);
            let mut var = env.variables.get_or_new("local", Scope::Global);
            var.assign("ignored", None).unwrap();

            let command: syntax::SimpleCommand = "var=123 /some/file foo bar".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));

            let state = state.borrow();
            let process = state.processes.values().last().unwrap();
            let arguments = process.last_exec().as_ref().unwrap();
            assert_eq!(arguments.0, c"/some/file".to_owned());
            assert_eq!(
                arguments.1,
                [
                    c"/some/file".to_owned(),
                    c"foo".to_owned(),
                    c"bar".to_owned()
                ]
            );
            let mut envs = arguments.2.clone();
            envs.sort();
            assert_eq!(envs, [c"env=scalar".to_owned(), c"var=123".to_owned()]);
        });
    }

    #[test]
    fn simple_command_returns_exit_status_from_external_utility() {
        in_virtual_system(|mut env, state| async move {
            let mut content = Inode::default();
            content.body = FileBody::Regular {
                content: Vec::new(),
                is_native_executable: true,
            };
            content.permissions.set(Mode::USER_EXEC, true);
            let content = Rc::new(RefCell::new(content));
            state
                .borrow_mut()
                .file_system
                .save("/some/file", content)
                .unwrap();

            let command: syntax::SimpleCommand = "/some/file foo bar".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            // In VirtualSystem, execve fails with ENOSYS.
            assert_eq!(env.exit_status, ExitStatus::NOEXEC);
        });
    }

    // TODO Test fall_back_on_sh

    #[test]
    fn simple_command_skips_running_external_utility_on_redirection_error() {
        in_virtual_system(|mut env, state| async move {
            let mut content = Inode::default();
            content.body = FileBody::Regular {
                content: Vec::new(),
                is_native_executable: true,
            };
            content.permissions.set(Mode::USER_EXEC, true);
            let content = Rc::new(RefCell::new(content));
            state
                .borrow_mut()
                .file_system
                .save("/some/file", content)
                .unwrap();

            let command: syntax::SimpleCommand = "/some/file </no/such/file".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::ERROR);
        });
    }

    #[test]
    fn simple_command_returns_127_for_non_existing_file() {
        in_virtual_system(|mut env, _state| async move {
            let command: syntax::SimpleCommand = "/some/file".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::NOT_FOUND);
        });
    }

    #[test]
    fn simple_command_returns_126_on_exec_failure() {
        in_virtual_system(|mut env, state| async move {
            let mut content = Inode::default();
            content.permissions.set(Mode::USER_EXEC, true);
            let content = Rc::new(RefCell::new(content));
            state
                .borrow_mut()
                .file_system
                .save("/some/file", content)
                .unwrap();

            let command: syntax::SimpleCommand = "/some/file".parse().unwrap();
            let result = command.execute(&mut env).await;
            assert_eq!(result, Continue(()));
            assert_eq!(env.exit_status, ExitStatus::NOEXEC);
        });
    }

    #[test]
    fn simple_command_returns_126_on_fork_failure() {
        let mut env = Env::new_virtual();
        let command: syntax::SimpleCommand = "/some/file".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::NOEXEC);
    }

    #[test]
    fn exit_status_is_127_on_command_not_found() {
        let mut env = Env::new_virtual();
        let command: syntax::SimpleCommand = "no_such_command".parse().unwrap();
        let result = command.execute(&mut env).now_or_never().unwrap();
        assert_eq!(result, Continue(()));
        assert_eq!(env.exit_status, ExitStatus::NOT_FOUND);
    }

    #[test]
    fn simple_command_assigns_variables_in_volatile_context_for_external_utility() {
        in_virtual_system(|mut env, _state| async move {
            let command: syntax::SimpleCommand = "a=123 /foo/bar".parse().unwrap();
            command.execute(&mut env).await;
            assert_eq!(env.variables.get("a"), None);
        });
    }

    #[test]
    fn simple_command_performs_redirections_and_assignments_for_target_not_found() {
        in_virtual_system(|mut env, state| async move {
            let command: syntax::SimpleCommand =
                "foo=${bar=baz} no_such_utility >/tmp/file".parse().unwrap();
            command.execute(&mut env).await;
            assert_eq!(env.variables.get("foo"), None);
            assert_eq!(
                env.variables.get("bar").unwrap().value,
                Some(Value::scalar("baz"))
            );

            let stdout = state.borrow().file_system.get("/tmp/file").unwrap();
            let stdout = stdout.borrow();
            assert_matches!(&stdout.body, FileBody::Regular { content, .. } => {
                assert_eq!(from_utf8(content), Ok(""));
            });
        });
    }

    #[test]
    fn job_control_for_external_utility() {
        in_virtual_system(|mut env, state| async move {
            env.options.set(yash_env::option::Monitor, On);
            stub_tty(&state);

            let mut content = Inode::default();
            content.body = FileBody::Regular {
                content: Vec::new(),
                is_native_executable: true,
            };
            content.permissions.set(Mode::USER_EXEC, true);
            let content = Rc::new(RefCell::new(content));
            state
                .borrow_mut()
                .file_system
                .save("/some/file", content)
                .unwrap();

            let command: syntax::SimpleCommand = "/some/file".parse().unwrap();
            let _ = command.execute(&mut env).await;

            let state = state.borrow();
            let (&pid, process) = state.processes.last_key_value().unwrap();
            assert_ne!(pid, env.main_pid);
            assert_ne!(process.pgid(), env.main_pgid);
        })
    }

    #[test]
    fn xtrace_for_external_utility() {
        in_virtual_system(|mut env, state| async move {
            env.options.set(yash_env::option::XTrace, On);

            let mut content = Inode::default();
            content.body = FileBody::Regular {
                content: Vec::new(),
                is_native_executable: true,
            };
            content.permissions.set(Mode::USER_EXEC, true);
            let content = Rc::new(RefCell::new(content));
            state
                .borrow_mut()
                .file_system
                .save("/some/file", content)
                .unwrap();

            let command: syntax::SimpleCommand =
                "VAR=123 /some/file foo bar >/dev/null".parse().unwrap();
            let _ = command.execute(&mut env).await;

            assert_stderr(&state, |stderr| {
                assert!(
                    stderr.starts_with("VAR=123 /some/file foo bar 1>/dev/null\n"),
                    "stderr = {stderr:?}"
                )
            });
        });
    }
}
