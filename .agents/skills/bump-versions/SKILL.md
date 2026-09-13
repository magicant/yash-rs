---
name: bump-versions
description: 'Apply the yash-rs versioning rules: bump affected crate versions, sync the root Cargo.toml, and add Unreleased changelog headings. Use proactively, before committing, for every code change to any yash-* crate in this workspace — not just when the user asks about versioning. Documentation updates are out of scope (see the update-docs skill).'
argument-hint: 'Which crates/behavior changed in this commit?'
---

# Bump Versions and Sync Release Metadata

This skill is the **canonical procedure** for the workspace's version- and
changelog-related rules. The
[PR template](../../../.github/pull_request_template.md) deliberately omits these
details so external contributors are not burdened with them; for such
contributions, the maintainer applies them at review time using this skill.

Use this skill for each code change **before it is committed**, so that the
release metadata affected by the change — crate version numbers, the root
`Cargo.toml` dependency versions, and each affected `CHANGELOG.md` — lands in
the **same commit** as the code change (see the commit-history rules in
[AGENTS.md](../../../AGENTS.md)). Do not defer the metadata to a single
catch-up pass after all of a PR's code changes are done; apply the skill once
per commit, to that commit's change.

**Scope.** This skill owns the version-number and changelog-heading rules and the
root `Cargo.toml` sync. Related concerns are delegated:

- For the changelog *wording and categorization* details, defer to the
  [update-changelog skill](../update-changelog/SKILL.md).
- **Documentation under `docs/src` is out of scope.** When a user-visible change
  also needs documentation, handle that separately with the
  [update-docs skill](../update-docs/SKILL.md). This skill does not edit docs.

## When to Use

- A code change is about to be committed and its version/changelog metadata
  must be included in the same commit.
- The user asks to bump versions, sync `Cargo.toml`, or add changelog entries
  for a change.
- A behavior or public-API change needs version + changelog updates.

## Inputs to gather first

1. **The diff of the change going into the commit at hand** (`git diff`,
   staged or working-tree as appropriate). Only when explicitly asked to catch
   up a whole branch at once, use the diff against the merge base instead.
2. **Which crates are touched**, directly or transitively.
3. **The kind of change per crate** (patch / minor / major — see `CLASSIFY`).

## How to read this skill

The procedure below is written as an algorithm. **Run it as written**: execute
`MAIN` top to bottom, call each subroutine where `MAIN` calls it, and do not
substitute a one-pass reading of the diff for the fan-out loop in Phase 2. The
most common failure of this skill is a *missed* bump, and it always has the same
cause: a crate that became affected only because *another* crate's version
changed, so it never appeared in the diff you started from. Phase 2 exists
solely to find those crates, and it is a loop, not a single pass.

Severity ordering used throughout:

```
BREAKING  >  COMPATIBLE  >  PATCH  >  NONE
```

`ASK_MAINTAINER(question)` is a blocking primitive: put the question to the
maintainer and wait for the answer. Each call site states the default to take if
the answer is unconfirmed; where no default is stated, **stop** rather than
guess.

## Procedure

### State

Maintain these tables explicitly (write them down; `VALIDATE` checks them):

```
severity   : crate -> BREAKING | COMPATIBLE | PATCH    # highest accumulated this cycle
version    : crate -> forecast version string          # output of FORECAST
dep_events : crate -> set of workspace crates whose new version it must record
             # an unlisted crate reads as the EMPTY SET, never as undefined
worklist   : queue of crates whose severity rose and whose dependents are not yet fanned out
root_before: crate -> its root requirement as it stood before Phase 3
root_raised: crate -> whether Phase 3 raised that requirement
log        : append-only list of (crate, severity, reason)   # the audit trail
```

`dep_events` is deliberately separate from `severity`: a dependency bump that a
crate must *record in its changelog* is not the same event as a bump that raises
its *severity*, and a crate already at the propagated severity still has to
record the dependency. `dep_events` stores crate names, not version strings, so a
later re-raise of an upstream crate cannot leave a stale version behind —
`WRITE_CHANGELOG` reads `version[X]` when it writes, after the fixpoint.

### MAIN

```
MAIN(diff):
    severity := {} ; version := {} ; dep_events := {} ; worklist := []
    root_before := {} ; root_raised := {} ; log := []

    # ---- Phase 1: seed from the diff --------------------------------------
    for each crate C that has a changed file in diff:
        RAISE(C, CLASSIFY(C, diff), "own source/manifest changed")

    if diff changes shell-observable behavior (in any crate whatsoever):
        RAISE("yash-cli", CLASSIFY("yash-cli", diff),
              "shell behavior changed")            # even if yash-cli itself was untouched

    # ---- Phase 2: fan out to dependents until the tables stop changing ----
    while worklist is not empty:
        X := pop(worklist)
        version[X] := FORECAST(X, severity[X])
        for each D in DEPENDENTS(X):               # ALWAYS recompute; never trust the diff
            dep_events[D] += X                     # record FIRST: RAISE may dedup below,
                                                   # but D must record X either way
            RAISE(D, PROPAGATED_SEVERITY(X, D), "records new " + X)
    # Loop invariant: on exit, every dependent of every bumped crate is itself
    # in `severity` (or was returned NONE by PROPAGATED_SEVERITY). Phase 2 ends
    # only at that fixpoint — a single pass over the diff never reaches it,
    # because a crate bumped in Phase 2 has dependents of its own.

    # ---- Phase 3: write the files ----------------------------------------
    for each crate C in severity:
        set `version` in C/Cargo.toml to version[C]
        WRITE_CHANGELOG(C)                         # changelog before SYNC_ROOT: case 2 reads it
    for each crate X in severity:
        root_before[X] := current root requirement for X
        root_raised[X] := SYNC_ROOT(X)             # ask the maintainer at most once

    # ---- Phase 4 ----------------------------------------------------------
    VALIDATE()
```

### RAISE — the only way a crate enters the tables

```
RAISE(C, s, reason):
    if s == NONE:                   return      # nothing to do for C
    if C in severity and severity[C] >= s:
        return                                  # no double bump for the same severity;
                                                # nothing new to propagate either.
                                                # NOTE: this return drops nothing the
                                                # changelog needs — the caller already
                                                # recorded the event in dep_events[C].
    severity[C] := s                            # monotonic: severity never decreases
    if C not in dep_events:  dep_events[C] := {}   # every crate in severity has an
                                                  # entry by the time Phase 3 reads it
    log += (C, s, reason)
    push C onto worklist                        # re-entering an already-bumped crate is
                                                # correct and required when its severity rises
```

### DEPENDENTS — do not rely on the diff to surface these

```
DEPENDENTS(X):
    hits := shell: grep -l '^X = ' */Cargo.toml          # e.g. `yash-env = { workspace = true }`
    return { crate(h) for h in hits } \ { X }
```

A version bump of `yash-env`, say, potentially affects *every* crate with
`yash-env = { workspace = true }`, not just the one crate whose source you were
asked to change. Run the grep for **each** crate whose version changed and add
every hit — this is the step most easily skipped, because the code diff usually
only touches one dependent directly.

Notes:

- A crate counts as a dependent whether the dependency is **public or private**.
- `[dependencies]` is what matters for published-crate compatibility; also check
  `[dev-dependencies]` for completeness, though those don't affect it.
- Entering the tables only means the crate *needs attention*; `PROPAGATED_SEVERITY`
  and `FORECAST` decide the severity. Do **not** assume a dependency bump alone
  skips the version bump — public vs. private changes only *severity* and
  *root-requirement* handling, never whether a bump happens at all.

### CLASSIFY — change category of a crate's own change

```
CLASSIFY(C, diff):
    if C is a library crate (all except yash-cli):       # classify by PUBLIC API
        if breaking API change:                     return BREAKING
        if backward-compatible API addition:        return COMPATIBLE
        if no API change (internal only, bug fix):  return PATCH

    if C == "yash-cli":                                  # classify by WHAT USERS GET
        if the release adds something a user can newly invoke
           (a new shell option, built-in, syntax, or the like):
                                                    return COMPATIBLE   # a minor bump
        else:                                       return PATCH
```

For `yash-cli`, *anything else* is **patch-level**: a change or restriction of
existing behavior, a relaxation of it, or a bug fix. **Refining existing behavior
is patch-level**, including each new rejection added under the `portable` option.

The test to apply: does the release *add* something a user can newly invoke?
"Users can now write X" is minor; "the shell now rejects / now reports / now
behaves differently for X" is patch. Precedents: 3.3.0 was minor because it
introduced the `portable` option itself, while versions 3.3.1 through 3.3.4
stayed patch-level despite carrying many new `portable` rejections. 3.4.0 was
minor despite also carrying many such rejections, because it separately added the
`kill -SIGINT` form users can newly write — the rejections did not earn the bump.

`yash-cli` re-exports nothing, so only observable behavior drives its version. A
dependency bump alone never bumps `yash-cli` (see `PROPAGATED_SEVERITY`), and
unlike every other crate, its changelog never records dependency version changes
either (see `WRITE_CHANGELOG`).

### APPLY_BUMP — severity to version number

Every crate in this workspace follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Because most crates
are still pre-1.0, apply **Cargo's 0.x convention** (as documented in each
crate's `CHANGELOG.md` preamble, clarified in commit `9d7a76ba`):

| Crate major version | BREAKING | COMPATIBLE | PATCH |
| ------------------- | -------- | ---------- | ----- |
| `0.y.z` (pre-1.0)   | bump `y` → `0.(y+1).0` | bump `z` → `0.y.(z+1)` | bump `z` → `0.y.(z+1)` |
| `≥ 1.0.0`           | bump major | bump minor | bump patch |

So while the major version is `0`, a breaking change bumps the **minor** `y`
(resetting `z` to 0), and any backward-compatible change — including a bug fix —
bumps the **patch** `z`. Check each crate's current version to pick the right
row; the workspace currently mixes `0.y.z` crates with a few `1.x` ones
(`yash-executor`, `yash-fnmatch`, `yash-quote`).

### FORECAST — the version written to `<crate>/Cargo.toml`

The forecast version is **the latest published release bumped exactly once, by
the most severe change category accumulated across *all* unreleased work** — not
the current change alone, and not one bump per change.

```
FORECAST(C, s_current):
    latest   := newest [x.y.z] release heading in C/CHANGELOG.md   # ignore any Unreleased
    s_prior  := severity already reflected by an existing `[x.y.z] - Unreleased`
                heading relative to `latest`, or NONE if there is no such heading
    s        := max(s_current, s_prior, severity of every other unreleased change)
    return APPLY_BUMP(latest, s)
```

The `[x.y.z] - Unreleased` heading is an **output** of this computation, not an
input: always (re)derive the forecast from latest-release + highest-accumulated
severity, then make the heading and `Cargo.toml` match it. Two consequences:

- **No double bump for the same severity.** If the Unreleased version already
  reflects a bump of the same or higher severity than the current change, the
  recomputation yields the same value — leave it as is. Example: latest release
  `1.2.3`, Unreleased already `1.3.0` (minor), and the current change is another
  backward-compatible one → still `1.3.0`.
- **Raise the forecast when the current change is more severe.** If it outranks
  the bump the Unreleased version currently reflects, the recomputation yields a
  higher value — rewrite both the forecast and the `[x.y.z] - Unreleased`
  heading. Example: latest release `1.2.3`, Unreleased `1.3.0` (minor), and the
  current change is **breaking** → rewrite to `2.0.0`. (In 0.x, the analogous
  case is `0.2.3` → Unreleased `0.2.4` (patch) + breaking change → `0.3.0`.)

### PROPAGATED_SEVERITY — what X's bump does to a dependent D

```
PROPAGATED_SEVERITY(X, D):
    if D == "yash-cli":                                  return NONE
    if severity[X] == BREAKING and IS_PUBLIC_DEP(D, X):  return BREAKING
                                   # ...and re-run CLASSIFY(D); see the note below
    return PATCH                   # "normally" — see the note below
```

For every crate that depends on X, recording X's new version is itself a
changelog-worthy change and — per the `APPLY_BUMP` table, where *compatible* and
*patch-level* both bump `z` by exactly the same amount in 0.x — **normally earns
the dependent its own patch-level (`z`) version bump, regardless of whether the
dependency is public or private.** This holds even when X's bump was itself only
patch-level, and even when nothing else about the dependent changed (real
precedent: `yash-fnmatch` 1.1.1 → 1.1.2 solely for a *private* `thiserror` patch
bump; `yash-syntax`, `yash-semantics`, and `yash-prompt` have each released
solely for a *public* `yash-env` bump). Public vs. private changes two things
only:

- **Which changelog list the entry goes in** — "Public dependency versions" vs
  "Private dependency versions" (`WRITE_CHANGELOG`).
- **Whether a *breaking* upstream bump propagates as breaking.** If the
  dependency is **public** (its items are re-exported) and X's bump was breaking
  (`SYNC_ROOT` case 1, i.e. a minor bump in 0.x), the propagated severity is
  breaking too — re-run `CLASSIFY` for that crate instead of defaulting to
  patch-level. If the dependency is **private**, the propagated severity stays
  capped at patch-level even when X's own bump was breaking, since nothing in the
  dependent's own public API changed.

Whether the *root* requirement for X itself gets raised is a separate question,
decided by `SYNC_ROOT` — do not conflate "does the root requirement rise" with
"does this dependent get a changelog entry and version bump." The two are
independent: a crate can (and often does) get a patch bump purely to record a
dependency note even while the root requirement for that dependency stays put
(`SYNC_ROOT` case 2).

### IS_PUBLIC_DEP — trust the changelog history

Do **not** try to re-derive a dependency's public/private status from the source
each time.

```
IS_PUBLIC_DEP(D, X):
    entry := most recent version in D/CHANGELOG.md that classified X under a
             "Public dependency versions" or "Private dependency versions" list
             (or a line like "X is now a private dependency")
    if entry exists:  return the classification it records
    else:             ASK_MAINTAINER("Is X a public or private dependency of D?")
                      # newly added or never-mentioned dependency: do NOT guess
```

- Example: `yash-prompt`'s `[0.13.0]` lists `yash-env` and `yash-syntax` under
  **Public dependency versions**, so treat both as *public* for the next
  `yash-prompt` release (e.g. `0.13.1` / `0.14.0`) unless the changelog later
  reclassifies them.
- **Do not proactively audit** existing classifications for correctness. But if
  you *incidentally* notice a contradiction or error (e.g. the changelog calls a
  dependency private while the crate clearly re-exports its types), **report it to
  the maintainer** rather than silently fixing or relying on it.

### SYNC_ROOT — the root `Cargo.toml` workspace dependency table

The workspace dependency table in the root
[Cargo.toml](../../../Cargo.toml) (the `yash-* = { path = ..., version = ... }`
lines) holds, for each workspace crate, the **single workspace-wide minimum
version requirement** that all dependents inherit via `workspace = true`. This
table is the **driver**: when you raise a requirement here, every dependent's
required version rises at once, and `WRITE_CHANGELOG` then records that bump in
each affected dependent's changelog mechanically. Decide the root here first;
never let a changelog entry drive the root value (that would be circular).

The `version` in the table is a Cargo **caret** requirement (`"0.2.3"` means
`>=0.2.3, <0.3.0`), and it is intentionally kept at the *lowest* version that all
dependents actually need — it is **not** automatically bumped just because the
crate released a new version.

```
SYNC_ROOT(X) -> raised?:             # returns whether it raised the requirement
    # case 1 — Forced: X's bump is breaking (0.x: a minor bump; 1.x: a major bump)
    if severity[X] == BREAKING:
        root[X] := version[X]            # mechanical; no confirmation needed
        return true

    # case 2 — Internal-only compatible bump: leave the requirement unchanged
    if X's Unreleased changelog entries consist ONLY of
           - a Rust / MSRV version bump, and/or
           - internal fixes that do not touch the public API, and/or
           - private-dependency version bumps:
        return false                     # mechanical; no confirmation needed

    # case 3 — Public-API compatible bump: ask the maintainer (default: leave)
    if X's Unreleased changelog includes any public-API addition or change:
        if ASK_MAINTAINER("Does any dependent now require the new " + X +
                          ", so the workspace requirement should be raised?"):
            root[X] := version[X]
            return true
        else:
            return false                 # default to leaving it unchanged if unconfirmed
```

- Case 1 rationale: the old caret requirement excludes the new version, so
  dependents cannot build against it. **Always raise.**
- Case 2 rationale: no dependent can depend on anything new in X, so **keep the
  root requirement as is**. (A Rust/MSRV bump stays in this case **regardless of
  whether the changelog files it under "Public" or "Private dependency
  versions"** — what matters is that it adds no adoptable API. Example:
  `yash-arith 0.2.4`, whose only Unreleased change is a Rust version bump listed
  under *Public dependency versions*, still leaves the root requirement correctly
  at `0.2.3`.)
- Case 3 rationale: a dependent *could* now adopt the addition, but this skill
  cannot reliably tell from the diff whether one actually did.

### WRITE_CHANGELOG

```
WRITE_CHANGELOG(C):
    ensure C/CHANGELOG.md has an `[version[C]] - Unreleased` heading
    ensure it has an entry describing the change

    # Dependency notes come from dep_events, NOT from reading C/Cargo.toml:
    # a workspace crate's bump usually leaves `X = { workspace = true }` untouched,
    # so the manifest shows nothing even though the entry is required.
    deps := { (X, version[X]) for X in dep_events[C] }   # empty set if C had none
          + any dependency added, removed, or updated in C/Cargo.toml itself
    if deps is non-empty:
        if C == "yash-cli":  skip the dependency entry        # exception, see below
        else:                note each, listing PRIVATE and PUBLIC dependency
                             changes in separate lists (IS_PUBLIC_DEP decides which)
```

Follow the [update-changelog skill](../update-changelog/SKILL.md) for category
choice, wording, net-state representation, and the release-link reference.

Rules specific to this workflow:

- **`yash-cli` always gets an Unreleased heading when shell behavior changes**,
  even if `yash-cli` itself was not otherwise modified. (This is the Phase 1 seed
  rule above.)
- **Dependency changes are mentioned.** If a `Cargo.toml` dependency was added,
  removed, or updated, note it in the changelog. List **private and public**
  dependency changes separately.
- **Exception: `yash-cli` never gets a dependency-change entry.** Its changelog
  header states it documents only observable shell behavior, not the implementing
  library crates' versions — so even though a dependency's bump would otherwise
  make `yash-cli` "affected", skip it here unless the same change also has
  user-visible behavior to record under the rule above.

> **Documentation is out of scope here.** If the change is user-visible, the
> `docs/src` pages (and the "introduced in `yash-cli` x.y.z" version mention)
> still need updating — do that separately with the
> [update-docs skill](../update-docs/SKILL.md). This skill stops at versions and
> changelogs.

### VALIDATE

Run every assertion. A failed assertion means go back to the phase named in the
comment, not "fix it up locally".

```
VALIDATE():
    # A. Was the Phase 2 fixpoint really reached? Re-derive it from scratch.
    for each crate X in severity:
        for each D in DEPENDENTS(X):
            p := PROPAGATED_SEVERITY(X, D)
            if p == NONE:  continue                  # only yash-cli
            assert D in severity and severity[D] >= p
                   # membership alone is NOT enough: a public dependent of a
                   # BREAKING X sitting at PATCH is under-bumped and must fail here
            assert X in dep_events[D]                # D must record X's new version
            # a failure here is THE missed-bump bug: go back to Phase 2

    # B. Per-crate consistency
    for each crate C in severity:
        assert `version` in C/Cargo.toml == version[C]
        assert C/CHANGELOG.md has a `[version[C]] - Unreleased` heading
               # same version in both places
        assert every entry THIS RUN added under that heading is backed by either
               a change in the diff or an entry in dep_events[C]
               # scope matters: entries left by earlier commits in the same
               # unreleased cycle are legitimate (FORECAST accounts for them), and
               # propagated dependency notes are by construction absent from the
               # diff — neither is an invented entry

    # C. Root table — a gap here is often CORRECT
    for each crate X in severity:                    # read the Phase 3 outcome;
        if root_raised[X]:                           # never call SYNC_ROOT again —
            assert root entry for X == version[X]    # it is side-effecting and case 3
        else:                                        # would re-prompt the maintainer
            assert root entry for X == root_before[X]
            # the root deliberately stays at the lower caret minimum;
            # do NOT "fix" that gap (e.g. yash-arith crate 0.2.4 with root
            # requirement 0.2.3 is correct under SYNC_ROOT case 2)

    # D. Build artifacts and checks
    run `cargo build` / `cargo test`      # regenerates Cargo.lock
    assert Cargo.lock is consistent with the manifests
           and is included in the same commit as the code change and its bumps
    run `./check.sh`        ; assert it passes
    run `./check-semver.sh` ; assert it passes        # ALWAYS, every invocation
```

**Always run `./check-semver.sh`.** It runs `cargo semver-checks` over every
library crate and confirms each crate's version bump is consistent with its
actual public-API changes — the authoritative backstop for the
`CLASSIFY`/`FORECAST` classification. Run it whenever this skill changed any
version, which is every invocation; do not treat it as optional.

Know its limits, though — passing it does **not** prove the versions are fully
correct:

- It checks **library crates' public API surface only**.
- It does **not** cover `yash-cli` (a binary); its version, driven by observable
  shell behavior, still rests on the `CLASSIFY` judgment.
- It cannot catch a behavioral change that leaves the API unchanged, so the
  judgment behind a compatible (patch-level) bump is not validated here.

## Limits

- Do not bump a version that was already bumped to a sufficient severity for the
  current unreleased cycle (`RAISE` / `FORECAST` enforce this).
- Do not invent changelog entries for changes not present in the diff.
- Do not edit `docs/src`; defer documentation to the update-docs skill.
