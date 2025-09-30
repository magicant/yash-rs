# Changelog

All notable changes to `yash-syntax` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Terminology: A _public dependency_ is one that’s exposed through this crate’s
public API (e.g., re-exported types).
A _private dependency_ is used internally and not visible to downstream users.

## [0.16.0] - Unreleased

### Changed

- Renamed `syntax::SwitchType` to `syntax::SwitchAction` and
  `syntax::Switch::type` to `syntax::Switch::action` to better reflect their
  purpose.
- Updated the optional public dependency annotate-snippets from 0.11.4 to
  0.12.4. Items provided by this crate have been redefined to reflect the
  changes in the new version of annotate-snippets:
    - `impl From<AnnotationType> for annotate_snippets::Level` →
      `impl<'a> From<AnnotationType> for annotate_snippets::Level<'a>`
    - Added `impl From<AnnotationType> for annotate_snippets::AnnotationKind`
    - `impl<'a> From<&'a Message<'a>> for annotate_snippets::Message<'a>` →
      `impl<'a> From<&'a Message<'a>> for annotate_snippets::Group<'a>`

## [0.15.2] - 2025-09-23

### Added

- New error variants in `parser::SyntaxError`:
    - `UnsupportedFunctionDefinitionSyntax`
    - `UnsupportedDoubleBracketCommand`
    - `UnsupportedProcessRedirection`

### Changed

- The command parser (`parser::Parser::command`) now raises
  `SyntaxError::UnsupportedFunctionDefinitionSyntax` and
  `SyntaxError::UnsupportedDoubleBracketCommand` when it encounters the
  `function` and `[[` reserved words, respectively. These syntaxes are not yet
  supported.
- The redirection parser (`parser::Parser::redirection`) now raises
  `SyntaxError::UnsupportedProcessRedirection` when it encounters process
  redirections (`>(...)` and `<(...)`). This syntax is not yet supported.

## [0.15.1] - 2025-09-14

### Added

- Added the `Location::byte_range` method to obtain the byte range corresponding
  to a character range in source code locations.

### Fixed

- The implementation of
  `From<&'a source::pretty::Message<'a>> for annotate_snippets::Message<'a>` was
  incorrectly using character ranges instead of byte ranges for source
  annotations, which could lead to incorrect highlighting in messages.

## [0.15.0] - 2025-05-11

### Added

- The `parser::lex::TokenId` enum now has the `IoLocation` variant.
- The `parser::SyntaxError` enum now has the `InvalidIoLocation` variant.

### Changed

- The `parser::lex::Lexer::token` method now returns a `Token` with the
  `TokenId::IoLocation` variant if the token is of the form `{...}` and
  immediately precedes a redirection operator.
- The `parser::Parser::redirection` method now fails with a
  `SyntaxError::InvalidIoLocation` if it encounters an I/O location token
  (e.g., `{n}>/dev/null`) preceding a redirection operator.
    - Currently, the parser does not support parsing of I/O location tokens.
      This error is returned whenever the parser finds an I/O location token
      attached to a redirection operator.
- The associated value of the `syntax::WordUnit::Tilde` enum variant has been
  changed to have two named fields: `name: String` and `followed_by_slash: bool`.
  This is needed to support correct adjustment of the number of slashes in the
  tilde expansion that is followed by a slash.
- The for loop parser (`parser::Parser::for_loop`) now returns
  `SyntaxError::MissingForBody` instead of `SyntaxError::InvalidForValue` if it
  encounters the end of the input while parsing the word list of the for loop.
  This should provide a more accurate error message for this case.

## [0.14.1] - 2025-05-03

### Added

- `parser::lex::is_name` and `parser::lex::is_portable_name`
    - These functions can be used to check if a string is a valid name.

## [0.14.0] - 2025-03-23

This version adds support for declaration utilities. It also reorganizes how the
parser is configured on construction, so that the parser can be constructed with
more flexible and readable configurations.

### Added

- The `syntax::ExpansionMode` enum is added to represent how a word is expanded.
- The `decl_util` module is added, which contains the `Glossary` trait and the
  `EmptyGlossary` and `PosixGlossary` structs.
- Added the `Config` struct to the `parser` module. Currently, it allows
  setting glossaries for aliases and declaration utilities for the parser.
- Added the `Config` struct to the `parser::lex` module. Currently, it allows
  setting the starting line number and the source information for the lexer.
- The `syntax::Word::parse_tilde_everywhere_after` method is added.
- The `with_code` function is added to the `parser::lex::Lexer` struct.
- The `From<&str>` trait is now implemented for `input::Memory`.

### Changed

- The `syntax::SimpleCommand::words` field is now a `Vec<(Word, ExpansionMode)>`
  instead of a `Vec<Word>`.
- The `parser::Parser::new` function now only takes a `&mut Lexer` argument.
  The `&dyn alias::Glossary` argument has been removed in favor of the `Config`
  struct.
- When a simple command is parsed, the parser now checks if the command name is
  a declaration utility. If it is, following words in an assignment form are
  parsed like assignments.
- The `parser::lex::Lexer` struct is now `#[must_use]`.
- The `parser::lex::Lexer::new` method now only takes a `Box<dyn InputObject>`
  argument. The `start_line_number: NonZeroU64` and `source: Rc<Source>`
  arguments have been removed in favor of construction with a `Config` struct.
- Public dependency versions:
    - Rust 1.82.0 → 1.85.0
- Private dependency versions:
    - itertools 0.13.0 → 0.14.0

## [0.13.0] - 2024-12-14

### Added

- Extended the case item syntax to allow `;&`, `;|`, and `;;&` as terminators.
    - The `SemicolonAnd`, `SemicolonSemicolonAnd`, and `SemicolonBar` variants
      are added to the `parser::lex::Operator` enum.
    - The `parser::Parser::case_item` and `syntax::CaseItem::from_str` methods
      now consume a trailing terminator token, if any. The terminator can be
      not only `;;`, but also `;&`, `;|`, or `;;&`.
- Dollar-single-quotes are now supported.
    - The `DollarSingleQuote` variant is added to the `syntax::WordUnit` enum.
    - The `EscapeUnit` enum and `EscapedString` struct are added to the
      `syntax` module.
    - The `escape_unit` and `escaped_string` methods are added to the
      `parser::lex::Lexer` struct.
    - The following error variants are added to `parser::SyntaxError`:
        - `IncompleteControlBackslashEscape`
        - `IncompleteControlEscape`
        - `IncompleteEscape`
        - `IncompleteHexEscape`
        - `IncompleteLongUnicodeEscape`
        - `IncompleteShortUnicodeEscape`
        - `InvalidControlEscape`
        - `InvalidEscape`
        - `UnclosedDollarSingleQuote`
        - `UnicodeEscapeOutOfRange`
- In the `syntax::MaybeLiteral` trait, the `extend_if_literal` method is
  replaced with the `extend_literal` method, which now takes a mutable reference
  to an `Extend<char>` object, instead of an ownership of it. The method may
  leave intermediate results in the `Extend<char>` object if unsuccessful.
    - The `syntax::NotLiteral` struct is added to represent the case where the
      method is unsuccessful.

### Changed

- Error messages returned from `parser::SyntaxError::message` are no longer
  capitalized.
- The implementations of `std::str::FromStr` for `TextUnit` and `WordUnit` in
  the `syntax` module now return `Option<Error>` instead of `Error` for the
  error type. Previously, the `from_str` method was panicking when the input
  string was empty.
- Private dependency versions:
    - thiserror 1.0.47 → 2.0.4

### Removed

- As mentioned above, the `extend_if_literal` method of the
  `syntax::MaybeLiteral` trait is removed in favor of the new `extend_literal`
  method.

## [0.12.1] - 2024-11-10

### Changed

- In the `parser::Parser::case_item` method, the parser now accepts an `esac`
  token as a pattern after an opening parenthesis, as required by POSIX.1-2024.
  The previous version of POSIX did not allow `esac` as the first pattern, so
  the method was returning `SyntaxError::EsacAsPattern` in that case.
- Public dependency versions:
    - Rust 1.79.0 → 1.82.0
- Private dependency versions:
    - futures-util 0.3.28 → 0.3.31

### Deprecated

- `parser::SyntaxError::EsacAsPattern`
    - This variant is deprecated because this error condition is no longer
      possible as described above.

## [0.12.0] - 2024-09-29

### Added

- `input::InputObject` trait
    - This new trait is an object-safe version of `input::Input`.

### Changed

- The `input::Input` trait is now `#[must_use]`.
- The `input::Input::next_line` method now returns
  `impl Future<Output = input::Result>`. This change reduces the number of
  allocations when reading input lines.

### Removed

- Private dependencies:
    - async-trait 0.1.73

## [0.11.0] - 2024-08-22

### Added

- The following functions are now `const`:
    - `parser::lex::is_portable_name_char`
    - `parser::lex::is_special_parameter_char`
- The `parser::lex::is_single_char_name` const function is added.
- A new `syntax::Param` struct is introduced to represent a parameter in
  parameter expansions (`syntax::TextUnit::RawParam` and `syntax::BracedParam`).
  New enum types `syntax::SpecialParam` and `syntax::ParamType` are added to
  represent the details of the parameter. Note that the former `syntax::Param`
  struct is renamed to `syntax::BracedParam`.
- The `parser::SyntaxError::InvalidParam` variant is added, which is returned
  when a parameter expansion has an invalid name.
- The `source::Source` enum is extended with new variants `VariableValue`,
  `InitFile`, and `Other`.

### Changed

- `syntax::Param` has been renamed to `syntax::BracedParam`.
- The `syntax::TextUnit::RawParam` variant now has a `param: syntax::Param`
  field instead of a `name: String` field.
- The `syntax::BracedParam` struct (formerly `syntax::Param`) now has a
  `param: syntax::Param` field instead of a `name: String` field.
- `source::Source::label` now returns `"<arithmetic_expansion>"` for
  `Source::Arith`. Previously, it returned `"<arith>"`.
- The `parser::lex::WordLexer::braced_param` method now returns
  `parser::SyntaxError::InvalidParam` if the parameter starts with a digit but
  contains a non-digit character.
- Public dependency versions:
    - Rust 1.77.0 → 1.79.0

## [0.10.0] - 2024-07-12

### Added

- `source::Code::line_number`
    - This new method returns the line number of a particular character in the
      code.
- `alias::Glossary`
    - This new trait is now used as an interface to provide the parser with
      alias definitions.
- `impl<T> input::Input for T where T: DerefMut<Target: input::Input>`
    - This new trait implementation allows more types to be used as input
      sources, especially when it is used with a decorator that requires
      another input source.
- `input::Context::is_first_line`
    - This new method allows changing the behavior of the input function
      depending on whether the current line is the first line of the input.
    - The corresponding setter method `set_is_first_line` is also added.

### Changed

- Public dependency versions:
    - Rust 1.70.0 → 1.77.0
    - annotate-snippets 0.10.0 → 0.11.4
- `source::Code::source` is now `Rc<Source>` instead of `Source`.
    - This change is made to avoid cloning the `Source` object when the `Lexer`
      flushes its buffer and creates a new `Code` object sharing the same
      `Source`.
    - The lexer constructor `Lexer::new` now takes  `Rc<Source>` instead of
      `Source`.
    - The lexer constructor `Lexer::from_memory` now takes a generic parameter
      that can be converted to `Rc<Source>`.
- The second argument of `parser::Parser::new` is now `&dyn alias::Glossary`
  instead of `&alias::AliasSet`.

### Fixed

- Small performance improvements

## [0.9.0] - 2024-06-09

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
- Public dependency versions
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
- Public dependency versions
    - Rust 1.58.0 → 1.67.0
- Private dependency versions
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
- Private dependency versions
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
- Private dependency versions
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
- Private dependency versions
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

[0.16.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.16.0
[0.15.2]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.15.2
[0.15.1]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.15.1
[0.15.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.15.0
[0.14.1]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.14.1
[0.14.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.14.0
[0.13.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.13.0
[0.12.1]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.12.1
[0.12.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.12.0
[0.11.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.11.0
[0.10.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.10.0
[0.9.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.9.0
[0.8.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.8.0
[0.7.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.7.0
[0.6.1]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.6.1
[0.6.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.6.0
[0.5.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.5.0
[0.4.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.4.0
[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.1.0
