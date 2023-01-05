# Changelog

All notable changes to `yash-quote` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] - 2023-01-05

### Fixed

- `yash-quote` now quotes strings containing `:~` so that the results can safely
  be used in the value of an assignment.

## [1.0.0] - 2022-02-03

No changes. Just bumping the version number to make the public API stable.

## [0.1.0] - 2021-12-11

### Added

- The `quote` function

[1.0.1]: https://github.com/magicant/yash-rs/releases/tag/yash-quote-1.0.1
[1.0.0]: https://github.com/magicant/yash-rs/releases/tag/yash-quote-1.0.0
[0.1.0]: https://github.com/magicant/yash-rs/releases/tag/yash-quote-0.1.0
