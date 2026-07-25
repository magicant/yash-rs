---
name: unit-tests
description: 'Design unit tests for yash-rs crates: organization, naming, Arrange-Act-Assert structure, imports, and VirtualSystem-based test doubles. Use when writing, adding, reviewing, or restructuring unit tests in any yash-* crate, and when writing the test in the test-first step of Canon TDD.'
argument-hint: 'Which crate/module and what behavior needs a unit test?'
---

# Unit Tests

Design unit tests as follows unless you are instructed otherwise:

## Organization

- Unit tests should be written in the same file as the function they test, in a `#[cfg(test)]` module named `tests`, unless existing tests are already organized otherwise.
- Unit tests should be sorted in the order of the functions they test.
- Unit tests for the same function should be sorted so that the most basic tests come first, followed by more complex or minor cases.

## Naming

- Unit test function names should describe the behavior being tested while omitting verbose words.
  - Prefer: `open_returns_enoent_when_file_is_missing`
  - Avoid: `test_that_the_open_function_returns_enoent_when_the_file_is_missing`

## Test implementation

- Each unit test should be self-contained and independent of other tests.
- Each unit test should follow the Arrange-Act-Assert pattern.
- The values used to specify the pre- and post-conditions of the behavior being tested should be explicitly specified as literals or named constants directly in the unit test function.
  - Do not rely on a helper function to produce a value that is significant to the behavior being tested. Exceptions: the helper function is clearly named to indicate the value it produces, or the unit test function passes the value to the helper function as an argument.
  - Do not compute the expected values in the unit test function; write the final expected values directly. A computed expectation duplicates the implementation's logic, so the test passes even when both are wrong.
    - Prefer: `assert_eq!(result, "foobar")`
    - Avoid: `assert_eq!(result, format!("{prefix}bar"))`

### Test doubles for `yash_env::system::*` traits

Creating an `Env` instance requires a `yash_env::system::*` trait implementation, which should be prepared as follows:

- Never use `yash_env::system::real::RealSystem` in unit tests, unless you are testing the behavior of `RealSystem` itself and the test has no side effects.
- Use `yash_env::system::r#virtual::VirtualSystem`, `Rc<Concurrent<VirtualSystem>>`, or other `VirtualSystem`-based test doubles for unit tests that require an `Env` instance.
  - However, you can define and employ a test double dedicated to specific units tests if it is shorter than using `VirtualSystem`.
  - Prefer a simple `VirtualSystem` over a more complex test double like `Rc<Concurrent<VirtualSystem>>` if possible.
  - Tip: An `Env<Rc<Concurrent<VirtualSystem>>>` can be created simply by calling `Env::new_virtual()`. However, if you need to modify the `VirtualSystem` before putting it into an `Env`, instantiate a `VirtualSystem` first, modify it, and then wrap it in an `Rc<Concurrent<_>>` before passing it to `Env::with_system`.

### Test helpers about `yash_env` items

- Consider using items in `yash_env::test_helper` for unit tests that involve `yash_env` items. Particularly, `in_virtual_system`, `assert_stdout`, and `assert_stderr` are often useful for tests that involve `VirtualSystem`.
- The `yash_env::test_helper` module is available only when the `test-helper` feature is enabled. If you want to use it in a crate that does not have `yash-env` as a dev-dependency or the `test-helper` feature enabled, ask the user what they want to do rather than adding a dev-dependency and enabling the feature without consent.

### Testing asynchronous code

- If applicable, use the `now_or_never` method from the `futures_util::future::FutureExt` trait to drive a `Future` to completion in a unit test. This is often applicable when there is only one asynchronous task involved in the unit test.
- If multiple asynchronous tasks are involved, consider using `yash_env::test_helper::in_virtual_system` to run a main asynchronous task to completion while allowing other asynchronous tasks to progress concurrently.
- If `in_virtual_system` is not applicable, manually set up a `yash_executor::Executor` and drive the `Future` to completion using its methods. You might want to see the implementation of `in_virtual_system` for an example of how to do this.
- Avoid using `futures_executor::block_on` in unit tests, though it may be found in existing unit tests.

## Imports

- Imports for unit tests should be placed at the top of the `tests` module, not at the top of the file or in each unit test function.
  - Exception: In case of name conflicts, imports can be placed in the unit test function.
- The `tests` module should import `super::*` to access items from the outer module.
- Items that are already in scope should not be re-imported, particularly items from the outer module.
