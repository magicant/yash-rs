---
name: commit
description: Implement and commit work in yash-rs incrementally — decompose into independent steps, then for each step code, run ./check.sh, and commit. Use as soon as a task will produce commits, not only when finalizing.
---

# Incremental Work-and-Commit Workflow for yash-rs

Use this skill to drive *how the work is done*, not just how it is recorded. The
core principle: each commit must be a tree state that independently passes
`./check.sh`. The reliable way to guarantee that is to commit each logical step
**as you finish it**, before starting the next one — not to pile up all the
changes and slice them apart at the end.

## Why incremental, not retroactive

If you do all the work first and then try to split the result into commits with
`git add -p`, each partial commit is an artificial slice of the tree that was
never actually checked. It may not compile, its tests may fail, and clippy may
warn — because `./check.sh` only ever saw the *combined* state. Committing each
step right after it passes `./check.sh` is the only way to keep every commit
green.

So: **prefer to plan the decomposition before you start coding**, and run the
loop below. Only fall back to retroactive splitting (see the Fallback section)
when changes already exist in the working tree.

## When to Use

- A task is assigned that will result in one or more commits — engage this skill
  *before* coding so you can structure the work into independent steps.
- You are addressing multiple code-review comments.
- A single prompt contains several distinct work instructions.

## Outcome

- Every commit independently passes `./check.sh` with zero errors and zero
  warnings.
- Each commit is logically atomic: one coherent change.
- Commit messages follow the project style (plain imperative subject, no
  `type:` prefixes).

## Step 1 — Decompose the task into independent steps

Before writing code, break the assignment into the smallest steps that are each:

- **Self-contained** — the step leaves the codebase in a working state where
  `./check.sh` passes on its own.
- **Single-purpose** — one concern per step.

Split into separate steps when the task involves distinct concerns, for example:

- Multiple independent code-review comments (one step per comment, or per
  logical group).
- A single prompt requesting several unrelated tasks (e.g., "fix the bug and
  update the docs").
- Changes in unrelated crates that need not ship together.
- A functional change plus a separate mechanical refactor or formatting fix.

If a step would leave the tree unable to pass `./check.sh` (e.g., adding an API
in one commit and its first caller in the next), either merge those steps or
order them so each boundary is still green. When the right decomposition is
unclear, ask the user before starting.

Keep the planned steps in a todo list so the loop below has a clear sequence.

## Step 2 — Per-step loop

For each step, in order:

1. **Implement only that step.**
   Make just the changes belonging to this one logical concern. Do not start
   the next step until this one is committed.

2. **Format the code.**

   ```bash
   cargo fmt
   ```

   Running this first guarantees the `cargo fmt --check` inside `./check.sh`
   below will not fail on formatting.

3. **Run the check.**

   ```bash
   ./check.sh
   ```

   It must exit 0 with no errors and no warnings. If it reports anything, fix it
   now — within this step — before committing. `./check.sh` runs
   `cargo fmt --check`, `cargo test`, `cargo clippy --all-targets`, and
   `cargo doc --no-deps`; a failure in any is a blocker.

4. **Stage this step's changes.**

   ```bash
   git add <files for this step>
   ```

   Because the working tree now contains only this step's changes, staging is
   straightforward. Avoid `git add -A`/`git add .` only if unrelated changes
   somehow remain in the tree.

5. **Write the commit message.**
   Follow the project convention — see
   [commit-message skill](../commit-message/SKILL.md):
   - Imperative subject, ≤72 characters, no `type:` prefix.
   - Optional body explaining motivation and observable impact.

6. **Commit.**

   ```bash
   git commit -m "$(cat <<'EOF'
   <subject>

   <optional body>
   EOF
   )"
   ```

7. **Verify it landed.**

   ```bash
   git show --stat HEAD
   ```

Then move to the next step and repeat. Each commit is created from a tree that
just passed `./check.sh`, so every commit is independently green.

## Step 3 — Report to the user

After all steps are committed, list each commit hash and subject so the user can
confirm the result.

## Fallback — committing changes that already exist

If you arrive with a working tree that already contains finished, unsplit
changes (e.g., you forgot to commit incrementally, or the user did the work):

1. Run `git diff` / `git status` to survey everything.
2. Run `./check.sh` once on the whole tree; it must pass.
3. Group the changes and commit each group with `git add <files>` (or
   `git add -p` for a file spanning multiple groups).
4. **Caveat:** intermediate commits made this way were never checked in
   isolation. If independent verifiability matters, re-checkout each commit and
   run `./check.sh`, or reorder so the green boundaries hold. Tell the user when
   you cannot guarantee a mid-history commit passes on its own.

This path is strictly inferior to the incremental loop — use it only when the
changes are already in the tree.

## Quality Checks

- `./check.sh` exits 0 immediately before every commit — no exceptions.
- No commit bundles unrelated changes unless the user explicitly asks to group
  them.
- No commit message uses marker prefixes (`fix:`, `feat:`, `chore:`, etc.)
  unless the user requests that style.
- No `.env` files, credential files, or large generated binaries are staged.

## Limits

- Do not amend a commit already pushed to a shared remote without explicit user
  approval.
- Do not use `--no-verify` to bypass hooks.
- Do not force-push to `main`/`master`.
