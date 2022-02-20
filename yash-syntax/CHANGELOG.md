# Changelog

All notable changes to `yash-syntax` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - ????-??-??

### Added

- `parser::Lexer::location_range`

### Changed

- `source::Location::index: usize` replaced with `range: Range<usize>`

### Removed

- `source::Location::advance`

## [0.3.0] - 2022-02-06

This version simplifies type definitions for the abstract syntax tree (AST);
`syntax::HereDoc::content` is now wrapped in `RefCell` to remove generic type
parameters from `RedirBody` and other AST types.

### Changed

- `source::Source` now `non_exhaustive`
- `syntax::HereDoc::content` redefined as `RefCell<Text>` (previously `Text`)
- `impl From<HereDoc> for RedirBody` replaced with `impl<T: Into<Rc<HereDoc>>> From<T> for RedirBody`
- `parser::Lexer::here_doc_content` now taking a `&HereDoc` parameter and returning `Result<()>`
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

[0.3.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.3.0
[0.2.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.2.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.1.0
