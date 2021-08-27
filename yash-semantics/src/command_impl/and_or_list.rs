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
    async fn execute(&self, env: &mut Env) -> Result {
        self.first.execute(env).await
        // TODO rest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::return_builtin;
    use futures_executor::block_on;
    use yash_env::exec::ExitStatus;

    #[test]
    fn single_pipeline_list() {
        let mut env = Env::new_virtual();
        env.builtins.insert("return", return_builtin());
        let list: AndOrList = "return -n 36".parse().unwrap();
        let result = block_on(list.execute(&mut env));
        assert_eq!(result, Ok(()));
        assert_eq!(env.exit_status, ExitStatus(36));
    }
}
