# Changelog

All notable changes to `yash-syntax` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - ????-??-??

### Added

- `Lexer::pending`
- `Lexer::flush`

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
- `Lexer::new` now requiring the `start_line_number` and `source` parameters
- Dependency versions
    - `async-trait` 0.1.50 → 0.1.52
    - `futures-util` 0.3.18 → 0.3.19
    - `itertools` 0.10.1 → 0.10.3

## [0.1.0] - 2021-12-11

### Added

- Functionalities to parse POSIX shell scripts
- Alias substitution support

[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-syntax-0.1.0
