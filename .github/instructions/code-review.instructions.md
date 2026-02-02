---
applyTo: "**"
excludeAgent: "coding-agent"
---

# Yash-rs Code Review Instructions

## Review Objectives

Review Rust code changes comprehensively from every angle, focusing on issues that the automated tools (compiler, formatter, clippy) would NOT catch. Your role is to provide high-value human-centric feedback.

## What NOT to Review

**DO NOT** comment on issues that will be caught by automated tools:
- Formatting issues (handled by `cargo fmt`)
- Common lints and warnings (handled by `cargo clippy`)
- Compilation errors (handled by `rustc`)
- Unused imports, variables, or functions (compiler warnings)
- Basic syntax errors

## What TO Review

### 1. Architecture and Design
- **API Design**: Is the public API intuitive, consistent, and well-designed?
- **Abstraction Levels**: Are abstractions at appropriate levels? Not too abstract or too concrete?
- **Separation of Concerns**: Are responsibilities properly separated?
- **POSIX Compliance**: Does the implementation follow POSIX shell semantics correctly?
- **Design Patterns**: Are appropriate patterns used? Are there better alternatives?

### 2. Correctness and Logic
- **Edge Cases**: Are all edge cases handled (empty strings, null bytes, special characters, boundary conditions)?
- **Error Handling**: Are errors handled appropriately? Are error messages helpful?
- **Concurrency**: Are there potential race conditions, deadlocks, or data races not caught by the type system?
- **Shell Semantics**: Does the behavior match shell specification and expectations?
- **Parser Correctness**: For syntax changes, are all grammar rules and precedence correct?

### 3. Performance and Efficiency
- **Algorithmic Complexity**: Are algorithms efficient? Any unnecessary O(n²) operations?
- **Memory Usage**: Excessive allocations, clones, or memory leaks?
- **I/O Efficiency**: Are file and stream operations optimized?
- **Caching Opportunities**: Are there repeated computations that could be cached?

### 4. Security
- **Input Validation**: Are all inputs properly validated and sanitized?
- **Path Traversal**: Are file paths properly validated to prevent directory traversal attacks?
- **Command Injection**: Are shell commands properly escaped and validated?
- **Resource Limits**: Are there protections against resource exhaustion (memory, CPU, file descriptors)?
- **Signal Safety**: Are signal handlers async-signal-safe?

### 5. Testing and Testability
- **Test Coverage**: Are new features and bug fixes covered by tests?
- **Test Quality**: Are tests meaningful and not just checking trivial behavior?
- **Test Scenarios**: Are both positive and negative test cases included?
- **Integration Tests**: For shell behavior changes, are scripted tests in `yash-cli/tests/scripted_test/` updated?

### 6. Documentation
- **Public API Documentation**: Are all public functions, types, and modules properly documented?
- **Documentation Quality**: Are docs clear, complete, and include examples where appropriate?
- **Edge Case Documentation**: Are important edge cases and limitations documented?
- **CHANGELOG**: Are user-facing changes documented in the appropriate CHANGELOG.md?
- **Version Documentation**: For new features, is the version number mentioned in docs?

### 7. Maintainability
- **Code Clarity**: Is the code easy to understand? Are complex parts explained with comments?
- **Naming**: Are names descriptive and follow Rust conventions?
- **Code Duplication**: Is there duplicated logic that should be extracted?
- **Future-Proofing**: Will this code be easy to extend or modify in the future?

### 8. Compatibility and Portability
- **Platform Compatibility**: Does the code work on all target platforms (Linux, macOS, Windows where applicable)?
- **MSRV Compatibility**: Does the code use features available in Rust 1.87.0?
- **Dependency Compatibility**: Are new dependencies justified and compatible with existing ones?

### 9. Version Management (CRITICAL for yash-rs)
- **Version Bumping**: Is the version bumped appropriately in affected crates?
  - Patch: Bug fixes, internal changes
  - Minor: New features, non-breaking API additions
  - Major: Breaking changes
- **Avoid Double Bumps**: If a version was already bumped in a previous unreleased PR, is it bumped again incorrectly?
- **Workspace Dependencies**: Are workspace dependency versions updated in root `Cargo.toml`?

### 10. Yash-rs Specific Concerns
- **POSIX Compliance**: Shell behavior must match POSIX specifications
- **Backward Compatibility**: Changes to yash-cli should maintain backward compatibility when possible
- **License Compliance**: MIT/Apache-2.0 crates should not depend on GPL crates
- **Feature Flags**: Are conditional features handled correctly?
- **Signal Handling**: Shell signal handling is complex - verify correctness
- **Job Control**: Terminal and job control code requires special attention

## Review Process

1. **Understand the Change**: What problem is being solved? What is the approach?
2. **Check Scope**: Are changes minimal and focused? Is the PR trying to do too much?
3. **Verify Tests**: Do tests validate the fix or feature? Are they comprehensive?
4. **Consider Alternatives**: Could this be implemented in a better way?
5. **Think Like a User**: How will this affect users? Is it intuitive?
6. **Think Like a Maintainer**: Will this be easy to maintain and extend?

## Providing Feedback

- **Be Specific**: Point to exact lines or sections
- **Explain Why**: Don't just say something is wrong, explain why and suggest alternatives
- **Be Constructive**: Focus on improving the code, not criticizing the author
- **Prioritize**: Distinguish between critical issues, suggestions, and nitpicks
- **Acknowledge Good Work**: Point out well-done aspects of the PR

## Special Focus Areas

### Parser Changes (`yash-syntax`)
- Grammar correctness and completeness
- Error recovery and reporting
- Performance with large inputs
- Unicode and special character handling

### Executor Changes (`yash-executor`)
- Signal handling correctness
- File descriptor management
- Process lifecycle management
- Resource cleanup on errors

### Builtin Changes (`yash-builtin`)
- POSIX compliance
- Error messages and exit codes
- Interaction with shell state
- Option parsing correctness

### Environment Changes (`yash-env`)
- State management correctness
- Thread safety considerations
- Memory management
- API consistency
