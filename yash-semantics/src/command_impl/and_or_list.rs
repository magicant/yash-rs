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

//! Implementation of the and-or list semantics.

use super::Command;
use async_trait::async_trait;
use yash_env::exec::Result;
use yash_env::Env;
use yash_syntax::syntax::AndOrList;

#[async_trait(?Send)]
impl Command for AndOrList {
    /// Executes the and-or list.
    ///
    /// TODO Elaborate
    async fn execute(&self, env: &mut Env) -> Result {
        use yash_syntax::syntax::AndOr::*;
        self.first.execute(env).await?;
        for (and_or, pipeline) in &self.rest {
            let success = env.exit_status.is_successful();
            let run = match and_or {
                AndThen => success,
                OrElse => !success,
            };
            if run {
                pipeline.execute(env).await;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::echo_builtin;
    use crate::tests::return_builtin;
    use futures_executor::block_on;
    use std::rc::Rc;
    use yash_env::exec::Divert;
    use yash_env::exec::ExitStatus;
    use yash_env::VirtualSystem;

    #[test]
    fn single_pipeline_list() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "return -n 36".parse().unwrap();
        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(36));
    }

    #[test]
    fn true_and_true() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let list: AndOrList = "echo one && echo two".parse().unwrap();

        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(stdout.content, "one\ntwo\n".as_bytes());
    }

    #[test]
    fn true_and_false() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "return -n 0 && return -n 5".parse().unwrap();
        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(5));
    }

    #[test]
    fn false_and_true() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "return -n 1 && echo !".parse().unwrap();

        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(1));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(stdout.content, []);
    }

    #[test]
    fn true_and_true_and_true() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        let list: AndOrList = "echo 1 && echo 2 && echo 3".parse().unwrap();

        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(stdout.content, "1\n2\n3\n".as_bytes());
    }

    #[test]
    fn true_and_false_and_true() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "return -n 0 && return -n 2 && echo !".parse().unwrap();

        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(2));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(stdout.content, []);
    }

    #[test]
    fn false_and_any_or_true() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "return -n 8 && X || return -n 0".parse().unwrap();
        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(0));
    }

    #[test]
    fn true_or_false() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "echo + || return -n 100".parse().unwrap();

        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(stdout.content, "+\n".as_bytes());
    }

    #[test]
    fn false_or_true() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "{ echo one; return -n 1; } || { echo two; }"
            .parse()
            .unwrap();

        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(stdout.content, "one\ntwo\n".as_bytes());
    }

    #[test]
    fn false_or_false() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "{ echo one; return -n 1; } || { echo two; return -n 2; }"
            .parse()
            .unwrap();

        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(2));

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(stdout.content, "one\ntwo\n".as_bytes());
    }

    #[test]
    fn false_or_false_or_false() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "return -n 1 || return -n 2 || return -n 3".parse().unwrap();

        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(3));
    }

    #[test]
    fn false_or_true_or_false() {
        let system = VirtualSystem::new();
        let state = Rc::clone(&system.state);
        let mut env = Env::with_system(Box::new(system));
        env.builtins.insert("echo", echo_builtin());
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "return -n 3 || echo + || return -n 4".parse().unwrap();

        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus::SUCCESS);

        let state = state.borrow();
        let stdout = state.file_system.get("/dev/stdout").unwrap().borrow();
        assert_eq!(stdout.content, "+\n".as_bytes());
    }

    #[test]
    fn true_or_any_and_false() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "return -n 0 || X && return -n 9".parse().unwrap();
        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(9));
    }

    #[test]
    fn diverting_first() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "return 97".parse().unwrap();
        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Err(Divert::Return));
        assert_eq!(env.exit_status, ExitStatus(97));
    }

    // TODO What if the right-hand-side diverts?
}
