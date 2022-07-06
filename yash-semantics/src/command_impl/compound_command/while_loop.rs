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

//! Execution of the while loop

use yash_env::semantics::Result;
use yash_env::Env;
use yash_syntax::syntax::List;

/// Executes the while loop.
pub async fn execute_while(_env: &mut Env, _condition: &List, _body: &List) -> Result {
    todo!()
}

/// Executes the until loop.
pub async fn execute_until(_env: &mut Env, _condition: &List, _body: &List) -> Result {
    todo!()
}
