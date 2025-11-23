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

use super::*;
use itertools::Itertools as _;
use std::fmt;
use std::fmt::Write as _;

impl fmt::Display for SpecialParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_char().fmt(f)
    }
}

impl fmt::Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.id.fmt(f)
    }
}

impl fmt::Display for SwitchAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SwitchAction::*;
        let c = match self {
            Alter => '+',
            Default => '-',
            Assign => '=',
            Error => '?',
        };
        f.write_char(c)
    }
}

impl fmt::Display for SwitchCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SwitchCondition::*;
        match self {
            Unset => Ok(()),
            UnsetOrEmpty => f.write_char(':'),
        }
    }
}

impl fmt::Display for Switch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}", self.condition, self.action, self.word)
    }
}

impl fmt::Display for TrimSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use TrimSide::*;
        let c = match self {
            Prefix => '#',
            Suffix => '%',
        };
        f.write_char(c)
    }
}

impl fmt::Display for Trim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.side.fmt(f)?;
        match self.length {
            TrimLength::Shortest => (),
            TrimLength::Longest => self.side.fmt(f)?,
        }
        self.pattern.fmt(f)
    }
}

impl fmt::Display for BracedParam {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Modifier::*;
        match self.modifier {
            None => write!(f, "${{{}}}", self.param),
            Length => write!(f, "${{#{}}}", self.param),
            Switch(ref switch) => write!(f, "${{{}{}}}", self.param, switch),
            Trim(ref trim) => write!(f, "${{{}{}}}", self.param, trim),
        }
    }
}

impl fmt::Display for BackquoteUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackquoteUnit::Literal(c) => write!(f, "{c}"),
            BackquoteUnit::Backslashed(c) => write!(f, "\\{c}"),
        }
    }
}

impl fmt::Display for TextUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Literal(c) => write!(f, "{c}"),
            Backslashed(c) => write!(f, "\\{c}"),
            RawParam { param, .. } => write!(f, "${param}"),
            BracedParam(param) => param.fmt(f),
            CommandSubst { content, .. } => write!(f, "$({content})"),
            Backquote { content, .. } => {
                f.write_char('`')?;
                content.iter().try_for_each(|unit| unit.fmt(f))?;
                f.write_char('`')
            }
            Arith { content, .. } => write!(f, "$(({content}))"),
        }
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|unit| unit.fmt(f))
    }
}

impl fmt::Display for EscapeUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(c) => c.fmt(f),
            Self::DoubleQuote => f.write_str("\\\""),
            Self::SingleQuote => f.write_str("\\'"),
            Self::Backslash => f.write_str("\\\\"),
            Self::Question => f.write_str("\\?"),
            Self::Alert => f.write_str("\\a"),
            Self::Backspace => f.write_str("\\b"),
            Self::Escape => f.write_str("\\e"),
            Self::FormFeed => f.write_str("\\f"),
            Self::Newline => f.write_str("\\n"),
            Self::CarriageReturn => f.write_str("\\r"),
            Self::Tab => f.write_str("\\t"),
            Self::VerticalTab => f.write_str("\\v"),
            Self::Control(b) => write!(f, "\\c{}", (*b ^ 0x40) as char),
            Self::Octal(b) => write!(f, "\\{b:03o}"),
            Self::Hex(b) => write!(f, "\\x{b:02X}"),
            Self::Unicode(c) if *c <= '\u{FFFF}' => write!(f, "\\u{:04x}", *c as u32),
            Self::Unicode(c) => write!(f, "\\U{:08X}", *c as u32),
        }
    }
}

impl fmt::Display for EscapedString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.iter().try_for_each(|unit| unit.fmt(f))
    }
}

impl fmt::Display for WordUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Unquoted(dq) => dq.fmt(f),
            SingleQuote(s) => write!(f, "'{s}'"),
            DoubleQuote(content) => write!(f, "\"{content}\""),
            DollarSingleQuote(content) => write!(f, "$'{content}'"),
            Tilde { name, .. } => write!(f, "~{name}"),
        }
    }
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.units.iter().try_for_each(|unit| write!(f, "{unit}"))
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Scalar(word) => word.fmt(f),
            Array(words) => write!(f, "({})", words.iter().format(" ")),
        }
    }
}

impl fmt::Display for Assign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", &self.name, &self.value)
    }
}

impl fmt::Display for RedirOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Operator::from(*self).fmt(f)
    }
}

impl fmt::Display for HereDoc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.remove_tabs { "<<-" } else { "<<" })?;

        // This space is to disambiguate `<< --` and `<<- -`
        if let Some(Unquoted(Literal('-'))) = self.delimiter.units.first() {
            f.write_char(' ')?;
        }

        write!(f, "{}", self.delimiter)
    }
}

impl fmt::Display for RedirBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RedirBody::Normal { operator, operand } => write!(f, "{operator}{operand}"),
            RedirBody::HereDoc(h) => write!(f, "{h}"),
        }
    }
}

impl fmt::Display for Redir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(fd) = self.fd {
            write!(f, "{fd}")?;
        }
        write!(f, "{}", self.body)
    }
}

impl fmt::Display for SimpleCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let i1 = self.assigns.iter().map(|x| x as &dyn fmt::Display);
        let i2 = self.words.iter().map(|x| &x.0 as &dyn fmt::Display);
        let i3 = self.redirs.iter().map(|x| x as &dyn fmt::Display);

        if !self.assigns.is_empty() || !self.first_word_is_keyword() {
            write!(f, "{}", i1.chain(i2).chain(i3).format(" "))
        } else {
            // We usually display the words before the redirections, but when
            // the first word is a keyword and there are no assignments, we
            // display the redirections first to make sure the simple command is
            // not mistaken for a compound command.
            write!(f, "{}", i3.chain(i2).format(" "))
        }
    }
}

impl fmt::Display for ElifThen {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "elif {:#} then ", self.condition)?;
        if f.alternate() {
            write!(f, "{:#}", self.body)
        } else {
            write!(f, "{}", self.body)
        }
    }
}

impl fmt::Display for CaseContinuation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Operator::from(*self).fmt(f)
    }
}

impl fmt::Display for CaseItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "({}) {}{}",
            self.patterns.iter().format(" | "),
            self.body,
            self.continuation,
        )
    }
}

impl fmt::Display for CompoundCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use CompoundCommand::*;
        match self {
            Grouping(list) => write!(f, "{{ {list:#} }}"),
            Subshell { body, .. } => write!(f, "({body})"),
            For { name, values, body } => {
                write!(f, "for {name}")?;
                if let Some(values) = values {
                    f.write_str(" in")?;
                    for value in values {
                        write!(f, " {value}")?;
                    }
                    f.write_char(';')?;
                }
                write!(f, " do {body:#} done")
            }
            While { condition, body } => write!(f, "while {condition:#} do {body:#} done"),
            Until { condition, body } => write!(f, "until {condition:#} do {body:#} done"),
            If {
                condition,
                body,
                elifs,
                r#else,
            } => {
                write!(f, "if {condition:#} then {body:#} ")?;
                for elif in elifs {
                    write!(f, "{elif:#} ")?;
                }
                if let Some(r#else) = r#else {
                    write!(f, "else {else:#} ")?;
                }
                f.write_str("fi")
            }
            Case { subject, items } => {
                write!(f, "case {subject} in ")?;
                for item in items {
                    write!(f, "{item} ")?;
                }
                f.write_str("esac")
            }
        }
    }
}

impl fmt::Display for FullCompoundCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let FullCompoundCommand { command, redirs } = self;
        write!(f, "{command}")?;
        redirs.iter().try_for_each(|redir| write!(f, " {redir}"))
    }
}

impl fmt::Display for FunctionDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.has_keyword {
            f.write_str("function ")?;
        }
        write!(f, "{}() {}", self.name, self.body)
    }
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Simple(c) => c.fmt(f),
            Command::Compound(c) => c.fmt(f),
            Command::Function(c) => c.fmt(f),
        }
    }
}

impl fmt::Display for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> fmt::Result {
        if self.negation {
            write!(f, "! ")?;
        }
        write!(f, "{}", self.commands.iter().format(" | "))
    }
}

impl fmt::Display for AndOr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AndOr::AndThen => write!(f, "&&"),
            AndOr::OrElse => write!(f, "||"),
        }
    }
}

impl fmt::Display for AndOrList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.first)?;
        self.rest
            .iter()
            .try_for_each(|(c, p)| write!(f, " {c} {p}"))
    }
}

/// Allows conversion from Item to String.
///
/// By default, the `;` terminator is omitted from the formatted string.
/// When the alternate flag is specified as in `{:#}`, the result is always
/// terminated by either `;` or `&`.
impl fmt::Display for Item {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.and_or)?;
        if self.async_flag.is_some() {
            write!(f, "&")
        } else if f.alternate() {
            write!(f, ";")
        } else {
            Ok(())
        }
    }
}

/// Allows conversion from List to String.
///
/// By default, the last `;` terminator is omitted from the formatted string.
/// When the alternate flag is specified as in `{:#}`, the result is always
/// terminated by either `;` or `&`.
impl fmt::Display for List {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some((last, others)) = self.0.split_last() {
            for item in others {
                write!(f, "{item:#} ")?;
            }
            if f.alternate() {
                write!(f, "{last:#}")
            } else {
                write!(f, "{last}")
            }
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switch_display() {
        let switch = Switch {
            action: SwitchAction::Alter,
            condition: SwitchCondition::Unset,
            word: "".parse().unwrap(),
        };
        assert_eq!(switch.to_string(), "+");

        let switch = Switch {
            action: SwitchAction::Default,
            condition: SwitchCondition::UnsetOrEmpty,
            word: "foo".parse().unwrap(),
        };
        assert_eq!(switch.to_string(), ":-foo");

        let switch = Switch {
            action: SwitchAction::Assign,
            condition: SwitchCondition::UnsetOrEmpty,
            word: "bar baz".parse().unwrap(),
        };
        assert_eq!(switch.to_string(), ":=bar baz");

        let switch = Switch {
            action: SwitchAction::Error,
            condition: SwitchCondition::Unset,
            word: "?error".parse().unwrap(),
        };
        assert_eq!(switch.to_string(), "??error");
    }

    #[test]
    fn trim_display() {
        let trim = Trim {
            side: TrimSide::Prefix,
            length: TrimLength::Shortest,
            pattern: "foo".parse().unwrap(),
        };
        assert_eq!(trim.to_string(), "#foo");

        let trim = Trim {
            side: TrimSide::Prefix,
            length: TrimLength::Longest,
            pattern: "".parse().unwrap(),
        };
        assert_eq!(trim.to_string(), "##");

        let trim = Trim {
            side: TrimSide::Suffix,
            length: TrimLength::Shortest,
            pattern: "bar".parse().unwrap(),
        };
        assert_eq!(trim.to_string(), "%bar");

        let trim = Trim {
            side: TrimSide::Suffix,
            length: TrimLength::Longest,
            pattern: "*".parse().unwrap(),
        };
        assert_eq!(trim.to_string(), "%%*");
    }

    #[test]
    fn braced_param_display() {
        let param = BracedParam {
            param: Param::variable("foo"),
            modifier: Modifier::None,
            location: Location::dummy(""),
        };
        assert_eq!(param.to_string(), "${foo}");

        let param = BracedParam {
            modifier: Modifier::Length,
            ..param
        };
        assert_eq!(param.to_string(), "${#foo}");

        let switch = Switch {
            action: SwitchAction::Assign,
            condition: SwitchCondition::UnsetOrEmpty,
            word: "bar baz".parse().unwrap(),
        };
        let param = BracedParam {
            modifier: Modifier::Switch(switch),
            ..param
        };
        assert_eq!(param.to_string(), "${foo:=bar baz}");

        let trim = Trim {
            side: TrimSide::Suffix,
            length: TrimLength::Shortest,
            pattern: "baz' 'bar".parse().unwrap(),
        };
        let param = BracedParam {
            modifier: Modifier::Trim(trim),
            ..param
        };
        assert_eq!(param.to_string(), "${foo%baz' 'bar}");
    }

    #[test]
    fn backquote_unit_display() {
        let literal = BackquoteUnit::Literal('A');
        assert_eq!(literal.to_string(), "A");
        let backslashed = BackquoteUnit::Backslashed('X');
        assert_eq!(backslashed.to_string(), r"\X");
    }

    #[test]
    fn text_unit_display() {
        let literal = Literal('A');
        assert_eq!(literal.to_string(), "A");
        let backslashed = Backslashed('X');
        assert_eq!(backslashed.to_string(), r"\X");

        let raw_param = RawParam {
            param: Param::variable("PARAM"),
            location: Location::dummy(""),
        };
        assert_eq!(raw_param.to_string(), "$PARAM");

        let command_subst = CommandSubst {
            content: r"foo\bar".into(),
            location: Location::dummy(""),
        };
        assert_eq!(command_subst.to_string(), r"$(foo\bar)");

        let backquote = Backquote {
            content: vec![
                BackquoteUnit::Literal('a'),
                BackquoteUnit::Backslashed('b'),
                BackquoteUnit::Backslashed('c'),
                BackquoteUnit::Literal('d'),
            ],
            location: Location::dummy(""),
        };
        assert_eq!(backquote.to_string(), r"`a\b\cd`");

        let arith = Arith {
            content: Text(vec![literal, backslashed, command_subst, backquote]),
            location: Location::dummy(""),
        };
        assert_eq!(arith.to_string(), r"$((A\X$(foo\bar)`a\b\cd`))");
    }

    #[test]
    fn escape_unit_display() {
        use EscapeUnit::*;

        assert_eq!(Literal('A').to_string(), "A");
        assert_eq!(DoubleQuote.to_string(), r#"\""#);
        assert_eq!(SingleQuote.to_string(), r"\'");
        assert_eq!(Backslash.to_string(), r"\\");
        assert_eq!(Question.to_string(), r"\?");
        assert_eq!(Alert.to_string(), r"\a");
        assert_eq!(Backspace.to_string(), r"\b");
        assert_eq!(Escape.to_string(), r"\e");
        assert_eq!(FormFeed.to_string(), r"\f");
        assert_eq!(Newline.to_string(), r"\n");
        assert_eq!(CarriageReturn.to_string(), r"\r");
        assert_eq!(Tab.to_string(), r"\t");
        assert_eq!(VerticalTab.to_string(), r"\v");
        assert_eq!(Control(b'\x01').to_string(), r"\cA");
        assert_eq!(Control(b'\x7F').to_string(), r"\c?");
        assert_eq!(Octal(0o003).to_string(), r"\003");
        assert_eq!(Octal(0o123).to_string(), r"\123");
        assert_eq!(Hex(0x05).to_string(), r"\x05");
        assert_eq!(Hex(0xAB).to_string(), r"\xAB");
        assert_eq!(Unicode('A').to_string(), r"\u0041");
        assert_eq!(Unicode('😊').to_string(), r"\U0001F60A");
    }

    #[test]
    fn word_unit_display() {
        let unquoted = Unquoted(Literal('A'));
        assert_eq!(unquoted.to_string(), "A");
        let unquoted = Unquoted(Backslashed('B'));
        assert_eq!(unquoted.to_string(), "\\B");

        let single_quote = SingleQuote("".to_string());
        assert_eq!(single_quote.to_string(), "''");
        let single_quote = SingleQuote(r#"a"b"c\"#.to_string());
        assert_eq!(single_quote.to_string(), r#"'a"b"c\'"#);

        let double_quote = DoubleQuote(Text(vec![]));
        assert_eq!(double_quote.to_string(), "\"\"");
        let double_quote = DoubleQuote(Text(vec![Literal('A'), Backslashed('B')]));
        assert_eq!(double_quote.to_string(), "\"A\\B\"");

        let dollar_single_quote = DollarSingleQuote(EscapedString(vec![]));
        assert_eq!(dollar_single_quote.to_string(), "$''");
        let dollar_single_quote = DollarSingleQuote(EscapedString(vec![
            EscapeUnit::Literal('A'),
            EscapeUnit::Backslash,
        ]));
        assert_eq!(dollar_single_quote.to_string(), r"$'A\\'");

        let tilde = Tilde {
            name: "".to_string(),
            followed_by_slash: false,
        };
        assert_eq!(tilde.to_string(), "~");
        let tilde = Tilde {
            name: "foo".to_string(),
            followed_by_slash: true,
        };
        assert_eq!(tilde.to_string(), "~foo");
    }

    #[test]
    fn scalar_display() {
        let s = Scalar(Word::from_str("my scalar value").unwrap());
        assert_eq!(s.to_string(), "my scalar value");
    }

    #[test]
    fn array_display_empty() {
        let a = Array(vec![]);
        assert_eq!(a.to_string(), "()");
    }

    #[test]
    fn array_display_one() {
        let a = Array(vec![Word::from_str("one").unwrap()]);
        assert_eq!(a.to_string(), "(one)");
    }

    #[test]
    fn array_display_many() {
        let a = Array(vec![
            Word::from_str("let").unwrap(),
            Word::from_str("me").unwrap(),
            Word::from_str("see").unwrap(),
        ]);
        assert_eq!(a.to_string(), "(let me see)");
    }

    #[test]
    fn assign_display() {
        let mut a = Assign::from_str("foo=bar").unwrap();
        assert_eq!(a.to_string(), "foo=bar");

        a.value = Array(vec![]);
        assert_eq!(a.to_string(), "foo=()");
    }

    #[test]
    fn here_doc_display() {
        let heredoc = HereDoc {
            delimiter: Word::from_str("END").unwrap(),
            remove_tabs: true,
            content: Text::from_str("here").unwrap().into(),
        };
        assert_eq!(heredoc.to_string(), "<<-END");

        let heredoc = HereDoc {
            delimiter: Word::from_str("XXX").unwrap(),
            remove_tabs: false,
            content: Text::from_str("there").unwrap().into(),
        };
        assert_eq!(heredoc.to_string(), "<<XXX");
    }

    #[test]
    fn here_doc_display_disambiguation() {
        let heredoc = HereDoc {
            delimiter: Word::from_str("--").unwrap(),
            remove_tabs: false,
            content: Text::from_str("here").unwrap().into(),
        };
        assert_eq!(heredoc.to_string(), "<< --");

        let heredoc = HereDoc {
            delimiter: Word::from_str("-").unwrap(),
            remove_tabs: true,
            content: Text::from_str("here").unwrap().into(),
        };
        assert_eq!(heredoc.to_string(), "<<- -");
    }

    #[test]
    fn redir_display() {
        let heredoc = HereDoc {
            delimiter: Word::from_str("END").unwrap(),
            remove_tabs: false,
            content: Text::from_str("here").unwrap().into(),
        };

        let redir = Redir {
            fd: None,
            body: heredoc.into(),
        };
        assert_eq!(redir.to_string(), "<<END");
        let redir = Redir {
            fd: Some(Fd(0)),
            ..redir
        };
        assert_eq!(redir.to_string(), "0<<END");
        let redir = Redir {
            fd: Some(Fd(9)),
            ..redir
        };
        assert_eq!(redir.to_string(), "9<<END");
    }

    #[test]
    fn simple_command_display() {
        let mut command = SimpleCommand {
            assigns: vec![],
            words: vec![],
            redirs: vec![].into(),
        };
        assert_eq!(command.to_string(), "");

        command
            .assigns
            .push(Assign::from_str("name=value").unwrap());
        assert_eq!(command.to_string(), "name=value");

        command
            .assigns
            .push(Assign::from_str("hello=world").unwrap());
        assert_eq!(command.to_string(), "name=value hello=world");

        command
            .words
            .push((Word::from_str("echo").unwrap(), ExpansionMode::Multiple));
        assert_eq!(command.to_string(), "name=value hello=world echo");

        command
            .words
            .push((Word::from_str("foo").unwrap(), ExpansionMode::Single));
        assert_eq!(command.to_string(), "name=value hello=world echo foo");

        Rc::make_mut(&mut command.redirs).push(Redir {
            fd: None,
            body: RedirBody::from(HereDoc {
                delimiter: Word::from_str("END").unwrap(),
                remove_tabs: false,
                content: Text::from_str("").unwrap().into(),
            }),
        });
        assert_eq!(command.to_string(), "name=value hello=world echo foo <<END");

        command.assigns.clear();
        assert_eq!(command.to_string(), "echo foo <<END");

        command.words.clear();
        assert_eq!(command.to_string(), "<<END");

        Rc::make_mut(&mut command.redirs).push(Redir {
            fd: Some(Fd(1)),
            body: RedirBody::from(HereDoc {
                delimiter: Word::from_str("here").unwrap(),
                remove_tabs: true,
                content: Text::from_str("ignored").unwrap().into(),
            }),
        });
        assert_eq!(command.to_string(), "<<END 1<<-here");

        command.assigns.push(Assign::from_str("foo=bar").unwrap());
        assert_eq!(command.to_string(), "foo=bar <<END 1<<-here");
    }

    #[test]
    fn simple_command_display_with_keyword() {
        let command = SimpleCommand {
            assigns: vec![],
            words: vec![("if".parse().unwrap(), ExpansionMode::Multiple)],
            redirs: vec!["<foo".parse().unwrap()].into(),
        };
        assert_eq!(command.to_string(), "<foo if");
    }

    #[test]
    fn elif_then_display() {
        let condition: List = "c 1& c 2".parse().unwrap();
        let body = "b 1& b 2".parse().unwrap();
        let elif = ElifThen { condition, body };
        assert_eq!(format!("{elif}"), "elif c 1& c 2; then b 1& b 2");
        assert_eq!(format!("{elif:#}"), "elif c 1& c 2; then b 1& b 2;");

        let condition: List = "c&".parse().unwrap();
        let body = "b&".parse().unwrap();
        let elif = ElifThen { condition, body };
        assert_eq!(format!("{elif}"), "elif c& then b&");
        assert_eq!(format!("{elif:#}"), "elif c& then b&");
    }

    #[test]
    fn case_item_display() {
        let item = CaseItem {
            patterns: vec!["foo".parse().unwrap()],
            body: "".parse::<List>().unwrap(),
            continuation: CaseContinuation::Break,
        };
        assert_eq!(item.to_string(), "(foo) ;;");

        let item = CaseItem {
            patterns: vec!["bar".parse().unwrap()],
            body: "echo ok".parse::<List>().unwrap(),
            continuation: CaseContinuation::Break,
        };
        assert_eq!(item.to_string(), "(bar) echo ok;;");

        let item = CaseItem {
            patterns: ["a", "b", "c"].iter().map(|s| s.parse().unwrap()).collect(),
            body: "foo; bar&".parse::<List>().unwrap(),
            continuation: CaseContinuation::Break,
        };
        assert_eq!(item.to_string(), "(a | b | c) foo; bar&;;");

        let item = CaseItem {
            patterns: vec!["foo".parse().unwrap()],
            body: "bar".parse::<List>().unwrap(),
            continuation: CaseContinuation::FallThrough,
        };
        assert_eq!(item.to_string(), "(foo) bar;&");
    }

    #[test]
    fn grouping_display() {
        let list = "foo".parse::<List>().unwrap();
        let grouping = CompoundCommand::Grouping(list);
        assert_eq!(grouping.to_string(), "{ foo; }");
    }

    #[test]
    fn for_display_without_values() {
        let name = Word::from_str("foo").unwrap();
        let values = None;
        let body = "echo ok".parse::<List>().unwrap();
        let r#for = CompoundCommand::For { name, values, body };
        assert_eq!(r#for.to_string(), "for foo do echo ok; done");
    }

    #[test]
    fn for_display_with_empty_values() {
        let name = Word::from_str("foo").unwrap();
        let values = Some(vec![]);
        let body = "echo ok".parse::<List>().unwrap();
        let r#for = CompoundCommand::For { name, values, body };
        assert_eq!(r#for.to_string(), "for foo in; do echo ok; done");
    }

    #[test]
    fn for_display_with_some_values() {
        let name = Word::from_str("V").unwrap();
        let values = Some(vec![
            Word::from_str("a").unwrap(),
            Word::from_str("b").unwrap(),
        ]);
        let body = "one; two&".parse::<List>().unwrap();
        let r#for = CompoundCommand::For { name, values, body };
        assert_eq!(r#for.to_string(), "for V in a b; do one; two& done");
    }

    #[test]
    fn while_display() {
        let condition = "true& false".parse::<List>().unwrap();
        let body = "echo ok".parse::<List>().unwrap();
        let r#while = CompoundCommand::While { condition, body };
        assert_eq!(r#while.to_string(), "while true& false; do echo ok; done");
    }

    #[test]
    fn until_display() {
        let condition = "true& false".parse::<List>().unwrap();
        let body = "echo ok".parse::<List>().unwrap();
        let until = CompoundCommand::Until { condition, body };
        assert_eq!(until.to_string(), "until true& false; do echo ok; done");
    }

    #[test]
    fn if_display() {
        let r#if: CompoundCommand = CompoundCommand::If {
            condition: "c 1; c 2&".parse().unwrap(),
            body: "b 1; b 2&".parse().unwrap(),
            elifs: vec![],
            r#else: None,
        };
        assert_eq!(r#if.to_string(), "if c 1; c 2& then b 1; b 2& fi");

        let r#if: CompoundCommand = CompoundCommand::If {
            condition: "c 1& c 2;".parse().unwrap(),
            body: "b 1& b 2;".parse().unwrap(),
            elifs: vec![ElifThen {
                condition: "c 3&".parse().unwrap(),
                body: "b 3&".parse().unwrap(),
            }],
            r#else: Some("b 4".parse().unwrap()),
        };
        assert_eq!(
            r#if.to_string(),
            "if c 1& c 2; then b 1& b 2; elif c 3& then b 3& else b 4; fi"
        );

        let r#if: CompoundCommand = CompoundCommand::If {
            condition: "true".parse().unwrap(),
            body: ":".parse().unwrap(),
            elifs: vec![
                ElifThen {
                    condition: "false".parse().unwrap(),
                    body: "a".parse().unwrap(),
                },
                ElifThen {
                    condition: "echo&".parse().unwrap(),
                    body: "b&".parse().unwrap(),
                },
            ],
            r#else: None,
        };
        assert_eq!(
            r#if.to_string(),
            "if true; then :; elif false; then a; elif echo& then b& fi"
        );
    }

    #[test]
    fn case_display() {
        let subject = "foo".parse().unwrap();
        let items = Vec::<CaseItem>::new();
        let case = CompoundCommand::Case { subject, items };
        assert_eq!(case.to_string(), "case foo in esac");

        let subject = "bar".parse().unwrap();
        let items = vec!["foo)".parse::<CaseItem>().unwrap()];
        let case = CompoundCommand::Case { subject, items };
        assert_eq!(case.to_string(), "case bar in (foo) ;; esac");

        let subject = "baz".parse().unwrap();
        let items = vec![
            "1)".parse::<CaseItem>().unwrap(),
            "(a|b|c) :&".parse().unwrap(),
        ];
        let case = CompoundCommand::Case { subject, items };
        assert_eq!(case.to_string(), "case baz in (1) ;; (a | b | c) :&;; esac");
    }

    #[test]
    fn function_definition_display() {
        let body = FullCompoundCommand {
            command: "( bar )".parse::<CompoundCommand>().unwrap(),
            redirs: vec![],
        };
        let fd = FunctionDefinition {
            has_keyword: false,
            name: Word::from_str("foo").unwrap(),
            body: Rc::new(body),
        };
        assert_eq!(fd.to_string(), "foo() (bar)");
    }

    #[test]
    fn pipeline_display() {
        let mut p = Pipeline {
            commands: vec![Rc::new("first".parse::<Command>().unwrap())],
            negation: false,
        };
        assert_eq!(p.to_string(), "first");

        p.negation = true;
        assert_eq!(p.to_string(), "! first");

        p.commands.push(Rc::new("second".parse().unwrap()));
        assert_eq!(p.to_string(), "! first | second");

        p.commands.push(Rc::new("third".parse().unwrap()));
        p.negation = false;
        assert_eq!(p.to_string(), "first | second | third");
    }

    #[test]
    fn and_or_list_display() {
        let p = "first".parse::<Pipeline>().unwrap();
        let mut aol = AndOrList {
            first: p,
            rest: vec![],
        };
        assert_eq!(aol.to_string(), "first");

        let p = "second".parse().unwrap();
        aol.rest.push((AndOr::AndThen, p));
        assert_eq!(aol.to_string(), "first && second");

        let p = "third".parse().unwrap();
        aol.rest.push((AndOr::OrElse, p));
        assert_eq!(aol.to_string(), "first && second || third");
    }

    #[test]
    fn list_display() {
        let and_or = "first".parse::<AndOrList>().unwrap();
        let item = Item {
            and_or: Rc::new(and_or),
            async_flag: None,
        };
        let mut list = List(vec![item]);
        assert_eq!(list.to_string(), "first");

        let and_or = "second".parse().unwrap();
        let item = Item {
            and_or: Rc::new(and_or),
            async_flag: Some(Location::dummy("")),
        };
        list.0.push(item);
        assert_eq!(list.to_string(), "first; second&");

        let and_or = "third".parse().unwrap();
        let item = Item {
            and_or: Rc::new(and_or),
            async_flag: None,
        };
        list.0.push(item);
        assert_eq!(list.to_string(), "first; second& third");
    }

    #[test]
    fn list_display_alternate() {
        let and_or = "first".parse::<AndOrList>().unwrap();
        let item = Item {
            and_or: Rc::new(and_or),
            async_flag: None,
        };
        let mut list = List(vec![item]);
        assert_eq!(format!("{list:#}"), "first;");

        let and_or = "second".parse().unwrap();
        let item = Item {
            and_or: Rc::new(and_or),
            async_flag: Some(Location::dummy("")),
        };
        list.0.push(item);
        assert_eq!(format!("{list:#}"), "first; second&");

        let and_or = "third".parse().unwrap();
        let item = Item {
            and_or: Rc::new(and_or),
            async_flag: None,
        };
        list.0.push(item);
        assert_eq!(format!("{list:#}"), "first; second& third;");
    }
}
