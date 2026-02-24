# Async Conversion Plan for yash-rs (#697)

## Overview

This plan breaks the asyncification of trap/signal handling into manageable tasks. The fundamental principle: async functions can call sync functions, but **sync functions cannot call async functions**. Therefore, changes must flow from top-down, making callers async before callees.

## Dependency Order

```
TrapSet methods (must be async to call async set_disposition)
  ↓
SignalSystem::set_disposition (async)
        ↓
yash-builtin call sites (must be async to call async TrapSet methods)
        ↓
Version bumps & CHANGELOGs (documentation of completed work)
```

Validation is performed within each task; there is no separate validation-only task.

---

# Task 1: Make TrapSet Methods Async

**Goal:** Convert TrapSet public and internal methods to async. This must be done before making `SignalSystem::set_disposition` async, because TrapSet calls it.

**Files to modify:**
- yash-env/src/trap.rs (TrapSet impl and GrandState)
- yash-env/src/trap/state.rs (internal state management)
- yash-env/src/lib.rs (Env wrapper methods)
- yash-env/src/semantics/command.rs (command execution)

## Subtasks

### 1.1: Make TrapSet public methods async
**File:** yash-env/src/trap.rs (methods starting ~line 189)

Convert these methods to async (add `async` keyword, change return type):
- `pub async fn set_action(...) -> impl Future<Output = Result<(), SetActionError>> + use<Self, S>`
- `pub async fn enter_subshell(...)` (returns `()`, not Result - errors are ignored)
- `pub async fn enable_internal_disposition_for_sigchld(...) -> impl Future<Output = Result<(), Errno>> + use<Self, S>`
- `pub async fn enable_internal_dispositions_for_terminators(...) -> impl Future<Output = Result<(), Errno>> + use<Self, S>`
- `pub async fn enable_internal_dispositions_for_stoppers(...) -> impl Future<Output = Result<(), Errno>> + use<Self, S>`
- `pub async fn disable_internal_dispositions(...) -> impl Future<Output = Result<(), Errno>> + use<Self, S>`
- `pub async fn disable_internal_dispositions_for_terminators(...) -> impl Future<Output = Result<(), Errno>> + use<Self, S>`
- `pub async fn disable_internal_dispositions_for_stoppers(...) -> impl Future<Output = Result<(), Errno>> + use<Self, S>`

**Key points:**
- For each method, change signature to `pub async fn` pattern
- Update return type to include `+ use<Self, S>` lifetime bounds
- Await internal calls to `GrandState` methods that are now async

### 1.2: Make GrandState internal methods async
**File:** yash-env/src/trap/state.rs (methods starting ~line 214)

These are internal helpers called by TrapSet. Make them async:
- `pub async fn set_action(...) -> Result<()>`
- `pub async fn set_internal_disposition(...) -> Result<()>`
- `pub async fn enter_subshell(...) -> Result<()>`
- `pub async fn ignore(...) -> Result<()>`

Update all internal `.await` calls where these call `system.set_disposition()`.

### 1.3: Update Env wrapper methods
**File:** yash-env/src/lib.rs (~line 430-450)

Methods like `wait_for_subshell()` that call `traps.enable_internal_disposition_for_sigchld()`:
- Add `async` keyword
- Add `.await` to trap method calls
- Update callers if needed

### 1.4: Update command execution
**File:** yash-env/src/semantics/command.rs (~line 148)

Functions like `replace_current_process()` that call trap methods:
- Add `async` keyword
- Add `.await` to trap method calls
- May require making the function return `impl Future<...>`

### 1.5: Update all TrapSet tests in yash-env
**File:** yash-env/src/trap.rs (all test functions)

Ensure all test functions properly use `.now_or_never().unwrap().unwrap()` pattern for async calls with DummySystem.

### 1.6: Clippy attributes to remove

**IMPORTANT:** The following `#[allow(clippy::unused_async)]` attributes were added temporarily because Task 1 makes methods async before their internal calls become truly async. These MUST be removed when completing Task 2 (making SignalSystem::set_disposition async):

- `yash-env/src/trap.rs`: 
  - `TrapSet::set_action` (~line 189)
  - `TrapSet::enable_internal_disposition_for_sigchld` (~line 340)
  - `TrapSet::enable_internal_dispositions_for_terminators` (~line 370)
  - `TrapSet::enable_internal_dispositions_for_stoppers` (~line 412)
  - `TrapSet::disable_internal_dispositions_for_terminators` (~line 424)
  - `TrapSet::disable_internal_dispositions_for_stoppers` (~line 436)

- `yash-env/src/trap/state.rs`:
  - `GrandState::set_action` (~line 215)
  - `GrandState::set_internal_disposition` (~line 296)
  - `GrandState::enter_subshell` (~line 340)
  - `GrandState::ignore` (~line 399)

**Action item for Task 2:** Search for all `#[allow(clippy::unused_async)]` attributes in yash-env and remove them after SignalSystem methods are truly async.

### 1.7: Validation for Task 1

Run after completing all subtasks:
```bash
cargo check --package yash-env
cargo test --package yash-env
cargo clippy --package yash-env
```

Expected: yash-env tests pass, no compiler errors or clippy warnings.

**Continue to Task 2 only after validation passes.**

---

# Task 2: Make SignalSystem::set_disposition Async

**Status: COMPLETED** (implemented in this branch)

**Goal:** Convert the `SignalSystem` trait's `set_disposition` method to return a Future. This happens after TrapSet is async.

**Files modified:**
- yash-env/src/trap.rs (trait definition, DummySystem test impl)
- yash-env/src/system/shared.rs (SharedSystem impl, test updates)
- yash-env/src/trap/state.rs (GrandState methods, UnusedSystem test impl)

## Subtasks

### 2.1: Update SignalSystem trait definition
**File:** yash-env/src/trap.rs (~line 53)

Change signature from:
```rust
fn set_disposition(&mut self, signal: Number, disposition: Disposition) -> Result<Disposition, Errno>;
```

To:
```rust
fn set_disposition(&self, signal: Number, disposition: Disposition) 
  -> impl Future<Output = Result<Disposition, Errno>> + use<Self>;
```

**Key points:**
- Change `&mut self` to `&self` (interior mutability will handle mutation)
- Return type uses `impl Trait` with `Future` (Rust 2024 prelude provides Future)
- Use `+ use<Self>` lifetime bound to specify captured types

### 2.2: Update SharedSystem<S> implementation
**File:** yash-env/src/system/shared.rs (~line 774)

Implement the new async signature:
```rust
fn set_disposition(&self, signal: signal::Number, disposition: Disposition) 
  -> impl Future<Output = Result<Disposition, Errno>> + use<S> {
  std::future::ready(self.0.borrow_mut().set_disposition(signal, disposition))
}
```

**Key points:**
- Use `std::future::ready()` to wrap the synchronous `SharedSystem` operation
- Call `self.0.borrow_mut()` since SharedSystem wraps interior-mutable state
- Returns a ready future (non-blocking, executes immediately)

### 2.3: Update SelectSystem<S> implementation  
**File:** yash-env/src/system/select.rs

Find the `impl SignalSystem for SelectSystem<S>` block and apply the same pattern as SharedSystem.

### 2.4: Update test implementations in trap.rs

**DummySystem struct (~line 540)**
- Wrap HashMap in `RefCell<HashMap<...>>` for interior mutability
- Implement new `set_disposition(&self, ...)` signature

**UnusedSystem struct (~line 700)**
- Update signature to match trait
- Add lifetime parameter: `fn set_disposition(&self, ...) -> impl Future<Output = ...> + use<Self>`

### 2.5: Fix all test functions in yash-env/src/trap.rs

Test functions calling `system.set_disposition()` or wrapper methods need adjustment:
- Replace calls with `.await` pattern or `.now_or_never().unwrap()` for sync tests
- Pattern for sync tests (using `DummySystem`):
  ```rust
  trap_set.set_action(&mut system, signal, action, origin, false)
    .now_or_never().unwrap().unwrap()
    // ^ unwrap the Future   ^ unwrap the Result
  ```
- Pattern for async functions: use `.await?`

**Note:** Verify pattern is `.now_or_never().unwrap().unwrap()` (two unwraps, not four).

### 2.6: Validation for Task 2

Run after completing all subtasks:
```bash
cargo check --package yash-env
cargo test --package yash-env
cargo clippy --package yash-env
```

Expected: All yash-env tests pass, no compilation errors.

**Continue to Task 3 only after validation passes.**

---

# Task 3: Update yash-builtin Async Callers

**Goal:** Update yash-builtin command implementations to handle async TrapSet methods. This is the largest scope task as it affects multiple builtins.

**Files to modify:**
- yash-builtin/src/trap.rs (trap builtin implementation)
- yash-builtin/src/wait/core.rs (wait builtin)
- yash-builtin/src/set.rs (set builtin)
- Any other files with trap method calls

## Subtasks

### 3.1: Update trap builtin
**File:** yash-builtin/src/trap.rs (~line 210)

The `set_action()` helper function that wraps `traps.set_action()`:
- Make function `async`
- Add `.await` to `traps.set_action()` call
- Chain `.map_err()` properly: `traps.set_action(...).await.map_err(...)`

All test functions that call `main()` use `.now_or_never().unwrap()` pattern (already correct from previous work).

### 3.2: Update wait builtin
**File:** yash-builtin/src/wait/core.rs (~line 75)

The `wait_for_any_job_or_trap()` async function:
- Add `.await` to `enable_internal_disposition_for_sigchld()` call
- Change from `env.traps...?` to `env.traps....await?`

Any tests calling `wait_for_any_job_or_trap()` may already be async-compatible if using `in_virtual_system()`.

### 3.3: Update set builtin
**File:** yash-builtin/src/set.rs (~line 78)

Two related functions need updating:

1. **`update_internal_dispositions_for_stoppers()` function**
   - Change from regular `fn` to `async fn`
   - Add `.await` to both branches where calling trap methods
   - Problem: if/else branches returning different Future opaque types
   - Solution: Either:
     - Use `Box<dyn Future<...>>` to box both branches
     - Or restructure to avoid divergent Future types

2. **`modify()` function (already async)**
   - Calling the now-async `update_internal_dispositions_for_stoppers()`
   - Change from `update_internal_dispositions_for_stoppers(env);` 
   - To: `update_internal_dispositions_for_stoppers(env).await;`

### 3.4: Search for all other trap method calls
**Command:** 
```bash
grep -r "\.enable_internal\|\.disable_internal\|\.enter_subshell\|\.set_action" \
  yash-builtin/src/ --include="*.rs" | grep -v "async"
```

For each match:
- Determine if in async context
- Add `.await` if in async function
- If in sync function, either:
  - Make the function async
  - Create async wrapper
  - Use appropriate test helper if in tests

### 3.5: Update yash-builtin tests
**Pattern:** Tests in yash-builtin already use `in_virtual_system()` helper or `.now_or_never().unwrap()` pattern. Verify these still work with updated async signatures.

### 3.6: Validation for Task 3

Run after all subtasks:
```bash
cargo check --package yash-builtin
cargo test --package yash-builtin
cargo clippy --package yash-builtin
```

Also check the main CLI integration:
```bash
cargo check --package yash-cli
cargo test --package yash-cli
```

Expected: No compiler errors, all builtin tests pass.

**Continue to Task 4 only after validation passes.**

---

# Task 4: Version Bumps and Changelog Updates

**Goal:** Document the API changes in version numbers and changelog files.

**Files to modify:**
- yash-env/Cargo.toml
- yash-env/CHANGELOG.md
- yash-builtin/Cargo.toml
- yash-builtin/CHANGELOG.md
- Cargo.toml (root workspace)

## Subtasks

### 4.1: Determine version bumps

Based on semantic versioning:
- **yash-env**: Public API changed (methods now async) → **Minor bump**
  - Example: 0.13.0 → 0.14.0
- **yash-builtin**: Depends on yash-env changes → **Minor bump**
  - Example: 0.16.0 → 0.17.0
- **yash-cli**: May depend on yash-builtin changes → Check if observable behavior changed
  - If only internal async changes: **Patch bump** 
  - If observable behavior changed: **Minor bump**

### 4.2: Update Cargo.toml versions

**yash-env/Cargo.toml**
```toml
[package]
version = "0.14.0"  # Changed from 0.13.0
```

**yash-builtin/Cargo.toml**
```toml
[package]
version = "0.17.0"  # Changed from 0.16.0

[dependencies]
yash-env = { workspace = true }  # Automatically uses workspace version
```

**Cargo.toml (root)**
```toml
[workspace.package]
yash-env = "0.14.0"
yash-builtin = "0.17.0"
# ... update version specs for any other affected crates
```

### 4.3: Update CHANGELOG.md files

**yash-env/CHANGELOG.md**

Add at the top (before any existing Unreleased section):
```markdown
## [0.14.0] - Unreleased

### Changed
- `SignalSystem::set_disposition()` is now an async method returning `impl Future<...>`
- `TrapSet::set_action()` is now async
- `TrapSet::enter_subshell()` is now async
- `TrapSet::enable_internal_disposition_for_sigchld()` is now async
- `TrapSet::enable_internal_dispositions_for_terminators()` is now async
- `TrapSet::enable_internal_dispositions_for_stoppers()` is now async
- `TrapSet::disable_internal_dispositions()` is now async
- `TrapSet::disable_internal_dispositions_for_terminators()` is now async
- `TrapSet::disable_internal_dispositions_for_stoppers()` is now async
- `Env::wait_for_subshell()` is now async
- `GrandState` internal methods are now async (internal API)
```

**yash-builtin/CHANGELOG.md**

Add at the top:
```markdown
## [0.17.0] - Unreleased

### Changed
- Updated to yash-env 0.14.0 with async trap API changes
- `trap` builtin internal helper functions updated to handle async trap operations
- `wait` builtin updated to handle async signal disposition setup
- `set` builtin updated to handle async internal disposition management
```

### 4.4: Final validation of entire workspace

Run comprehensive checks:
```bash
cargo fmt --all --check
cargo test --all
cargo clippy --all-targets --all-features
./check.sh -v
```

Expected: All tests pass, no formatting issues, no clippy warnings.

---

## Summary of Execution Flow

1. **Task 1**: Make `TrapSet` methods async
  - Validate with `cargo test --package yash-env`
   
2. **Task 2**: Make `SignalSystem::set_disposition` async (depends on Task 1)
  - Validate with `cargo test --package yash-env`
   
3. **Task 3**: Make yash-builtin callers async (depends on Task 2)
  - Validate with `cargo test --all`
   
4. **Task 4**: Version bumps and changelog updates
  - Validate with `./check.sh -v`

The key principle: **Never move to the next task until the current task passes all validations.**

---

## Key Points for Implementation

### Async Function Return Types
```rust
// In trait definitions
fn method(&self, ...) -> impl Future<Output = Result<T, E>> + use<Self>;

// In sync struct implementations (use ready())
std::future::ready(value)

// In async method implementations
// Just return the value, async fn wraps it automatically
```

### Test Patterns
```rust
// Synchronous test with async function (using .now_or_never())
#[test]
fn test_something() {
    let result = async_fn()
        .now_or_never().unwrap()  // Extract from Option<T>
        .unwrap();                 // Extract from Result<T, E>
}

// Async test using in_virtual_system()
#[test]
fn test_something_async() {
    in_virtual_system(|env, _| async {
        let result = env.traps.set_action(...).await?;
        // ... assertions
        Ok(())
    });
}
```

### Avoiding the if/else Future Type Problem
When both branches of if/else return different `impl Trait` types, use boxing:
```rust
// Problem: Opaque types don't match
let fut = if condition {
    method_a(...)  // impl Future<A>
} else {
    method_b(...)  // impl Future<B>
};

// Solution: Box both branches
let fut: Box<dyn Future<Output = Result<(), E>>> = if condition {
    Box::new(method_a(...))
} else {
    Box::new(method_b(...))
};
```

Or restructure to avoid the divergence (preferred):
```rust
let result = if condition {
    method_a(...).await
} else {
    method_b(...).await
}?;
```

