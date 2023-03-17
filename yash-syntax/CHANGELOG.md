# Changelog

All notable changes to `yash-syntax` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.6.1] - Unreleased

### Added

- Variants of `parser::SyntaxError`: `MissingSeparator`, `UnopenedGrouping`,
  `UnopenedSubshell`, `UnopenedLoop`, `UnopenedDoClause`, `UnopenedIf`,
  `UnopenedCase`, `InAsCommandName`

### Changed

- `syntax::Fd` to `#[repr(transparent)]`
- `parser::Parser::command_line` to return the newly added variants of
  `SyntaxError` instead of `InvalidCommandToken` depending on the type of
  erroneous tokens.
- Dependency versions
    - Rust 1.58.0 → 1.67.0
    - async-trait 0.1.56 → 0.1.66
    - futures-util 0.3.23 → 0.3.27
    - itertools 0.10.3 → 0.10.5

## [0.6.0] - 2022-10-01

### Added

- `source::Source::Arith`

### Changed

- `syntax::CompoundCommand::Subshell` from a tuple variant `Subshell(List)`
  to a struct variant `Subshell { body: Rc<List>, location: Location }`.
- Dependency versions
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
- Dependency versions
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
- Dependency versions
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

[0.6.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.6.0
[0.5.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.5.0
[0.4.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.4.0
[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.1.0
