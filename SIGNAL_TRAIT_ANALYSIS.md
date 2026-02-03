# Analysis: Signal Trait Bound Requirements

## Objective

Find all functions that require the `yash_env::trap::SignalSystem` trait as a type parameter bound. For each function that only depends on methods from the `yash_env::system::Signals` trait or other signal-related traits from `yash_env::system`, replace the bound with that trait to minimize bound requirements.

## Trait Hierarchy

### Signal-Related Traits in `yash_env::system`

1. **`Signals`** (base trait)
   - Provides signal constants (`SIGINT`, `SIGTERM`, etc.)
   - Provides signal name/number conversions (`sig2str`, `str2sig`, etc.)
   - No direct system interaction

2. **`GetSigaction: Signals`**
   - Provides: `get_sigaction(&self, signal) -> Result<Disposition>`
   - Low-level wrapper around `sigaction(2)` system call
   - Gets the current disposition for a signal

3. **`Sigaction: GetSigaction`**
   - Provides: `sigaction(&self, signal, action) -> Result<Disposition>`
   - Low-level wrapper around `sigaction(2)` system call  
   - Sets disposition and returns previous one
   - Takes `&self` (immutable reference)

4. **`Sigmask: Signals`**
   - Provides: `sigmask(&self, op, signal) -> Result<()>`
   - Manages signal blocking masks
   - Wrapper around `sigprocmask(2)` system call

### SignalSystem Trait (in `yash_env::trap`)

**`SignalSystem: Signals`**
- Provides: `get_disposition(&self, signal) -> Result<Disposition, Errno>`
- Provides: `set_disposition(&mut self, signal, disposition) -> Result<Disposition, Errno>`
- High-level interface for trap management
- Implemented for `SharedSystem<S> where S: Signals + Sigmask + Sigaction`
- Takes `&mut self` for `set_disposition` because it modifies signal masks internally

### Key Implementation Detail

`SignalSystem` methods are implemented using `Sigaction` + `Sigmask`:

```rust
// In SelectSystem:
pub fn get_disposition(&self, signal: Number) -> Result<Disposition>
where
    S: Sigaction,
{
    self.system.get_sigaction(signal)
}

pub fn set_disposition(&mut self, signal: Number, handling: Disposition) -> Result<Disposition>
where
    S: Sigaction + Sigmask,
{
    match handling {
        Disposition::Default | Disposition::Ignore => {
            let old = self.system.sigaction(signal, handling)?;
            self.sigmask(SigmaskOp::Remove, signal)?;  // Unblock signal
            Ok(old)
        }
        Disposition::Catch => {
            self.sigmask(SigmaskOp::Add, signal)?;  // Block signal
            self.system.sigaction(signal, handling)
        }
    }
}
```

## Functions with SignalSystem Bounds

### yash-env/src/trap.rs - TrapSet Methods

All public methods with `SignalSystem` bounds:

| Function | Direct Usage | Transitive Dependency |
|----------|--------------|----------------------|
| `peek_state` | Calls `GrandState::insert_from_system_if_vacant` | ⚠️ Needs `get_disposition` |
| `set_action` | Uses `S::SIGKILL`, `S::SIGSTOP` constants | ⚠️ Calls `GrandState::set_action` which needs `set_disposition` |
| `enter_subshell` | Uses signal constants | ⚠️ Calls `GrandState::enter_subshell` and `GrandState::ignore` which need `set_disposition` |
| `enable_internal_disposition_for_sigchld` | Uses `S::SIGCHLD` | ⚠️ Calls `set_internal_disposition` |
| `enable_internal_dispositions_for_terminators` | Uses `S::SIGINT`, `S::SIGTERM`, `S::SIGQUIT` | ⚠️ Calls `set_internal_disposition` |
| `enable_internal_dispositions_for_stoppers` | Uses `S::SIGTSTP`, `S::SIGTTIN`, `S::SIGTTOU` | ⚠️ Calls `set_internal_disposition` |
| `disable_internal_dispositions_for_terminators` | Uses signal constants | ⚠️ Calls `set_internal_disposition` |
| `disable_internal_dispositions_for_stoppers` | Uses signal constants | ⚠️ Calls `set_internal_disposition` |
| `disable_internal_dispositions` | Uses `S::SIGCHLD` | ⚠️ Calls helper functions |

### yash-env/src/trap/state.rs - GrandState Methods

All methods with `SignalSystem` bounds:

| Function | Direct System Calls |
|----------|---------------------|
| `insert_from_system_if_vacant` | ✅ `system.get_disposition(signal)` |
| `set_action` | ✅ `system.set_disposition(signal, disposition)` |
| `set_internal_disposition` | ✅ `system.set_disposition(signal, disposition)` |
| `enter_subshell` | ✅ `system.set_disposition(signal, new_disposition)` |
| `ignore` | ✅ `system.set_disposition(signal, Disposition::Ignore)` |

### yash-builtin/src/trap.rs - Display Functions

| Function | Direct Usage | Transitive Dependency |
|----------|--------------|----------------------|
| `display_trap` | Uses `cond.to_string(system)` (needs `Signals`) | ⚠️ Calls `traps.peek_state(system, cond)` which needs `SignalSystem` |
| `display_traps` | None | ⚠️ Calls `display_all_traps` |
| `display_all_traps` | Uses `Condition::iter(system)` and signal constants (needs `Signals`) | ⚠️ Calls `display_trap` |

## Analysis: Can Bounds Be Relaxed?

### Option 1: Use `Signals` Instead of `SignalSystem`

**Not viable** because:
- All `GrandState` methods directly call `get_disposition` or `set_disposition`
- These methods are **exclusively** defined in `SignalSystem`, not in `Signals`
- Display functions need to query system for initially-ignored signals via `peek_state`

### Option 2: Use `Signals + GetSigaction + Sigaction` Instead of `SignalSystem`

**Technically possible but not recommended** because:

1. **Mutable reference requirement**: `SignalSystem::set_disposition` requires `&mut self` because it modifies signal masks. The trap functions pass `&mut system`, which would need refactoring.

2. **Loss of semantics**: `SignalSystem` is a high-level, domain-specific trait designed for trap management. Replacing it with lower-level traits loses this semantic clarity.

3. **No practical benefit**: The functions using `SignalSystem` genuinely need both disposition management AND signal masking. Using the lower-level traits directly wouldn't reduce the actual requirements.

4. **Warning in documentation**: The `GetSigaction` and `Sigaction` trait documentation explicitly warns:
   > "This is a low-level function used internally by `SharedSystem`. You should not call this function directly, or you will leave the `SharedSystem` instance in an inconsistent state."

### Option 3: Refactor Display Functions

**Attempted but not viable** because:
- Display functions need to show initially-ignored signals (e.g., signals set to `Ignore` before shell startup)
- To detect these, they must query the system via `get_disposition`
- Using only `get_state` (which doesn't query the system) causes test failures

## Conclusion

**No functions can have their `SignalSystem` bounds relaxed.**

Every function with a `SignalSystem` bound either:
1. Directly calls `get_disposition` or `set_disposition`, OR
2. Calls other functions that do so transitively

The `SignalSystem` trait exists specifically to provide a clean, high-level interface for trap management, combining:
- Signal constants and conversions (from `Signals`)
- Signal disposition management (from `Sigaction`)
- Signal mask management (from `Sigmask`)

The current design is **already minimal and appropriate**. Each bound is necessary for the function's implementation.

## Recommendations

1. **No code changes needed**: The trait bounds are already optimal.

2. **Documentation could be enhanced**: Consider adding documentation to `SignalSystem` explaining why it exists as a separate trait and when to use it vs. the lower-level `Sigaction` trait.

3. **Future consideration**: If new trap-related functions are added that truly only need signal name conversion (just `Signals`), they should use `Signals` as the bound. However, no such functions currently exist.

## Test Results

All tests pass with the current implementation:
- `cargo fmt -- --check` ✅
- `cargo test -- --quiet` ✅  
- `cargo doc` ✅
- `cargo clippy --all-targets` ✅
