// This file is part of yash, an extended POSIX shell.
// Copyright (C) 2020 WATANABE Yuki
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

//! Utilities for implementing delayed parsing of here-document contents.
//!
//! One of the difficulties in parsing shell scripts is handling of here-document contents. In the
//! POSIX shell script syntax, the content of a here-document appears apart from the here-document
//! operator, which means the here-document cannot be parsed in a single pass in a recursive
//! descent parser. Instead, the operator and the content have to be parsed separately and combined
//! later.
//!
//! This module contains tools to support such a multi-step parsing.

use super::core::*;
use crate::syntax::*;
use std::rc::Rc;

/// Placeholder for a here-document that is not yet fully parsed.
///
/// This object is included in the abstract syntax tree in place of a
/// [`HereDoc`](crate::syntax::HereDoc) that is yet to be parsed.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct MissingHereDoc;

/// Partial abstract syntax tree (AST) that can be filled with missing parts to create the whole,
/// final AST.
pub trait Fill<T = HereDoc> {
    /// Final AST created by filling `self`.
    type Full;

    /// Takes some items from the iterator and fills the missing parts of `self` to create
    /// the complete AST.
    ///
    /// # Panics
    ///
    /// May panic if a value has to be filled but the iterator returns `None`.
    fn fill(self, i: &mut dyn Iterator<Item = T>) -> Result<Self::Full>;
}

impl<T> Fill for Vec<T>
where
    T: Fill,
{
    type Full = Vec<<T as Fill>::Full>;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<Self::Full> {
        self.into_iter().map(|x| x.fill(i)).collect()
    }
}

impl<T> Fill for Rc<T>
where
    T: Clone + Fill,
{
    type Full = Rc<<T as Fill>::Full>;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<Self::Full> {
        Rc::try_unwrap(self)
            .unwrap_or_else(|rc| (*rc).clone())
            .fill(i)
            .map(Rc::new)
    }
}

impl Fill for RedirBody<MissingHereDoc> {
    type Full = RedirBody;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<RedirBody> {
        match self {
            RedirBody::Normal { operator, operand } => Ok(RedirBody::Normal { operator, operand }),
            RedirBody::HereDoc(MissingHereDoc) => {
                Ok(RedirBody::HereDoc(i.next().expect("missing value to fill")))
            }
        }
    }
}

impl Fill for Redir<MissingHereDoc> {
    type Full = Redir;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<Redir> {
        Ok(Redir {
            fd: self.fd,
            body: self.body.fill(i)?,
        })
    }
}

impl Fill for SimpleCommand<MissingHereDoc> {
    type Full = SimpleCommand;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<SimpleCommand> {
        Ok(SimpleCommand {
            assigns: self.assigns,
            words: self.words,
            redirs: self
                .redirs
                .into_iter()
                .map(|redir| redir.fill(i))
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl Fill for CompoundCommand<MissingHereDoc> {
    type Full = CompoundCommand;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<CompoundCommand> {
        use CompoundCommand::*;
        Ok(match self {
            Subshell(list) => Subshell(list.fill(i)?),
        })
    }
}

impl Fill for FullCompoundCommand<MissingHereDoc> {
    type Full = FullCompoundCommand;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<FullCompoundCommand> {
        let FullCompoundCommand { command, redirs } = self;
        Ok(FullCompoundCommand {
            command: command.fill(i)?,
            redirs: redirs.fill(i)?,
        })
    }
}

impl Fill for FunctionDefinition<MissingHereDoc> {
    type Full = FunctionDefinition;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<FunctionDefinition> {
        let FunctionDefinition {
            has_keyword,
            name,
            body,
        } = self;
        let body = body.fill(i)?;
        Ok(FunctionDefinition {
            has_keyword,
            name,
            body,
        })
    }
}

impl Fill for Command<MissingHereDoc> {
    type Full = Command;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<Command> {
        use Command::*;
        Ok(match self {
            Simple(c) => Simple(c.fill(i)?),
            Compound(c) => Compound(c.fill(i)?),
            Function(c) => Function(c.fill(i)?),
        })
    }
}

impl Fill for Pipeline<MissingHereDoc> {
    type Full = Pipeline;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<Pipeline> {
        let Pipeline { commands, negation } = self;
        let commands = commands.fill(i)?;
        Ok(Pipeline { commands, negation })
    }
}

impl Fill for AndOrList<MissingHereDoc> {
    type Full = AndOrList;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<AndOrList> {
        let first = self.first.fill(i)?;
        let rest = self
            .rest
            .into_iter()
            .map(|(c, p)| Ok((c, p.fill(i)?)))
            .collect::<Result<Vec<_>>>()?;
        Ok(AndOrList { first, rest })
    }
}

impl Fill for Item<MissingHereDoc> {
    type Full = Item;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<Item> {
        let and_or = self.and_or.fill(i)?;
        let is_async = self.is_async;
        Ok(Item { and_or, is_async })
    }
}

impl Fill for List<MissingHereDoc> {
    type Full = List;
    fn fill(self, i: &mut dyn Iterator<Item = HereDoc>) -> Result<List> {
        let items = self
            .items
            .into_iter()
            .map(|item| item.fill(i))
            .collect::<Result<Vec<_>>>()?;
        Ok(List { items })
    }
}
