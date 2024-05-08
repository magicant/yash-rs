# Changelog

All notable changes to `yash-syntax` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.9.0] - Unreleased

### Added

- `parser::SyntaxError::RedundantToken` variant
- `parser::lex::ParseOperatorError` struct

### Changed

- Implementations of `FromStr` for syntactic elements like `Assign` and
  `SimpleCommand` now fails with `SyntaxError::RedundantToken` if the input
  string contains a token that is not part of the element.
- `Lexer::source_string` is no longer generic. The argument type is now
  `Range<usize>` instead of a generic implementor of
  `SliceIndex<[SourceChar], Output = [SourceChar]>`.
- `Word::parse_tilde_front` no longer delimits the tilde expansion at an
  unquoted `:` character.
- `WordLexer::braced_param` now returns `Err(SyntaxError::EmptyParam)` if `{`
  is not followed by any name characters. Previously, it returned
  `Err(SyntaxError::UnclosedParam{…})` if `{` was followed by a non-name
  character other than `}`.
- `impl std::fmt::Display for SimpleCommand` now prints the command words
  after the redirections if there are no assignments and the first word looks
  like a keyword.
- `TryFromOperatorError` is now a unit struct rather than an empty normal
  struct.

### Fixed

- A newline after a bar in a pipeline was not parsed correctly if it appeared as
  a result of alias substitution.
- Alias substitution was not performed as expected for a token that follows a
  result of blank-ending alias substitution if there is a line continuation
  between them.
- Character location indexes were not calculated correctly after an alias
  substitution changed the length of the source code.

## [0.8.0] - 2024-04-09

Starting from this version, the `yash-syntax` crate can be compiled on non-Unix
platforms, where `RawFd` falls back to `i32`.

### Added

- `source::Source::Eval`
- `source::Source::DotScript`
- `source::pretty::Footer`
- `source::pretty::Message::footers`
- `source::pretty::MessageBase::footers`
- `parser::lex::Keyword::as_str`
- `parser::lex::ParseKeywordError`
- `impl FromStr for parser::lex::Keyword`
- `parser::lex::Operator::as_str`
- `parser::lex::TryFromOperatorError`

### Changed

- `parser::lex::Keyword::is_clause_delimiter` now `const` and `#[must_use]`
- `parser::lex::Operator::is_clause_delimiter` now `const` and `#[must_use]`
- `<parser::lex::Operator as FromStr>::Err` from `()` to `ParseOperatorError`
- `<syntax::AndOr as FromStr>::Err` from `()` to `ParseOperatorError`
- `<syntax::AndOr as TryFrom<parser::lex::Operator>>::Error` from `()` to `TryFromOperatorError`
- `<syntax::RedirOp as FromStr>::Err` from `()` to `ParseOperatorError`
- `<syntax::RedirOp as TryFrom<parser::lex::Operator>>::Error` from `()` to `TryFromOperatorError`

### Removed

- `impl TryFrom<&str> for parser::lex::Keyword` in favor of `FromStr`

## [0.7.0] - 2023-11-12

### Added

- `impl Default for Text`

### Changed

- Type of `HereDoc::content` from `RefCell<Text>` to `OnceCell<Text>`
- External dependency versions
    - Rust 1.67.0 → 1.70.0

## [0.6.1] - 2023-05-01

### Added

- Variants of `parser::SyntaxError`: `MissingSeparator`, `UnopenedGrouping`,
  `UnopenedSubshell`, `UnopenedLoop`, `UnopenedDoClause`, `UnopenedIf`,
  `UnopenedCase`, `InAsCommandName`

### Changed

- `parser::Parser::command_line` to return the newly added variants of
  `SyntaxError` instead of `InvalidCommandToken` depending on the type of
  erroneous tokens.
- External dependency versions
    - Rust 1.58.0 → 1.67.0
- Internal dependency versions
    - async-trait 0.1.56 → 0.1.73
    - futures-util 0.3.23 → 0.3.28
    - itertools 0.10.3 → 0.11.0
    - thiserror 1.0.43 → 1.0.47

## [0.6.0] - 2022-10-01

### Added

- `source::Source::Arith`

### Changed

- `syntax::CompoundCommand::Subshell` from a tuple variant `Subshell(List)`
  to a struct variant `Subshell { body: Rc<List>, location: Location }`.
- Internal dependency versions
    - futures-util 0.3.21 → 0.3.23

## [0.5.0] - 2022-07-02

This version contains variety of fixes.

### Added

- `parser::lex::Keyword::is_clause_delimiter`
- `parser::lex::Operator::is_clause_delimiter`
- `parser::lex::TokenId::is_clause_delimiter`
- `impl std::error::Error for parser::Error`
- `source::pretty::MessageBase`
   - `impl MessageBase for parser::Error`
   - `impl<'a, T: MessageBase> From<&'a T> for source::pretty::Message<'a>`

### Changed

- `syntax::CommandSubst::content` from `String` to `Rc<str>`
- `parser::Error` now `non_exhaustive`
- `parser::Error::UnexpectedToken` renamed to `InvalidCommandToken`
- `parser::Parser::maybe_compound_list` now returning an `InvalidCommandToken`
error if the list is delimited by a token that is not a clause delimiter.
- Internal dependency versions
    - `async-trait` 0.1.52 → 0.1.56

### Removed

- `impl<'a> From<&'a parser::Error> for source::pretty::Message<'a>`

## [0.4.0] - 2022-02-27

This version modifies the definition of `source::Location` so it refers to a
range of source code characters rather than a single character.

### Added

- `parser::lex::Lexer::location_range`

### Changed

- `source::Location::index: usize` replaced with `range: Range<usize>`
- The following functions now taking the `start_index: usize` parameter instead of `opening_location: Location`:
    - `parser::lex::Lexer::arithmetic_expansion`
    - `parser::lex::Lexer::command_substitution`
    - `parser::lex::Lexer::raw_param`
    - `parser::lex::WordLexer::braced_param`
- The following functions now returning `Result<Option<TextUnit>>` instead of `Result<Result<TextUnit, Location>>`:
    - `parser::lex::Lexer::arithmetic_expansion`
    - `parser::lex::Lexer::raw_param`
    - `parser::lex::WordLexer::braced_param`

### Removed

- `parser::lex::PartialHereDoc`
- `source::Location::advance`

## [0.3.0] - 2022-02-06

This version simplifies type definitions for the abstract syntax tree (AST);
`syntax::HereDoc::content` is now wrapped in `RefCell` to remove generic type
parameters from `RedirBody` and other AST types.

### Changed

- `source::Source` now `non_exhaustive`
- `syntax::HereDoc::content` redefined as `RefCell<Text>` (previously `Text`)
- `impl From<HereDoc> for RedirBody` replaced with `impl<T: Into<Rc<HereDoc>>> From<T> for RedirBody`
- `parser::lex::Lexer::here_doc_content` now taking a `&HereDoc` parameter and returning `Result<()>`
- `parser::Parser::memorize_unread_here_doc` now taking an `Rc<HereDoc>` parameter

### Removed

- `parser::Parser::take_read_here_docs`
- `parser::Fill`
- `parser::MissingHereDoc`
- Generic type parameters of AST types `RedirBody`, `Redir`, `SimpleCommand`, `ElifThen`, `CaseItem`, `CompoundCommand`, `FullCompoundCommand`, `FunctionDefinition`, `Command`, `Pipeline`, `AndOrList`, `Item`, `List` in the `syntax` module

## [0.2.0] - 2022-02-03

Previously, source code attribution attached to ASTs was line-oriented. The
attribution now contains a whole fragment of code corresponding to a complete
command.

### Added

- `source_chars`
- `Lexer::pending`
- `Lexer::flush`
- `impl Default for input::Context`

### Changed

- Items in the `source` module:
    - `Line` renamed to `Code`
    - `Location`'s field `line` renamed to `code`
    - `Location`'s field `column` replaced with `index`
    - `Code`'s field `value` wrapped in the `RefCell`
    - `Annotation`'s field `location` changed to a reference
    - `Annotation`'s field `code` added
    - `Annotation`'s method `new` added
- Items in the `input` module:
    - `Result` redefined as `Result<String, Error>` (previously `Result<Code, Error>`)
    - `Error` redefined as `std::io::Error` (previously `(Location, std::io::Error)`)
    - `Context` now `non_exhaustive`
    - `Memory::new` no longer taking a `Source` parameter
- `Lexer::new` now requiring the `start_line_number` and `source` parameters
- Internal dependency versions
    - `async-trait` 0.1.50 → 0.1.52
    - `futures-util` 0.3.18 → 0.3.19
    - `itertools` 0.10.1 → 0.10.3

### Removed

- `Code::enumerate`
- `Lines`
- `lines`

## [0.1.0] - 2021-12-11

### Added

- Functionalities to parse POSIX shell scripts
- Alias substitution support

[0.8.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.8.0
[0.7.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.7.0
[0.6.1]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.6.1
[0.6.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.6.0
[0.5.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.5.0
[0.4.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.4.0
[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.1.0
